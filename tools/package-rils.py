#!/usr/bin/env python3
"""Build a self-contained Rils command-line package for one platform."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import tarfile
import uuid
import zipfile


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
PACKAGE_TARGETS = {
    "x86_64-pc-windows-msvc": ("windows-x86_64", ".exe", "zip"),
    "aarch64-pc-windows-msvc": ("windows-aarch64", ".exe", "zip"),
    "x86_64-unknown-linux-gnu": ("linux-x86_64", "", "tar.gz"),
    "aarch64-unknown-linux-gnu": ("linux-aarch64", "", "tar.gz"),
    "x86_64-apple-darwin": ("macos-x86_64", "", "tar.gz"),
    "aarch64-apple-darwin": ("macos-aarch64", "", "tar.gz"),
}
INSTALLER_MAGIC = b"RILS-INSTALL-V1!"
INSTALLER_VERSION_BYTES = 64


def command_path(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise RuntimeError(f"Required command was not found on PATH: {name}")
    return path


def run(command: list[str], *, capture_output: bool = False) -> subprocess.CompletedProcess[str]:
    print("+", subprocess.list2cmdline(command), flush=True)
    return subprocess.run(
        command,
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=capture_output,
        text=True,
    )


def cargo_metadata(cargo: str) -> tuple[str, str, Path]:
    result = run(
        [cargo, "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
    )
    metadata = json.loads(result.stdout)
    root_package = next(
        (package for package in metadata["packages"] if package["name"] == "rils"),
        None,
    )
    if root_package is None:
        raise RuntimeError("The rils package is missing from the Cargo workspace")
    manager_package = next(
        (package for package in metadata["packages"] if package["name"] == "rils_up"),
        None,
    )
    if manager_package is None:
        raise RuntimeError("The rils_up package is missing from the Cargo workspace")
    return (
        str(root_package["version"]),
        str(manager_package["version"]),
        Path(metadata["target_directory"]),
    )


def host_rust_target(rustc: str) -> str:
    result = run([rustc, "-vV"], capture_output=True)
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise RuntimeError("rustc -vV did not report a host target")


def fallback_host_target() -> str:
    systems = {"windows": "pc-windows-msvc", "linux": "unknown-linux-gnu"}
    machines = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }
    system = platform.system().lower()
    machine = platform.machine().lower()
    if system == "darwin" and machine in machines:
        return f"{machines[machine]}-apple-darwin"
    if system in systems and machine in machines:
        return f"{machines[machine]}-{systems[system]}"
    raise RuntimeError(
        f"Cannot detect a supported package target for {system}/{machine}; "
        "pass --target explicitly"
    )


def resolve_target(explicit_target: str | None, rustc: str | None) -> str:
    target = explicit_target
    if target is None:
        target = host_rust_target(rustc) if rustc is not None else fallback_host_target()
    if target not in PACKAGE_TARGETS:
        raise RuntimeError(
            f"Unsupported package target: {target}; supported targets: "
            + ", ".join(sorted(PACKAGE_TARGETS))
        )
    return target


def copy_package_contents(staging_root: Path, binary_directory: Path, suffix: str) -> None:
    bin_directory = staging_root / "bin"
    bin_directory.mkdir(parents=True)
    for name in ("rils", "rils-analyzer"):
        source = binary_directory / f"{name}{suffix}"
        if not source.is_file():
            raise RuntimeError(f"Expected release binary was not created: {source}")
        destination = bin_directory / source.name
        shutil.copy2(source, destination)
        if not suffix:
            destination.chmod(destination.stat().st_mode | 0o111)

    for name in ("README.md", "LICENSE", "CHANGELOG.md"):
        source = REPOSITORY_ROOT / name
        if not source.is_file():
            raise RuntimeError(f"Required package file is missing: {source}")
        shutil.copy2(source, staging_root / name)
    shutil.copytree(REPOSITORY_ROOT / "docs", staging_root / "docs")
    shutil.copytree(REPOSITORY_ROOT / "examples", staging_root / "examples")


def create_archive(staging_root: Path, output_directory: Path, archive_format: str) -> Path:
    if archive_format == "zip":
        archive_path = output_directory / f"{staging_root.name}.zip"
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(staging_root.rglob("*")):
                if path.is_file():
                    archive_name = path.relative_to(staging_root.parent).as_posix()
                    entry = zipfile.ZipInfo(
                        archive_name, date_time=(1980, 1, 1, 0, 0, 0)
                    )
                    entry.compress_type = zipfile.ZIP_DEFLATED
                    mode = 0o755 if path.parent.name == "bin" else 0o644
                    entry.external_attr = mode << 16
                    with path.open("rb") as source, archive.open(entry, "w") as destination:
                        shutil.copyfileobj(source, destination)
        return archive_path

    archive_path = output_directory / f"{staging_root.name}.tar.gz"
    paths = [staging_root, *sorted(staging_root.rglob("*"))]
    with archive_path.open("wb") as raw_archive:
        with gzip.GzipFile(fileobj=raw_archive, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path in paths:
                    archive_name = path.relative_to(staging_root.parent).as_posix()
                    entry = archive.gettarinfo(str(path), arcname=archive_name)
                    entry.uid = 0
                    entry.gid = 0
                    entry.uname = ""
                    entry.gname = ""
                    entry.mtime = 0
                    if path.is_file():
                        with path.open("rb") as source:
                            archive.addfile(entry, source)
                    else:
                        archive.addfile(entry)
    return archive_path


def write_checksum(archive_path: Path) -> Path:
    digest = hashlib.sha256()
    with archive_path.open("rb") as archive:
        for chunk in iter(lambda: archive.read(1024 * 1024), b""):
            digest.update(chunk)
    checksum_path = archive_path.with_name(f"{archive_path.name}.sha256")
    checksum_path.write_text(
        f"{digest.hexdigest()}  {archive_path.name}\n", encoding="utf-8"
    )
    return checksum_path


def copy_manager(
    binary_directory: Path,
    output_directory: Path,
    package_platform: str,
    executable_suffix: str,
    manager_version: str,
) -> tuple[Path, Path]:
    manager_suffix = ".exe" if executable_suffix else ""
    manager_path = output_directory / (
        f"rils-up-{manager_version}-{package_platform}{manager_suffix}"
    )
    shutil.copy2(binary_directory / f"rils-up{executable_suffix}", manager_path)
    if not executable_suffix:
        manager_path.chmod(manager_path.stat().st_mode | 0o111)
    return manager_path, write_checksum(manager_path)


def create_installer(
    manager_binary: Path,
    archive_path: Path,
    output_directory: Path,
    version: str,
    package_platform: str,
    executable_suffix: str,
) -> tuple[Path, Path]:
    version_bytes = version.encode("utf-8")
    if len(version_bytes) > INSTALLER_VERSION_BYTES:
        raise RuntimeError("Rils version is too long for the installer footer")
    installer_suffix = ".exe" if executable_suffix else ""
    installer_path = output_directory / (
        f"rils-installer-{version}-{package_platform}{installer_suffix}"
    )
    digest = hashlib.sha256()
    payload_length = archive_path.stat().st_size
    with installer_path.open("wb") as installer:
        with manager_binary.open("rb") as manager:
            shutil.copyfileobj(manager, installer)
        with archive_path.open("rb") as archive:
            for chunk in iter(lambda: archive.read(1024 * 1024), b""):
                digest.update(chunk)
                installer.write(chunk)
        installer.write(struct.pack("<Q", payload_length))
        installer.write(digest.digest())
        installer.write(version_bytes.ljust(INSTALLER_VERSION_BYTES, b"\0"))
        installer.write(INSTALLER_MAGIC)
    if not executable_suffix:
        installer_path.chmod(installer_path.stat().st_mode | 0o111)
    return installer_path, write_checksum(installer_path)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        choices=sorted(PACKAGE_TARGETS),
        help="Rust target triple (defaults to the current host target)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="artifact directory (defaults to target/release-artifacts/v<version>)",
    )
    parser.add_argument(
        "--expected-version",
        help="fail unless the workspace version matches this value",
    )
    parser.add_argument(
        "--expected-manager-version",
        help="fail unless the rils-up version matches this value",
    )
    parser.add_argument(
        "--manager-only",
        action="store_true",
        help="build only the independently versioned rils-up asset",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="package binaries already present in the Cargo target directory",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    cargo = command_path("cargo")
    rustc = None if args.target is not None else command_path("rustc")
    version, manager_version, cargo_target_directory = cargo_metadata(cargo)
    if args.expected_version is not None and version != args.expected_version:
        raise RuntimeError(
            f"Workspace version {version} does not match expected version "
            f"{args.expected_version}"
        )
    if (
        args.expected_manager_version is not None
        and manager_version != args.expected_manager_version
    ):
        raise RuntimeError(
            f"rils-up version {manager_version} does not match expected version "
            f"{args.expected_manager_version}"
        )

    target = resolve_target(args.target, rustc)
    package_platform, executable_suffix, archive_format = PACKAGE_TARGETS[target]
    binary_directory = cargo_target_directory / target / "release"
    if not args.skip_build:
        packages = ["rils_up"] if args.manager_only else [
            "rils_cli",
            "rils_analyzer",
            "rils_up",
        ]
        command = [cargo, "build", "--locked", "--release", "--target", target]
        for package in packages:
            command.extend(["-p", package])
        run(command)

    output_directory = args.output_dir
    if output_directory is None:
        release_name = (
            f"rils-up-v{manager_version}" if args.manager_only else f"v{version}"
        )
        output_directory = cargo_target_directory / "release-artifacts" / release_name
    elif not output_directory.is_absolute():
        output_directory = REPOSITORY_ROOT / output_directory
    output_directory.mkdir(parents=True, exist_ok=True)

    manager_path, manager_checksum_path = copy_manager(
        binary_directory,
        output_directory,
        package_platform,
        executable_suffix,
        manager_version,
    )
    if args.manager_only:
        print("rils-up package completed successfully:")
        print(f"  {manager_path}")
        print(f"  {manager_checksum_path}")
        return 0

    package_name = f"rils-{version}-{package_platform}"
    temporary_directory = output_directory / f".rils-package-{uuid.uuid4().hex}"
    temporary_directory.mkdir()
    try:
        staging_root = temporary_directory / package_name
        staging_root.mkdir()
        copy_package_contents(staging_root, binary_directory, executable_suffix)
        archive_path = create_archive(staging_root, output_directory, archive_format)
    finally:
        shutil.rmtree(temporary_directory)
    checksum_path = write_checksum(archive_path)
    installer_path, installer_checksum_path = create_installer(
        binary_directory / f"rils-up{executable_suffix}",
        archive_path,
        output_directory,
        version,
        package_platform,
        executable_suffix,
    )
    print("Rils package completed successfully:")
    print(f"  {archive_path}")
    print(f"  {checksum_path}")
    print(f"  {manager_path}")
    print(f"  {manager_checksum_path}")
    print(f"  {installer_path}")
    print(f"  {installer_checksum_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
