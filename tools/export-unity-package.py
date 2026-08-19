#!/usr/bin/env python3
"""Export the Unity runtime facade and native library into RilsForUnity."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import uuid
from pathlib import Path


def resolve_output(repository_root: Path, value: str | None) -> Path:
    output = (
        Path(value)
        if value is not None
        else repository_root
        / "integrations"
        / "RilsForUnity"
        / "Packages"
        / "com.rils-lang.rils-for-unity"
        / "Runtime"
        / "Rils.CSharp"
    )
    if not output.is_absolute():
        output = repository_root / output
    output = output.resolve()

    if output.name != "Rils.CSharp":
        raise SystemExit("the output directory must be named Rils.CSharp")

    repository_root = repository_root.resolve()
    source_directory = (
        repository_root / "crates" / "rils_capi" / "csharp" / "Rils.CSharp"
    ).resolve()
    if repository_root == output or repository_root.is_relative_to(output):
        raise SystemExit(f"refusing to replace a repository ancestor: {output}")
    if (
        source_directory == output
        or source_directory.is_relative_to(output)
        or output.is_relative_to(source_directory)
    ):
        raise SystemExit(f"refusing to overlap the C# source directory: {output}")
    return output


def collect_sources(source_directory: Path) -> list[Path]:
    sources = sorted(source_directory.glob("*.cs"))
    sources.extend(sorted((source_directory / "Generated").glob("*.cs")))
    if not sources:
        raise SystemExit(f"no C# sources found under {source_directory}")

    names: set[str] = set()
    for source in sources:
        if source.name in names:
            raise SystemExit(f"duplicate flattened C# source name: {source.name}")
        names.add(source.name)
    return sources


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        help=(
            "destination Rils.CSharp directory; defaults to "
            "integrations/RilsForUnity/Packages/com.rils-lang.rils-for-unity/"
            "Runtime/Rils.CSharp"
        ),
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="export a debug native library instead of a release build",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="reuse the existing crates/rils_capi/dist/win-x64/rils_capi.dll",
    )
    args = parser.parse_args()
    if args.debug and args.skip_build:
        parser.error("--debug cannot be combined with --skip-build")

    repository_root = Path(__file__).resolve().parent.parent
    output_directory = resolve_output(repository_root, args.output)
    source_directory = (
        repository_root / "crates" / "rils_capi" / "csharp" / "Rils.CSharp"
    )
    native_library = (
        repository_root
        / "crates"
        / "rils_capi"
        / "dist"
        / "win-x64"
        / "rils_capi.dll"
    )

    if not args.skip_build:
        command = [sys.executable, str(repository_root / "tools" / "build-capi.py")]
        if args.debug:
            command.append("--debug")
        subprocess.run(command, cwd=repository_root, check=True)
    if not native_library.is_file():
        raise SystemExit(
            f"missing native library: {native_library}; run without --skip-build first"
        )

    sources = collect_sources(source_directory)
    asmdef = source_directory / "Rils.CSharp.asmdef"
    if not asmdef.is_file():
        raise SystemExit(f"missing Unity assembly definition: {asmdef}")

    output_directory.parent.mkdir(parents=True, exist_ok=True)
    # tempfile.mkdtemp applies a private Windows ACL. Renaming that directory
    # into the Unity package keeps the restrictive ACL and can make the export
    # unreadable to Unity or sandboxed tooling. A normal directory inherits the
    # package parent's ACL while the random name still prevents collisions.
    staging = output_directory.parent / f".rils-csharp-unity-{uuid.uuid4().hex}"
    staging.mkdir()
    preserved_meta: dict[Path, Path] = {}
    if output_directory.is_dir():
        generated_files = {
            source.name for source in sources
        } | {asmdef.name, native_library.name}
        for meta in output_directory.rglob("*.meta"):
            relative = meta.relative_to(output_directory)
            target = relative.with_suffix("")
            if target.name in generated_files or relative in {
                Path("Internal.meta"),
                Path("Internal/x86_64.meta"),
            }:
                preserved_meta[relative] = meta

    try:
        for source in sources:
            shutil.copy2(source, staging / source.name)
        shutil.copy2(asmdef, staging / asmdef.name)

        architecture_directory = staging / "Internal" / "x86_64"
        architecture_directory.mkdir(parents=True)
        shutil.copy2(native_library, architecture_directory / native_library.name)

        for relative, meta in preserved_meta.items():
            destination = staging / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(meta, destination)

        if output_directory.exists():
            if not output_directory.is_dir():
                raise SystemExit(f"output exists and is not a directory: {output_directory}")
            shutil.rmtree(output_directory)
        staging.replace(output_directory)
    except BaseException:
        if staging.exists():
            shutil.rmtree(staging)
        raise

    print(output_directory)
    for path in sorted(output_directory.rglob("*")):
        if path.is_file():
            print(f"  {path.relative_to(output_directory)} ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
