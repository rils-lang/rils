#!/usr/bin/env python3
"""Regenerate Rils declarations from the Rust standard-library definitions."""

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUTS = (
    ROOT / "crates/rils_builtins/stdlib/core/option.rils",
    ROOT / "crates/rils_builtins/stdlib/core/result.rils",
    ROOT / "crates/rils_builtins/stdlib/core/integer.rils",
)


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
    if len(parts) != len(OUTPUTS):
        print("unexpected Rust standard-library export count", file=sys.stderr)
        return 1

    stale = []
    for target, content in zip(OUTPUTS, parts):
        if args.check:
            if not target.exists() or target.read_bytes().replace(b"\r\n", b"\n") != content:
                stale.append(target.relative_to(ROOT))
        else:
            target.write_bytes(content)
    if stale:
        for path in stale:
            print(f"stale generated standard-library source: {path}", file=sys.stderr)
        print("run python tools/generate-stdlib-sources.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
