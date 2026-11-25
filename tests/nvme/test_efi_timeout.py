"""Regress ALAR's RHEL 7 standalone EFI timeout handling in fake chroots.

Linux root required; uses existing copied tools, never Azure, real mounts or disks.
Each case exercises the actual action's timeout function and helper bodies.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from run_tests import prepare_base, write


@unittest.skipUnless(sys.platform == 'linux' and os.geteuid() == 0, 'Linux root required')
class EfiTimeoutTests(unittest.TestCase):
    """Keep stale EFI/defaults, exact token checks and other loader layouts safe."""
    @classmethod
    def setUpClass(cls):
        """Read the repository action and copy existing userspace tools."""
        cls.tmp = tempfile.TemporaryDirectory(prefix='alar-efi-timeout-')
        cls.addClassCleanup(cls.tmp.cleanup)
        cls.base = Path(cls.tmp.name) / 'base'
        prepare_base(cls.base)
        repo = Path(__file__).resolve().parents[2]
        cls.scripts = ((repo / 'src/action_implementation/nvme-impl.sh').read_text(),)

    def run_case(self, script, mode='fix', version='7.9', efi=True, ready=False,
                 token='', broken=False, redirect=False, failure=False, missing_defaults=False, silent_failure=False, other_kernel_missing=False):
        """Run actual function definitions against synthetic boot entries and logged rebuilds."""
        temporary = tempfile.TemporaryDirectory(dir=self.tmp.name)
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / 'guest'
        shutil.copytree(self.base, root)
        target = '/boot/efi/EFI/redhat/grub.cfg'
        write(root, 'etc/os-release', f'ID=rhel\nVERSION_ID="{version}"\n')
        defaults = 'GRUB_CMDLINE_LINUX="quiet nvme_core.io_timeout=240 console=ttyS0"\n'
        if not missing_defaults:
            write(root, 'etc/default/grub', defaults)
        entry = 'linuxefi /vmlinuz root=/dev/mapper/rootvg-rootlv quiet console=ttyS0'
        write(root, 'boot/grub2/grub.cfg', entry + ' nvme_core.io_timeout=240\n')
        if efi:
            write(root, target.lstrip('/'), 'configfile /boot/grub2/grub.cfg\n' if redirect else
                  entry + (' nvme_core.io_timeout=240' if ready else (' ' + token if token else '')) + '\n' + (entry.replace('/vmlinuz', '/vmlinuz-older') + '\n' if other_kernel_missing else ''))
            (root / 'etc/grub2-efi.cfg').symlink_to('/boot/efi/EFI/redhat/missing.cfg' if broken else target)
        write(root, 'bin/grub2-mkconfig', '#!/bin/bash\necho "grub2-mkconfig $*" >> /commands.log\n'
              + ('echo rebuild-error >&2; exit 42\n' if failure else 'exit 0\n' if silent_failure else
                 '. /etc/default/grub\nprintf "linuxefi /vmlinuz root=/dev/mapper/rootvg-rootlv %s\\n" "$GRUB_CMDLINE_LINUX" > "$2"\n'))
        (root / 'bin/grub2-mkconfig').chmod(0o755)
        write(root, 'commands.log', '')
        write(root, 'dev/null', '')
        for directory in ('tmp/staging/original', 'tmp/staging/modified', 'tmp/staging/diffs'):
            (root / directory).mkdir(parents=True, exist_ok=True)
        functions = '\n\n'.join(re.findall(r'^(?:nvme_timeout_entries_ready|set_rhel7_timeout_defaults|check_nvme_timeout)\(\) \{\n.*?^\}', script, re.MULTILINE | re.DOTALL))
        write(root, 'run.sh', functions + '\ndistro=rhel\nfix=' + ('false' if mode == 'check' else 'true')
              + '\ndry_run=' + ('true' if mode == 'dryrun' else 'false')
              + '\nstaging_dir=/tmp/staging\ncheck_nvme_timeout\n')
        before = {str(path.relative_to(root)): path.read_bytes() for path in (root / 'boot').rglob('*') if path.is_file()}
        result = subprocess.run(['/usr/sbin/chroot', str(root), '/bin/bash', '/run.sh'],
                                env={'PATH': '/bin'}, text=True, capture_output=True, timeout=30)
        return root, result, before

    def each_source(self, **kwargs):
        """Exercise the repository action, not a copied timeout algorithm."""
        return [self.run_case(script, **kwargs) for script in self.scripts]

    def test_stale_efi_is_not_ready_from_defaults_or_other_config(self):
        """The exact live failure must not report readiness from the wrong files."""
        for root, result, _ in self.each_source(mode='check'):
            self.assertIn('[ERROR] nvme_core.io_timeout is not set to 240.', result.stdout)
            self.assertNotIn('[INFO] nvme_core.io_timeout is set to 240.', result.stdout)

    def test_fix_rebuilds_standalone_efi_without_duplicate_default_argument(self):
        """Repair the proven standalone EFI target even when desired defaults already have 240."""
        for root, result, _ in self.each_source():
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('grub2-mkconfig -o /boot/efi/EFI/redhat/grub.cfg', (root/'commands.log').read_text())
            self.assertEqual((root/'etc/default/grub').read_text().count('nvme_core.io_timeout=240'), 1)
            self.assertIn('quiet', (root/'etc/default/grub').read_text())
            self.assertIn('nvme_core.io_timeout=240', (root/'boot/efi/EFI/redhat/grub.cfg').read_text())

    def test_ready_standalone_efi_is_idempotent(self):
        """A ready EFI entry skips a needless rewrite."""
        for root, result, _ in self.each_source(ready=True):
            self.assertNotIn('grub2-mkconfig', (root/'commands.log').read_text())
            self.assertIn('is set to 240.', result.stdout)

    def test_partial_or_wrong_argument_is_not_ready(self):
        """2400 and a conflicting timeout must not be accepted as 240."""
        for token in ('nvme_core.io_timeout=2400', 'nvme_core.io_timeout=240 nvme_core.io_timeout=30'):
            for root, result, _ in self.each_source(mode='check', token=token):
                self.assertIn('[ERROR] nvme_core.io_timeout is not set to 240.', result.stdout)

    def test_dryrun_does_not_change_boot_files(self):
        """Stage defaults and target diagnostics without rebuilding real fixture files."""
        for root, result, before in self.each_source(mode='dryrun'):
            after = {str(path.relative_to(root)): path.read_bytes() for path in (root/'boot').rglob('*') if path.is_file()}
            self.assertEqual(before, after)
            self.assertIn('/boot/efi/EFI/redhat/grub.cfg', result.stdout)
            self.assertNotIn('grub2-mkconfig', (root/'commands.log').read_text())

    def test_broken_efi_target_is_explicit_error(self):
        """A broken configured EFI link must not fall through to false readiness."""
        for root, result, _ in self.each_source(broken=True):
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('[ERROR]', result.stdout)
            self.assertEqual((root/'commands.log').read_text(), '')

    def test_modern_redirect_is_never_overwritten(self):
        """A configfile stub must not become a full generated configuration."""
        for root, result, before in self.each_source(redirect=True):
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('[ERROR]', result.stdout)
            self.assertEqual((root/'boot/efi/EFI/redhat/grub.cfg').read_bytes(), before['boot/efi/EFI/redhat/grub.cfg'])

    def test_modern_release_keeps_original_behavior(self):
        """RHEL8+/BLS selection remains outside this targeted old standalone EFI path."""
        for root, result, _ in self.each_source(version='9.8', redirect=True):
            self.assertEqual(result.returncode, 0)
            self.assertIn('is set to 240.', result.stdout)
            self.assertEqual((root/'commands.log').read_text(), '')

    def test_bios_layout_keeps_original_behavior(self):
        """Without a guest EFI configuration link retain the existing BIOS path."""
        for root, result, _ in self.each_source(efi=False):
            self.assertEqual(result.returncode, 0)
            self.assertIn('is set to 240.', result.stdout)

    def test_failed_efi_rebuild_is_error(self):
        """An actual failed generator must not be hidden by desired defaults."""
        for root, result, _ in self.each_source(failure=True):
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('[ERROR]', result.stdout)

    def test_missing_defaults_is_error(self):
        """The fix cannot silently succeed without its expected defaults file."""
        for root, result, _ in self.each_source(missing_defaults=True):
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('[ERROR]', result.stdout)

    def test_every_standalone_kernel_entry_requires_timeout(self):
        """One ready entry cannot mask an older kernel without the required flag."""
        for root, result, _ in self.each_source(mode='check', token='nvme_core.io_timeout=240', other_kernel_missing=True):
            self.assertIn('[ERROR] nvme_core.io_timeout is not set to 240.', result.stdout)

    def test_successful_generator_without_repair_is_rejected(self):
        """An exit-zero generator that leaves stale EFI entries is not a successful fix."""
        for root, result, _ in self.each_source(silent_failure=True):
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('[ERROR] Failed to set nvme_core.io_timeout.', result.stdout)
