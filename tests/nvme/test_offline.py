"""Seven regressions for ALAR's offline guest-kernel and device-map adapters.

Run through unittest discovery as Linux root with the same copied-tool/chroot
prerequisites as run_tests.py. Other hosts skip this class, not certify it.
Tests use private fake guest directories, stub commands and the repository
candidate; they do not import the reference or use real disks/mounts/Azure/SSH.
See the adjacent README.md for the combined suite command and coverage.
"""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from run_tests import Case, fixture, prepare_base, write


@unittest.skipUnless(sys.platform == 'linux' and os.geteuid() == 0, 'Linux root required for isolated chroot fixtures')
class OfflineTests(unittest.TestCase):
    """Verify guest identity, all kernels, safe swap mapping and explicit failures."""
    @classmethod
    def setUpClass(cls):
        """Create a shared temporary userspace base and read the candidate shell source."""
        cls.temporary = tempfile.TemporaryDirectory(prefix='alar-nvme-adapters-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.base = Path(cls.temporary.name) / 'base'
        prepare_base(cls.base)
        cls.action = (Path(__file__).resolve().parents[2] / 'src/action_implementation/nvme-impl.sh').read_text()

    def setUp(self):
        """Allocate a separate guest directory with automatic cleanup for each test."""
        self.directory = tempfile.TemporaryDirectory(dir=self.temporary.name)
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / 'guest'

    def prepare(self, distro='rhel', state='missing'):
        """Build the selected fake guest layout for default-fix candidate execution."""
        fixture(self.base, self.root, Case(distro, state, 'fix'), self.action, original=False)

    def run_action(self, mapping='/device-map'):
        """Execute the candidate in the test chroot with a 60-second timeout.

        mapping names a guest-visible map file; None omits ALAR_NVME_DEVICE_MAP entirely.
        Return captured status/streams so tests can assert diagnostics and command use.
        """
        environment = {'PATH': '/bin'}
        if mapping is not None:
            environment['ALAR_NVME_DEVICE_MAP'] = mapping
        return subprocess.run(['/usr/sbin/chroot', str(self.root), '/bin/bash', '/run.sh'],
                              env=environment, capture_output=True, text=True, timeout=60)

    def log(self):
        """Read the stub-command transcript from the current fake guest."""
        return (self.root / 'commands.log').read_text()

    def test_guest_builtin_kernel_not_rescue_kernel(self):
        """A guest built-in driver must skip rebuilding without consulting rescue uname."""
        self.prepare('ubuntu', 'builtin')
        result = self.run_action()
        self.assertIn('NVMe driver is built into the kernel.', result.stdout)
        self.assertNotIn('uname ', self.log())
        self.assertNotIn('update-initramfs ', self.log())

    def test_all_guest_kernels_are_checked(self):
        """A second installed guest kernel must be checked and repaired when needed."""
        self.prepare(state='builtin')
        write(self.root, 'lib/modules/guest-2/modules.builtin', '')
        self.run_action()
        self.assertIn('modinfo -k guest-2 nvme', self.log())
        self.assertIn('dracut -f --regenerate-all', self.log())

    def test_explicit_partition_mapping_covers_unmounted_swap(self):
        """An explicit guest/rescue partition pair must resolve an unmounted swap entry."""
        self.prepare()
        write(self.root, 'device-map', '/dev/sda2 /dev/nvme2n5p1\n')
        self.run_action()
        self.assertIn('UUID=data-uuid none swap', (self.root / 'etc/fstab').read_text())

    def test_ambiguous_swap_is_not_looked_up_on_rescue_host(self):
        """Unresolved swap must remain intact without probing its old name on the rescue."""
        self.prepare()
        write(self.root, 'etc/fstab', 'UUID=stable / ext4 defaults 0 1\n/dev/sda2 none swap sw 0 0\n')
        result = self.run_action(mapping=None)
        self.assertIn('Cannot map guest device /dev/sda2', result.stderr)
        self.assertIn('/dev/sda2 none swap', (self.root / 'etc/fstab').read_text())
        self.assertNotIn('blkid -c /dev/null /dev/sda2', self.log())

    def test_invalid_map_is_reported_without_device_guessing(self):
        """Malformed mapping must emit an error without speculative blkid calls."""
        self.prepare()
        write(self.root, 'device-map', 'invalid-map\n')
        result = self.run_action()
        self.assertIn('[ERROR] Invalid device map entry:', result.stderr)
        self.assertNotIn('blkid ', self.log())

    def test_missing_map_is_reported(self):
        """An unreadable explicitly selected map must emit a diagnostic."""
        self.prepare()
        result = self.run_action(mapping='/missing-map')
        self.assertIn('[ERROR] Cannot read device map:', result.stderr)

    def test_missing_guest_kernels_prevent_mutations(self):
        """Missing installed guest kernels must fail before any stubbed repair command."""
        self.prepare()
        shutil.rmtree(self.root / 'lib/modules/guest-1')
        result = self.run_action()
        self.assertEqual(result.returncode, 1)
        self.assertIn('[ERROR] No installed guest kernels', result.stdout)
        self.assertEqual(self.log(), '')


if __name__ == '__main__':
    unittest.main()
