#!/usr/bin/env python3
"""Regenerate Rils declarations from the Rust standard-library definitions."""

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT_ROOT = ROOT / "crates/rils_builtins/stdlib"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if generated files differ")
    args = parser.parse_args()

    result = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "rils_stdlib", "--example", "export_sources"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        sys.stderr.buffer.write(result.stderr)
        return result.returncode
    parts = result.stdout.split(b"\0")
    if not parts or parts[-1] != b"" or len(parts) < 3 or len(parts) % 2 != 1:
        print("invalid Rust standard-library export", file=sys.stderr)
        return 1

    stale = []
    seen = set()
    for raw_path, content in zip(parts[::2], parts[1::2]):
        try:
            relative = Path(raw_path.decode("utf-8"))
        except UnicodeDecodeError:
            print("invalid standard-library export path", file=sys.stderr)
            return 1
        if (
            relative.is_absolute()
            or ".." in relative.parts
            or relative.suffix != ".rils"
            or relative.parts[0] not in ("core", "std")
            or relative in seen
        ):
            print(f"invalid or duplicate standard-library export path: {relative}", file=sys.stderr)
            return 1
        seen.add(relative)
        target = OUTPUT_ROOT / relative
        if args.check:
            if not target.exists() or target.read_bytes().replace(b"\r\n", b"\n") != content:
                stale.append(target.relative_to(ROOT))
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    if stale:
        for path in stale:
            print(f"stale generated standard-library source: {path}", file=sys.stderr)
        print("run python tools/generate-stdlib-sources.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
