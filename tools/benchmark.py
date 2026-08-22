#!/usr/bin/env python3
"""Run an opt-in Rils release benchmark and save a machine-local JSON result."""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "target" / "benchmarks"


def command_output(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario", choices=["vm-integer-loop"])
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--work", type=int, default=100_000)
    arguments = parser.parse_args()

    command = [
        "cargo",
        "run",
        "--release",
        "-p",
        "rils_bench",
        "--",
        arguments.scenario,
        "--warmups",
        str(arguments.warmups),
        "--iterations",
        str(arguments.iterations),
        "--work",
        str(arguments.work),
    ]
    completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    if completed.returncode:
        sys.stderr.write(completed.stderr)
        return completed.returncode
    result = json.loads(completed.stdout)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    result["metadata"] = {
        "commit": command_output(["git", "rev-parse", "HEAD"]),
        "rustc": command_output(["rustc", "--version"]),
        "platform": platform.platform(),
        "python": platform.python_version(),
    }
    RESULTS.mkdir(parents=True, exist_ok=True)
    output = RESULTS / f"{arguments.scenario}-{timestamp}.json"
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(output.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
