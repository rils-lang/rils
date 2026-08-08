#!/usr/bin/env python3
"""Build Rils release artifacts and optionally publish its crates."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
RELEASE_PACKAGES = ("rils_frontend", "rils_compiler", "rils")
WORKSPACE_PACKAGES = (*RELEASE_PACKAGES, "rils_analyzer")


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


def assert_clean_worktree(git: str) -> None:
    result = run(
        [git, "status", "--porcelain", "--untracked-files=all"],
        capture_output=True,
    )
    if result.stdout.strip():
        raise RuntimeError(
            "The Git working tree is not clean. Commit or stash changes, "
            "or use --allow-dirty for a local trial run."
        )


def workspace_metadata(cargo: str) -> dict[str, object]:
    result = run(
        [cargo, "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
    )
    return json.loads(result.stdout)


def workspace_version(metadata: dict[str, object]) -> str:
    packages = {
        package["name"]: package["version"]
        for package in metadata["packages"]  # type: ignore[index]
        if package["name"] in WORKSPACE_PACKAGES
    }
    missing = sorted(set(WORKSPACE_PACKAGES) - packages.keys())
    if missing:
        raise RuntimeError(f"Cargo workspace packages are missing: {', '.join(missing)}")

    versions = set(packages.values())
    if len(versions) != 1:
        summary = ", ".join(f"{name}={version}" for name, version in packages.items())
        raise RuntimeError(f"Workspace package versions do not match: {summary}")
    return str(versions.pop())


def wait_for_crate(cargo: str, package: str, version: str) -> None:
    maximum_attempts = 30
    for attempt in range(1, maximum_attempts + 1):
        result = subprocess.run(
            [cargo, "info", f"{package}@{version}"],
            cwd=REPOSITORY_ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode == 0:
            return
        if attempt < maximum_attempts:
            print(
                f"Waiting for {package} v{version} to become available on "
                f"crates.io ({attempt}/{maximum_attempts})...",
                flush=True,
            )
            time.sleep(10)
    raise RuntimeError(
        f"{package} v{version} did not become available on crates.io within five minutes."
    )


def copy_binary(target_directory: Path, artifact_directory: Path, name: str) -> None:
    suffix = ".exe" if os.name == "nt" else ""
    source = target_directory / "release" / f"{name}{suffix}"
    if not source.is_file():
        raise RuntimeError(f"Expected release artifact was not created: {source}")
    shutil.copy2(source, artifact_directory / source.name)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow uncommitted files for a local trial run",
    )
    parser.add_argument(
        "--publish",
        action="store_true",
        help="publish rils_frontend, rils_compiler, and rils to crates.io",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    if args.allow_dirty and args.publish:
        raise RuntimeError("--allow-dirty cannot be combined with --publish")

    cargo = command_path("cargo")
    git = command_path("git")
    if not args.allow_dirty:
        assert_clean_worktree(git)

    metadata = workspace_metadata(cargo)
    version = workspace_version(metadata)
    target_directory = Path(str(metadata["target_directory"]))
    artifact_directory = target_directory / "release-artifacts" / f"v{version}"
    artifact_directory.mkdir(parents=True, exist_ok=True)

    print(f"Preparing Rils v{version}")
    print(f"Artifacts: {artifact_directory}")

    run([cargo, "fmt", "--check"])
    run([cargo, "test", "--workspace"])
    run([cargo, "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
    run([cargo, "build", "--release", "-p", "rils", "-p", "rils_analyzer"])

    copy_binary(target_directory, artifact_directory, "rils")
    copy_binary(target_directory, artifact_directory, "rils-analyzer")

    if args.publish:
        for index, package in enumerate(RELEASE_PACKAGES):
            run([cargo, "publish", "-p", package])
            if index < len(RELEASE_PACKAGES) - 1:
                wait_for_crate(cargo, package, version)

    print("Rils release completed successfully:")
    for artifact in sorted(artifact_directory.iterdir()):
        if artifact.is_file() and artifact.name in {
            "rils",
            "rils.exe",
            "rils-analyzer",
            "rils-analyzer.exe",
        }:
            print(f"  {artifact.name}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
