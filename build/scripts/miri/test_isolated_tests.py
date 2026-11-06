#!/usr/bin/env python3
# Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
# SPDX-License-Identifier: Apache-2.0
"""Regression tests for the audit runner's failure accounting."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from isolated_tests import classify, run_suite

FAKE = """
import sys
import time
if '--list' in sys.argv:
    print('bad: test\\ngood: test\\nslow: test')
elif 'bad' in sys.argv:
    print('error: Undefined Behavior: test diagnostic')
    sys.exit(1)
elif 'slow' in sys.argv:
    time.sleep(10)
else:
    print('test result: ok. 1 passed; 0 failed;')
"""


class IsolationTests(unittest.TestCase):
    def test_failure_and_timeout_do_not_hide_other_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            runner = directory / 'fake.py'
            runner.write_text(FAKE)
            output = directory / 'results'
            with contextlib.redirect_stdout(io.StringIO()):
                code = run_suite([sys.executable, str(runner)], output,
                                 timeout=1, separator=False)
            self.assertEqual(code, 1)
            report = json.loads((output / 'results.json').read_text())
            self.assertEqual(report['counts'],
                             {'undefined-behavior': 1, 'passed': 1, 'timeout': 1})
            self.assertEqual(len(list(output.glob('*.log'))), 4)

    def test_empty_selection_is_not_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            runner = directory / 'fake.py'
            runner.write_text(FAKE)
            with contextlib.redirect_stdout(io.StringIO()):
                code = run_suite([sys.executable, str(runner)], directory / 'results',
                                 test_filter='missing', separator=False)
            self.assertEqual(code, 1)

    def test_discovery_errors_are_not_test_failures(self):
        with tempfile.TemporaryDirectory() as temporary:
            with contextlib.redirect_stdout(io.StringIO()):
                code = run_suite([sys.executable, '-c', 'raise SystemExit(2)'],
                                 Path(temporary), separator=False)
            self.assertEqual(code, 1)
            report = json.loads((Path(temporary) / 'results.json').read_text())
            self.assertFalse(report['completed'])
            self.assertEqual(report['results'], [])

    def test_colored_diagnostic(self):
        self.assertEqual(classify(1, "\x1b[91merror\x1b[0m: Undefined Behavior: bad"),
                         "undefined-behavior")

    def test_ignored_and_zero_executed_are_not_success(self):
        self.assertEqual(classify(0, 'test result: ok. 0 passed; 0 failed; 1 ignored'),
                         'ignored')
        self.assertEqual(classify(0, 'test result: ok. 0 passed; 0 failed; 0 ignored'),
                         'runner-or-compiler-error')


if __name__ == '__main__':
    unittest.main()
