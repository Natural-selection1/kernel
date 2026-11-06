#!/usr/bin/env python3
# Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
# SPDX-License-Identifier: Apache-2.0
"""Run each discovered libtest test in a separate process, keeping every failure.

A non-passing case makes the suite fail. UB, panic, timeout, ignored tests and
unsupported operations are reported separately; no Miri checks are disabled.
Also usable with a native libtest binary via the command-line entry point.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import shlex
import subprocess
import time


def execute(command, env, timeout):
    start = time.monotonic()
    try:
        result = subprocess.run(command, env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=timeout, check=False)
        return result.returncode, result.stdout, time.monotonic() - start
    except subprocess.TimeoutExpired as error:
        output = error.stdout or b""
        if isinstance(output, bytes):
            output = output.decode("utf-8", errors="replace")
        return None, output, time.monotonic() - start


def classify(code, output):
    output = re.sub(r"\x1b\[[0-9;]*m", "", output)
    if code is None:
        return "timeout"
    if "error: Undefined Behavior:" in output:
        return "undefined-behavior"
    if "unsupported operation" in output:
        return "unsupported"
    if "memory leaked" in output:
        return "leak"
    if code == 0 and "test result: ok. 1 passed;" in output:
        return "passed"
    if code == 0 and "1 ignored" in output:
        return "ignored"
    if "test result: FAILED" in output or "panicked at" in output:
        return "panic-or-assertion"
    return "runner-or-compiler-error"


def write_report(path, report):
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def run_suite(command, directory, env=None, timeout=60, test_filter="", separator=True):
    if timeout <= 0:
        raise ValueError("timeout must be positive")
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "results.md").unlink(missing_ok=True)
    report = {"command": command, "timeout_seconds": timeout,
              "filter": test_filter, "completed": False, "results": []}
    report_path = directory / "results.json"
    write_report(report_path, report)
    prefix = [*command, "--"] if separator else list(command)
    list_command = [*prefix, "--list", "--format", "terse"]
    code, listing, _ = execute(list_command, env, timeout)
    (directory / "discovery.log").write_text(listing, encoding="utf-8")
    if code != 0:
        print(f"Test discovery failed: {directory / 'discovery.log'}", flush=True)
        return 1
    tests = [line[:-6] for line in listing.splitlines() if line.endswith(": test")]
    tests = [name for name in tests if test_filter in name]
    if not tests:
        print("No matching tests discovered; refusing an empty success", flush=True)
        return 1
    results = []
    report.update(discovered=len(tests), results=results)
    for index, name in enumerate(tests, 1):
        case_command = [*prefix, name, "--exact", "--nocapture", "--test-threads=1"]
        code, output, elapsed = execute(case_command, env, timeout)
        status = classify(code, output)
        log_name = f"{index:03}-" + re.sub(r"[^a-zA-Z0-9_.-]", "_", name) + ".log"
        (directory / log_name).write_text(
            "$ " + shlex.join(case_command) + "\n" + output, encoding="utf-8")
        results.append({"test": name, "status": status, "returncode": code,
                        "seconds": round(elapsed, 3), "log": log_name})
        # Retain completed cases even if a later test or the build is interrupted.
        report["counts"] = dict(Counter(case["status"] for case in results))
        write_report(report_path, report)
        print(f"[{index}/{len(tests)}] {status}: {name} ({elapsed:.1f}s)", flush=True)
    report["completed"] = True
    write_report(report_path, report)
    lines = ["| Test | Result | Seconds |", "| --- | --- | ---: |"]
    lines += [f"| [{case['test']}]({case['log']}) | {case['status']} | {case['seconds']} |"
              for case in results]
    (directory / "results.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Results: {directory / 'results.json'}: {report['counts']}", flush=True)
    return int(any(case["status"] != "passed" for case in results))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("--filter", default="")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command or args.timeout <= 0:
        parser.error("a command and a positive timeout are required")
    return run_suite(command, args.output_dir, timeout=args.timeout,
                     test_filter=args.filter, separator=False)


if __name__ == "__main__":
    raise SystemExit(main())
