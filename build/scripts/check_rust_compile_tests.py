#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Check Rust API boundaries with the dependencies/cfgs of a GN host probe."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--command-file", type=Path, required=True)
    parser.add_argument("--pass", dest="positive", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("negative", nargs="+", type=Path)
    args = parser.parse_args()
    tokens = shlex.split(args.command_file.read_text())
    env = os.environ.copy()
    while tokens and re.match(r"[A-Za-z_][A-Za-z0-9_]*=", tokens[0]):
        key, value = tokens.pop(0).split("=", 1)
        env[key] = value
    # Preserve GN's compiler wrapper: it loads the build-script environment.
    # Only the final driver changes from clippy-driver to its sibling rustc.
    compiler = []
    if len(tokens) >= 3 and Path(tokens[0]).name == "python3":
        assert Path(
            tokens[1]).name == "run_rustc.py", "unexpected compiler wrapper"
        compiler = tokens[:2]
        tokens = tokens[2:]
    assert tokens and Path(
        tokens[0]).name == "clippy-driver", "unexpected probe command"
    compiler.append(str(Path(tokens.pop(0)).with_name("rustc")))
    flags = []
    paired = {"--extern", "--cfg", "--check-cfg", "--edition", "-L"}
    it = iter(tokens)
    for token in it:
        if token in paired:
            flags.extend([token, next(it)])
        elif token.startswith(
            ("--extern=", "--cfg=", "--check-cfg=", "--edition=", "-L")):
            flags.append(token)
    args.out_dir.mkdir(parents=True, exist_ok=True)
    stamp = args.out_dir / "passed"
    stamp.unlink(missing_ok=True)
    for source in [args.positive, *args.negative]:
        result = subprocess.run([
            *compiler, "--crate-name", "api_check", "--crate-type", "rlib",
            "--emit=metadata", "--error-format=json", *flags,
            str(source), "-o",
            str(args.out_dir / (source.stem + ".rmeta"))
        ],
                                env=env,
                                capture_output=True,
                                text=True,
                                check=False)
        (args.out_dir / (source.stem + ".json")).write_text(result.stderr)
        diagnostics = [
            json.loads(line) for line in result.stderr.splitlines()
            if line.startswith("{")
        ]
        errors = [d for d in diagnostics if d.get("level") == "error"]
        codes = {d["code"]["code"] for d in errors if d.get("code")}
        if source == args.positive:
            valid = result.returncode == 0
        else:
            expected = set(
                re.findall(r"^// error-code: (E[0-9]+)$", source.read_text(),
                           re.M))
            valid = bool(
                expected) and result.returncode != 0 and codes == expected
            # Uncoded macro errors must not accidentally satisfy an unrelated
            # expected diagnostic. rustc's final abort summary has no spans.
            valid = valid and all(
                d.get("code") or not d.get("spans") for d in errors)
        if not valid:
            raise RuntimeError(
                f"Unexpected compile result for {source}:\n{result.stderr}")
        print(
            f"{source.name}: {'accepted' if source == args.positive else 'rejected ' + ','.join(sorted(codes))}"
        )
    stamp.write_text("passed\n")


if __name__ == "__main__":
    main()
