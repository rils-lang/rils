#!/usr/bin/env python3
"""Generate or verify Unity host manifests and static C# handlers."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PROJECT = REPOSITORY_ROOT / "integrations" / "RilsForUnity"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--project",
        type=Path,
        default=DEFAULT_PROJECT,
        help="Unity project containing Tools/rils_for_unity.py",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail when generated manifest or C# output is out of date",
    )
    arguments = parser.parse_args()
    project = arguments.project.resolve()
    entry = project / "Tools" / "rils_for_unity.py"
    if not entry.is_file():
        parser.error(f"Unity binding entry does not exist: {entry}")
    command = [sys.executable, str(entry), "bindings"]
    if arguments.check:
        command.append("--check")
    return subprocess.run(command, cwd=project, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
