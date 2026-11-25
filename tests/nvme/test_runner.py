"""Five regressions for fixture validity, raw evidence and interpreter copying.

Run with unittest discovery as Linux root and existing chroot/userspace tools.
Other hosts skip this class; that is not guest acceptance. Tests reject equally
broken scripts and unexpected successful exits, preserve raw success/failure
records, and prove a shebang tool can run with its copied interpreter/libraries.
No Azure, SSH, real guest disks or mounts are used. See the adjacent README.md.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from run_tests import Case, compare, copy_tool, prepare_base, write


@unittest.skipUnless(sys.platform == 'linux' and os.geteuid() == 0, 'Linux root required for isolated fixtures')
class RunnerTests(unittest.TestCase):
    """Validate the harness and audit traces, not real Linux NVMe boot readiness."""
    def test_raw_case_evidence_keeps_streams_and_file_snapshots(self):
        """Successful traces must retain both streams and literal before/after snapshots."""
        with tempfile.TemporaryDirectory(prefix='alar-raw-evidence-') as directory:
            parent = Path(directory)
            prepare_base(parent / 'base')
            script = 'echo standard-output; echo standard-error >&2\n'
            result = compare(parent / 'base', parent, Case('ubuntu', 'ready', 'check'), script, script, parent / 'review')
            self.assertEqual(result['status'], 'passed')
            evidence = json.loads((parent / 'review/ubuntu/ready/check/normal.json').read_text())
            self.assertFalse(evidence['boot_evidence'])
            self.assertEqual(len(evidence['phases']), 2)
            for phase in evidence['phases']:
                self.assertEqual(phase['returncode'], 0)
                self.assertEqual(phase['stdout'], 'standard-output\n')
                self.assertEqual(phase['stderr'], 'standard-error\n')
                self.assertIn('etc/fstab', phase['before'])
                self.assertEqual(phase['before'], phase['after'])

    def test_failed_case_preserves_actual_exit_in_evidence(self):
        """A caught shell failure must retain its real exit and failed result in evidence."""
        with tempfile.TemporaryDirectory(prefix='alar-failed-evidence-') as directory:
            parent = Path(directory)
            prepare_base(parent / 'base')
            result = compare(parent / 'base', parent, Case('ubuntu', 'ready', 'check'), 'if then\n', 'if then\n', parent / 'review')
            self.assertEqual(result['status'], 'failed')
            evidence = json.loads((parent / 'review/ubuntu/ready/check/normal.json').read_text())
            self.assertEqual(evidence['phases'][0]['returncode'], 2)
            self.assertEqual(evidence['result']['status'], 'failed')

    def test_equal_shell_errors_are_failures(self):
        """Matching shell syntax failures must still fail the expected-exit guard."""
        with tempfile.TemporaryDirectory(prefix='alar-equal-errors-') as directory:
            parent = Path(directory)
            prepare_base(parent / 'base')
            result = compare(parent / 'base', parent, Case('ubuntu', 'ready', 'check'), 'if then\n', 'if then\n')
            self.assertEqual(result['status'], 'failed')
            self.assertIn('Unexpected script exit 2', result['error'])

    def test_equal_success_cannot_satisfy_expected_usage_failure(self):
        """Matching exit0 results must not pass a case whose expected exit is 1."""
        with tempfile.TemporaryDirectory(prefix='alar-usage-errors-') as directory:
            parent = Path(directory)
            prepare_base(parent / 'base')
            result = compare(parent / 'base', parent, Case('ubuntu', 'ready', 'invalid'), 'exit 0\n', 'exit 0\n')
            self.assertEqual(result['status'], 'failed')
            self.assertIn('expected 1', result['error'])

    def test_shebang_fixture_copies_interpreter(self):
        """A copied script must run in its chroot with the absolute interpreter present."""
        bash = shutil.which('bash')
        self.assertIsNotNone(bash)
        with tempfile.TemporaryDirectory(prefix='alar-shebang-') as directory:
            parent = Path(directory)
            script = parent / 'script'
            script.write_text(f'#!{bash}\nprintf copied-interpreter\n')
            script.chmod(0o755)
            root = parent / 'root'
            copy_tool(root, script, root / 'bin/probe')
            write(root, 'dev/null', '')
            result = subprocess.run(['/usr/sbin/chroot', str(root), '/bin/probe'],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, 'copied-interpreter')


if __name__ == '__main__':
    unittest.main()
