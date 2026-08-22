#!/usr/bin/env python3
"""Build and stage the Windows Rils C API native and managed libraries."""

from __future__ import annotations

import argparse
import platform
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--debug",
        action="store_true",
        help="build the debug profile instead of release",
    )
    args = parser.parse_args()

    if platform.system() != "Windows":
        raise SystemExit(
            "The current C API packaging script only supports Windows DLL builds."
        )

    repository_root = Path(__file__).resolve().parent.parent
    subprocess.run(
        [sys.executable, str(repository_root / "tools" / "generate-csharp-bindings.py")],
        cwd=repository_root,
        check=True,
    )
    command = [
        "cargo",
        "build",
        "-p",
        "rils_capi",
        "--manifest-path",
        str(repository_root / "Cargo.toml"),
    ]
    if not args.debug:
        command.append("--release")
    subprocess.run(command, cwd=repository_root, check=True)

    configuration = "Debug" if args.debug else "Release"
    subprocess.run(
        [
            "dotnet",
            "build",
            str(
                repository_root
                / "tools"
                / "rils-capi"
                / "csharp"
                / "Rils.CSharp"
                / "Rils.CSharp.csproj"
            ),
            "--configuration",
            configuration,
        ],
        cwd=repository_root,
        check=True,
    )

    profile = "debug" if args.debug else "release"
    output_directory = repository_root / "tools" / "rils-capi" / "dist" / "win-x64"
    output_directory.mkdir(parents=True, exist_ok=True)
    for legacy_name in (
        "rils_unity.dll",
        "rils_unity.h",
        "rils.h",
        "Rils.CApi.dll",
    ):
        (output_directory / legacy_name).unlink(missing_ok=True)

    artifacts = (
        repository_root / "target" / profile / "rils_capi.dll",
        repository_root
        / "tools"
        / "rils-capi"
        / "csharp"
        / "Rils.CSharp"
        / "bin"
        / configuration
        / "netstandard2.1"
        / "Rils.CSharp.dll",
    )
    for artifact in artifacts:
        destination = output_directory / artifact.name
        shutil.copy2(artifact, destination)
        print(f"{destination.relative_to(repository_root)} ({destination.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
