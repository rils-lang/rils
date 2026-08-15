#!/usr/bin/env python3
"""Build Rils release artifacts and optionally publish its crates."""

from __future__ import annotations

import argparse
from collections import defaultdict
import heapq
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
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


def publishable_packages(metadata: dict[str, object]) -> dict[str, dict[str, object]]:
    packages = metadata.get("packages")
    if not isinstance(packages, list):
        raise RuntimeError("cargo metadata did not return a package list")
    publishable = {
        str(package["name"]): package
        for package in packages
        if isinstance(package, dict) and package.get("publish")
    }
    if not publishable:
        raise RuntimeError("workspace has no publishable packages")
    return publishable


def workspace_version(packages: dict[str, dict[str, object]]) -> str:
    versions = {str(package["version"]) for package in packages.values()}
    if len(versions) != 1:
        summary = ", ".join(f"{name}={package['version']}" for name, package in packages.items())
        raise RuntimeError(f"Workspace package versions do not match: {summary}")
    return str(versions.pop())


def publish_order(packages: dict[str, dict[str, object]]) -> list[str]:
    """Return a deterministic dependency-first order for publishable crates."""
    dependents: dict[str, set[str]] = defaultdict(set)
    indegree = {name: 0 for name in packages}
    for name, package in packages.items():
        dependencies = package.get("dependencies", [])
        if not isinstance(dependencies, list):
            continue
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                continue
            dependency_name = dependency.get("name")
            if dependency_name not in packages or dependency.get("path") is None:
                continue
            dependents[str(dependency_name)].add(name)
            indegree[name] += 1

    ready = [name for name, degree in indegree.items() if degree == 0]
    heapq.heapify(ready)
    order = []
    while ready:
        name = heapq.heappop(ready)
        order.append(name)
        for dependent in sorted(dependents[name]):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                heapq.heappush(ready, dependent)
    if len(order) != len(packages):
        cyclic = sorted(name for name, degree in indegree.items() if degree > 0)
        raise RuntimeError(
            "publishable workspace packages contain a dependency cycle: "
            + ", ".join(cyclic)
        )
    return order


def crate_is_available(package: str, version: str) -> bool:
    request = Request(
        f"https://crates.io/api/v1/crates/{package}/{version}",
        headers={
            "User-Agent": "rils-release-script/0.1 "
            "(https://github.com/rils-lang/rils)"
        },
    )
    try:
        with urlopen(request, timeout=15) as response:
            return response.status == 200
    except HTTPError as error:
        if error.code == 404:
            return False
        raise RuntimeError(
            f"crates.io returned HTTP {error.code} while checking "
            f"{package} v{version}"
        ) from error
    except URLError as error:
        raise RuntimeError(
            f"Could not check {package} v{version} on crates.io: {error.reason}"
        ) from error


def wait_for_crate(package: str, version: str) -> None:
    maximum_attempts = 30
    for attempt in range(1, maximum_attempts + 1):
        if crate_is_available(package, version):
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
        help="publish all workspace packages with publish metadata to crates.io",
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
    packages = publishable_packages(metadata)
    order = publish_order(packages)
    version = workspace_version(packages)
    target_directory = Path(str(metadata["target_directory"]))
    artifact_directory = target_directory / "release-artifacts" / f"v{version}"
    if artifact_directory.exists():
        shutil.rmtree(artifact_directory)
    artifact_directory.mkdir(parents=True, exist_ok=True)

    print(f"Preparing Rils v{version}")
    print(f"Publish order: {' -> '.join(order)}")
    print(f"Artifacts: {artifact_directory}")

    run([cargo, "fmt", "--check"])
    run([cargo, "test", "--workspace"])
    run([cargo, "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
    run([cargo, "build", "--release", "-p", "rils", "-p", "rils_analyzer"])

    copy_binary(target_directory, artifact_directory, "rils")
    copy_binary(target_directory, artifact_directory, "rils-analyzer")

    if args.publish:
        for index, package in enumerate(order):
            if crate_is_available(package, version):
                print(f"Skipping already published {package} v{version}", flush=True)
            else:
                run([cargo, "publish", "-p", package])
            if index < len(order) - 1:
                wait_for_crate(package, version)

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
