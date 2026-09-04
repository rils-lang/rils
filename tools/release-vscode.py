#!/usr/bin/env python3
"""Validate, package, and optionally publish the Rils VS Code extension."""

from __future__ import annotations

import argparse
import json
import os
import platform
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
EXTENSION_ROOT = REPOSITORY_ROOT / "editors" / "vscode-rils"
PLATFORM_TARGETS = {
    "win32-x64": ("x86_64-pc-windows-msvc", "rils-analyzer.exe"),
    "win32-arm64": ("aarch64-pc-windows-msvc", "rils-analyzer.exe"),
    "linux-x64": ("x86_64-unknown-linux-gnu", "rils-analyzer"),
    "linux-arm64": ("aarch64-unknown-linux-gnu", "rils-analyzer"),
    "linux-armhf": ("armv7-unknown-linux-gnueabihf", "rils-analyzer"),
    "alpine-x64": ("x86_64-unknown-linux-musl", "rils-analyzer"),
    "alpine-arm64": ("aarch64-unknown-linux-musl", "rils-analyzer"),
    "darwin-x64": ("x86_64-apple-darwin", "rils-analyzer"),
    "darwin-arm64": ("aarch64-apple-darwin", "rils-analyzer"),
}
PREVIEW_VERSION_PATTERN = re.compile(
    r"^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)"
    r"-(?:[0-9A-Za-z-]+)(?:\.[0-9A-Za-z-]+)*$"
)


def command_prefix(name: str) -> list[str]:
    path = shutil.which(name)
    if path is None:
        raise RuntimeError(f"Required command was not found on PATH: {name}")
    if os.name == "nt" and Path(path).suffix.lower() in {".bat", ".cmd"}:
        command_interpreter = os.environ.get("COMSPEC", "cmd.exe")
        return [command_interpreter, "/d", "/c", path]
    return [path]


def run(
    command: list[str],
    *,
    cwd: Path = REPOSITORY_ROOT,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    print("+", subprocess.list2cmdline(command), flush=True)
    return subprocess.run(
        command,
        cwd=cwd,
        check=True,
        capture_output=capture_output,
        text=True,
    )


def assert_clean_worktree(git: list[str]) -> None:
    result = run(
        [*git, "status", "--porcelain", "--untracked-files=all"],
        capture_output=True,
    )
    if result.stdout.strip():
        raise RuntimeError(
            "The Git working tree is not clean. Commit or stash changes, "
            "or use --allow-dirty for a local trial run."
        )


def cargo_version_and_target(cargo: list[str]) -> tuple[str, Path]:
    result = run(
        [*cargo, "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
    )
    metadata = json.loads(result.stdout)
    root_package = next(
        (package for package in metadata["packages"] if package["name"] == "rils"),
        None,
    )
    if root_package is None:
        raise RuntimeError("The rils package is missing from the Cargo workspace")
    return str(root_package["version"]), Path(metadata["target_directory"])


def detect_platform_target() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    architectures = {
        "amd64": "x64",
        "x86_64": "x64",
        "arm64": "arm64",
        "aarch64": "arm64",
        "armv7l": "armhf",
        "armv7": "armhf",
    }
    architecture = architectures.get(machine)
    systems = {"windows": "win32", "darwin": "darwin", "linux": "linux"}
    target_system = systems.get(system)
    if target_system is None or architecture is None:
        raise RuntimeError(
            f"Cannot detect a supported VS Code target for {system}/{machine}; "
            "pass --target explicitly"
        )
    target = f"{target_system}-{architecture}"
    if target not in PLATFORM_TARGETS:
        raise RuntimeError(
            f"The detected VS Code target is not supported: {target}; "
            "pass --target explicitly"
        )
    return target


def verify_bundled_analyzer(package_path: Path, executable: str) -> None:
    expected_entry = f"extension/server/{executable}"
    with zipfile.ZipFile(package_path) as archive:
        if expected_entry not in archive.namelist():
            raise RuntimeError(
                f"VSIX does not contain the bundled analyzer: {expected_entry}"
            )


def stage_preview_extension(version: str) -> tempfile.TemporaryDirectory[str]:
    """Copy the extension and override only the staged package metadata."""
    staging = tempfile.TemporaryDirectory(prefix="rils-vscode-preview-")
    staged_root = Path(staging.name) / "vscode-rils"
    shutil.copytree(
        EXTENSION_ROOT,
        staged_root,
        ignore=shutil.ignore_patterns("dist", "node_modules", "server", ".vscode"),
    )
    manifest_path = staged_root / "package.json"
    lockfile_path = staged_root / "package-lock.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    lockfile = json.loads(lockfile_path.read_text(encoding="utf-8"))
    manifest["version"] = version
    # The source extension was already bundled and checked before staging. Do
    # not run vscode:prepublish again here: node_modules is deliberately not
    # copied into the temporary directory.
    manifest.get("scripts", {}).pop("vscode:prepublish", None)
    lockfile["version"] = version
    lockfile["packages"][""]["version"] = version
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    lockfile_path.write_text(json.dumps(lockfile, indent=2) + "\n", encoding="utf-8")
    return staging


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow uncommitted files for a local trial run",
    )
    parser.add_argument(
        "--skip-install",
        action="store_true",
        help="reuse the existing node_modules directory instead of running npm ci",
    )
    parser.add_argument(
        "--publish",
        action="store_true",
        help="publish the generated VSIX to the Visual Studio Marketplace",
    )
    parser.add_argument(
        "--preview-version",
        help=(
            "package a local prerelease VSIX using this SemVer version without "
            "changing tracked manifests; cannot be combined with --publish"
        ),
    )
    parser.add_argument(
        "--target",
        choices=sorted(PLATFORM_TARGETS),
        help="VS Code platform target (defaults to the current platform)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    if args.allow_dirty and args.publish:
        raise RuntimeError("--allow-dirty cannot be combined with --publish")
    if args.preview_version and args.publish:
        raise RuntimeError("--preview-version cannot be combined with --publish")
    if args.preview_version and not PREVIEW_VERSION_PATTERN.fullmatch(
        args.preview_version
    ):
        raise RuntimeError(
            "--preview-version must be a SemVer prerelease, for example "
            "0.4.0-preview.0"
        )

    cargo = command_prefix("cargo")
    git = command_prefix("git")
    npm = command_prefix("npm")
    vsce = command_prefix("vsce")

    if not args.allow_dirty:
        assert_clean_worktree(git)

    manifest_path = EXTENSION_ROOT / "package.json"
    repository_license = REPOSITORY_ROOT / "LICENSE"
    if not manifest_path.is_file():
        raise RuntimeError(f"VS Code extension manifest not found: {manifest_path}")
    if not repository_license.is_file():
        raise RuntimeError(f"Repository license not found: {repository_license}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    workspace_version, target_directory = cargo_version_and_target(cargo)
    platform_target = args.target or detect_platform_target()
    rust_target, analyzer_executable = PLATFORM_TARGETS[platform_target]
    extension_version = args.preview_version or str(manifest["version"])
    if args.preview_version is None and extension_version != workspace_version:
        raise RuntimeError(
            f"VS Code extension version {extension_version} does not match "
            f"workspace version {workspace_version}"
        )

    package_name = f"{manifest['name']}-{extension_version}-{platform_target}.vsix"
    dist_directory = EXTENSION_ROOT / "dist"
    package_path = dist_directory / package_name
    artifact_directory = target_directory / "release-artifacts" / f"v{extension_version}"
    dist_directory.mkdir(parents=True, exist_ok=True)
    artifact_directory.mkdir(parents=True, exist_ok=True)

    print(
        f"Preparing {'preview ' if args.preview_version else ''}Rils VS Code extension v{extension_version} "
        f"for {platform_target}"
    )
    run(
        [
            *cargo,
            "build",
            "--release",
            "--package",
            "rils_analyzer",
            "--target",
            rust_target,
        ]
    )
    analyzer_path = (
        target_directory / rust_target / "release" / analyzer_executable
    )
    if not analyzer_path.is_file():
        raise RuntimeError(f"Analyzer executable was not created: {analyzer_path}")

    if not args.skip_install:
        run([*npm, "ci"], cwd=EXTENSION_ROOT)
    run([*npm, "run", "check"], cwd=EXTENSION_ROOT)

    preview_staging = (
        stage_preview_extension(extension_version) if args.preview_version else None
    )
    package_root = (
        Path(preview_staging.name) / "vscode-rils"
        if preview_staging is not None
        else EXTENSION_ROOT
    )
    try:
        extension_license = package_root / "LICENSE"
        server_directory = package_root / "server"
        if server_directory.exists():
            raise RuntimeError(
                f"Temporary analyzer staging directory already exists: {server_directory}"
            )
        temporary_license = not extension_license.exists()
        server_directory.mkdir()
        try:
            if temporary_license:
                shutil.copy2(repository_license, extension_license)
            shutil.copy2(analyzer_path, server_directory / analyzer_executable)
            package_command = [
                *vsce,
                "package",
                "--target",
                platform_target,
                "--out",
                str(package_path),
            ]
            if preview_staging is not None:
                # extension.js is bundled by esbuild before staging, and the
                # staging directory intentionally has no node_modules tree.
                package_command.append("--no-dependencies")
            run(package_command, cwd=package_root)
            if not package_path.is_file():
                raise RuntimeError(f"VSIX package was not created: {package_path}")
            verify_bundled_analyzer(package_path, analyzer_executable)
        finally:
            shutil.rmtree(server_directory)
            if temporary_license:
                extension_license.unlink(missing_ok=True)
    finally:
        if preview_staging is not None:
            preview_staging.cleanup()

    shutil.copy2(package_path, artifact_directory / package_name)

    if args.publish:
        run([*vsce, "publish", "--packagePath", str(package_path)], cwd=EXTENSION_ROOT)

    print("VS Code extension release completed successfully:")
    print(f"  {package_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
