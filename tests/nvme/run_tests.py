"""Compare fix-only Linux NVMe preparation in parallel, isolated chroot fixtures.

Requires Linux root, Python 3.10+ and existing userspace tools. The extension's
literal _LINUX_SCRIPT is read with AST, never imported. Reference and candidate
scripts run on separate fake guest filesystems with stubbed device commands;
there are no mounts, real guest disks, Azure calls or SSH connections.

CLI: --reference PATH --report NEW_JSON [--evidence-dir NEW_DIR] [--jobs N].
Reports and evidence must be new and outside the checkout. Completed comparisons
return 0 when all cases pass or 1 when any case fails; argparse errors return 2.
An aborted setup/run can leave an incomplete report, which is not a pass.
See the adjacent README.md for dependencies, commands and evidence schemas.
"""

from __future__ import annotations

import argparse
import ast
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


STUB = r'''#!/bin/bash
name=${0##*/}
printf '%s %s\n' "$name" "$*" >> /commands.log
if [[ -f /fail-command && "$(< /fail-command)" == "$name" ]]; then exit 42; fi
case "$name" in
  hostname) echo fixture-host ;;
  uname) echo rescue-kernel ;;
  modinfo|udevadm|azure-nvme-id) exit 0 ;;
  lsinitramfs|lsinitrd) cat "$@" ;;
  update-initramfs)
    [[ -f /failed-rebuild ]] && exit 0
    for image in /boot/initrd.img-*; do echo 'nvme hv_pci' > "$image"; done ;;
  dracut)
    [[ -f /failed-rebuild ]] && exit 0
    for image in /boot/initramfs-*.img; do
      [[ "$image" == *kdump* || "$image" == *rescue* ]] && continue
      echo 'nvme hv_pci' > "$image"
    done ;;
  update-grub) echo generated-grub > /boot/grub/grub.cfg ;;
  grub2-mkconfig) echo generated-grub > "$2" ;;
  grubby)
    if [[ "$1" == --info=ALL ]]; then cat /bls-args
    else echo nvme_core.io_timeout=240 > /bls-args; fi ;;
  findmnt)
    case "${@: -1}" in
      /) echo /dev/sdz1 ;; /boot) echo /dev/sdz3 ;;
      /boot/efi) echo /dev/sdz4 ;; *) exit 1 ;;
    esac ;;
  lsblk) echo sdz ;;
  blkid)
    device=${@: -1}
    case "$device" in
      /dev/sda1|/dev/sdz1) uuid=root-uuid ;;
      /dev/sda2|/dev/sdz2) uuid=swap-uuid ;;
      /dev/sda3|/dev/sdz3) uuid=boot-uuid ;;
      /dev/sda4|/dev/sdz4) uuid=efi-uuid ;;
      /dev/sdb1|/dev/disk/azure/scsi1/lun0-part1|/dev/nvme2n5p1) uuid=data-uuid ;;
      *) exit 2 ;;
    esac
    printf '%s: UUID="%s" TYPE="ext4"\n' "$device" "$uuid" ;;
  *) exit 1 ;;
esac
'''
DISTROS = ('ubuntu', 'debian', 'redhat', 'rhel', 'centos', 'rocky', 'almalinux',
           'suse', 'sles', 'ol', 'azurelinux', 'mariner')
COMMANDS = ('hostname uname modinfo udevadm lsinitramfs lsinitrd update-initramfs '
            'dracut update-grub grub2-mkconfig grubby findmnt lsblk blkid').split()
FSTAB = '''# Preserve unrelated and unresolved entries.
/dev/sda1 / ext4 defaults 0 1
/dev/sda3 /boot ext4 defaults 0 2
/dev/sda4 /boot/efi vfat defaults 0 2
/dev/sda2 none swap sw 0 0
/dev/sdb1 /data xfs defaults 0 2
/dev/disk/azure/scsi1/lun0-part1 /azure-data xfs defaults 0 2
/dev/sdc1 /unavailable xfs defaults 0 2
UUID=stable /stable ext4 defaults 0 2
PARTUUID=part /part ext4 defaults 0 2
/dev/mapper/rootvg-homelv /home xfs defaults 0 2
/dev/disk/azure/resource-part1 /mnt/resource ext4 defaults 0 2
'''


@dataclass(frozen=True)
class Case:
    """Immutable distro/driver/mode/scenario key used by fixtures and evidence paths."""
    distro: str
    state: str
    mode: str
    scenario: str = 'normal'

    @property
    def name(self) -> str:
        """Return the stable distro/state/mode/scenario identifier and relative path."""
        return '/'.join((self.distro, self.state, self.mode, self.scenario))


def write(root: Path, name: str, text: str) -> None:
    """Write a UTF-8 fixture file; callers supply names relative to root."""
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding='utf-8')


def copy_tool(base: Path, binary: Path, destination: Path, ancestors: tuple[Path, ...] = ()) -> None:
    """Copy a tool plus its shebang interpreter or ELF libraries into a chroot.

    Resolve symlinks and recursively copy absolute script interpreters. The ancestors
    chain rejects interpreter cycles; missing tools and failed ldd calls abort setup.
    """
    binary = binary.resolve()
    if binary in ancestors:
        raise RuntimeError(f'Cyclic fixture interpreter: {binary}')
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, destination)
    header = binary.read_bytes().split(b'\n', 1)[0]
    if header.startswith(b'#!'):
        interpreter = Path(header[2:].decode().split()[0])
        if not interpreter.is_absolute():
            raise RuntimeError(f'Fixture interpreter must be absolute: {binary}')
        # Recent coreutils packages use scripts with a multicall interpreter.
        copy_tool(base, interpreter, base / str(interpreter).lstrip('/'), (*ancestors, binary))
        return
    result = subprocess.run(['ldd', str(binary)], check=True, capture_output=True, text=True)
    for library in re.findall(r'(?:=>\s+|^\s*)(/\S+)', result.stdout, re.MULTILINE):
        target = base / library.lstrip('/')
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(Path(library).resolve(), target)


def prepare_base(base: Path) -> None:
    """Build the reusable userspace base and logged synthetic device-command stubs.

    Copy existing host tools only; do not install packages or run real Azure/device
    commands. Missing prerequisites raise an error before fixtures can be accepted.
    """
    binaries: dict[str, Path] = {}
    for tool in 'bash cat mkdir touch grep sed cp rm mv awk tee diff find sort readlink'.split():
        binary = shutil.which(tool)
        if binary is None:
            raise RuntimeError(f'Missing fixture tool: {tool}')
        binaries[tool] = Path(binary)
    for tool, binary in binaries.items():
        copy_tool(base, binary, base / 'bin' / tool)
    for name in COMMANDS:
        write(base, f'bin/{name}', STUB)
        (base / 'bin' / name).chmod(0o755)


def fixture(base: Path, root: Path, case: Case, script: str, original: bool) -> None:
    """Populate one private fake guest from the hard-linked userspace base.

    Case fields select driver contents, release files, boot arguments and failure
    injection. original=True supplies guest-1 through uname; the candidate retains
    the rescue-kernel stub to exercise installed-guest-kernel discovery.
    """
    shutil.copytree(base, root, copy_function=os.link)
    for directory in ('etc/default', 'etc/udev/rules.d', 'boot/grub', 'boot/grub2',
                      'lib/modules/guest-1', 'dev', 'tmp'):
        (root / directory).mkdir(parents=True, exist_ok=True)
    for name, value in {
        'dev/null': '', 'commands.log': '', 'bls-args': '', 'run.sh': script,
        'etc/os-release': f'ID={case.distro}\n',
        'etc/default/grub': 'GRUB_CMDLINE_LINUX_DEFAULT="quiet"\nGRUB_CMDLINE_LINUX="console=ttyS0"\nGRUB_ENABLE_BLSCFG=true\n',
        'etc/fstab': FSTAB, 'etc/fstab.new': 'stale interrupted content\n',
        'device-map': '/dev/sdb /dev/nvme2n5\n/dev/disk/azure/scsi1/lun0 /dev/nvme2n5\n',
        'lib/modules/guest-1/modules.builtin': 'kernel/drivers/nvme/host/nvme.ko\n' if case.state == 'builtin' else '',
    }.items():
        write(root, name, value)
    contents = {'missing': '', 'builtin': '', 'ready': 'nvme hv_pci',
                'no-hvpci': 'nvme', 'no-nvme': 'hv_pci'}[case.state]
    for image in ('initrd.img-guest-1', 'initramfs-guest-1.img', 'initramfs-guest-2.img'):
        write(root, f'boot/{image}', contents + '\n')
    for image in ('initramfs-kdump.img', 'initramfs-rescue.img'):
        write(root, f'boot/{image}', 'excluded\n')
    if original:
        (root / 'bin/uname').unlink()
        write(root, 'bin/uname', '#!/bin/bash\necho guest-1\n')
        (root / 'bin/uname').chmod(0o755)
    scenario = case.scenario
    if scenario == 'no-grubby':
        (root / 'bin/grubby').unlink()
    elif scenario in ('bls-ready', 'defaults-ready', 'grub2-ready', 'grub-ready'):
        target = {'bls-ready': 'bls-args', 'defaults-ready': 'etc/default/grub',
                  'grub2-ready': 'boot/grub2/grub.cfg', 'grub-ready': 'boot/grub/grub.cfg'}[scenario]
        with (root / target).open('a') as stream:
            stream.write('\nnvme_core.io_timeout=240\n')
    elif scenario.startswith('utils-'):
        write(root, 'bin/azure-nvme-id', STUB)
        (root / 'bin/azure-nvme-id').chmod(0o755)
        if scenario != 'utils-missing-rules':
            directory = {'utils-etc': 'etc', 'utils-lib': 'lib', 'utils-usrlib': 'usr/lib'}[scenario]
            write(root, f'{directory}/udev/rules.d/80-azure-disk.rules', '')
    elif scenario == 'fallback-ready':
        write(root, 'etc/udev/rules.d/99-azure-nvme-fallback.rules', 'existing rule\n')
    elif scenario == 'no-grub-file':
        (root / 'etc/default/grub').unlink()
    elif scenario == 'failed-rebuild':
        write(root, 'failed-rebuild', '')
    elif scenario.startswith('fail-'):
        command = {'fail-update': 'update-initramfs', 'fail-dracut': 'dracut', 'fail-grub': 'grub2-mkconfig'}[scenario]
        write(root, 'fail-command', command)
    elif scenario == 'no-fstab-changes':
        write(root, 'etc/fstab', 'UUID=stable / ext4 defaults 0 1\n')
    elif scenario == 'no-release' or scenario.startswith('legacy-'):
        (root / 'etc/os-release').unlink()
        if scenario.startswith('legacy-'):
            write(root, f'etc/{scenario.removeprefix("legacy-")}', '')


def normalized(text: str) -> str:
    """Remove only declared offline diagnostics and unsupported reference usage flags.

    Use this for comparisons, never for the retained raw stdout/stderr evidence.
    """
    text = re.sub(r'^\[INFO\] Offline guest:.*\n|^\[WARNING\] Cannot map guest device .*\n', '', text, flags=re.MULTILINE)
    return text.replace('[-check] ', '').replace(' [-dryrun]', '')


def snapshot(root: Path, normalize_content: bool = True) -> dict[str, tuple[int, str]]:
    """Capture fixture text and permission bits beneath etc, boot and tmp.

    normalize_content=False retains literal text for raw before/after evidence;
    the default applies only the declared comparison normalization.
    """
    return {str(path.relative_to(root)): (path.stat().st_mode & 0o7777,
                                        normalized(path.read_text()) if normalize_content else path.read_text())
            for directory in ('etc', 'boot', 'tmp') for path in (root / directory).rglob('*')
            if path.is_file()}


def compare(base: Path, parent: Path, case: Case, reference: str, action: str,
            evidence_directory: Path | None = None) -> dict[str, str]:
    """Run and validate both scripts for one case, returning passed/failed details.

    Each chroot phase has a 60-second timeout and an independently expected exit,
    so equally broken scripts cannot pass by equality alone. Compare normalized
    output, guest files, BLS arguments, mapping behavior and invalid-input nonmutation.
    When evidence_directory is supplied, write a new schema-version-1 trace with
    raw streams, commands and literal snapshots, including caught partial failures.
    Existing evidence files are rejected rather than overwritten.
    """
    phases: list[dict[str, object]] = []
    trace: dict[str, object] = {'schema_version': 1, 'kind': 'offline-stub-fixture',
                              'case': case.name, 'phases': phases, 'boot_evidence': False}
    try:
        with tempfile.TemporaryDirectory(dir=parent, prefix='case-') as temporary:
            roots = [Path(temporary) / name for name in ('original', 'action')]
            outputs = []
            for index, (root, script) in enumerate(zip(roots, (reference, action))):
                fixture(base, root, case, script, original=index == 0)
                before = {name: value for name, value in snapshot(root).items() if not name.startswith('tmp/')}
                args = {'fix': ['-fix'] if index == 0 else [],
                        'invalid': ['-invalid']}[case.mode]
                command = ['/usr/sbin/chroot', str(root), '/bin/bash', '/run.sh', *args]
                phase: dict[str, object] = {'script': 'original' if index == 0 else 'candidate',
                                           'command': command, 'before': snapshot(root, False)}
                phases.append(phase)
                result = subprocess.run(command,
                                        env={'PATH': '/bin', 'ALAR_NVME_DEVICE_MAP': '/device-map'},
                                        capture_output=True, text=True, timeout=60)
                phase.update({'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr,
                              'after': snapshot(root, False), 'commands': (root / 'commands.log').read_text()})
                expected_exit = 1 if case.mode == 'invalid' or case.scenario == 'no-release' else 0
                if result.returncode != expected_exit:
                    raise AssertionError(f'Unexpected script exit {result.returncode}; expected {expected_exit}: '
                                         f'{result.stdout}{result.stderr}')
                outputs.append((result.returncode, normalized(result.stdout + result.stderr)))
                if case.mode == 'invalid' or case.scenario == 'no-release':
                    after = {name: value for name, value in snapshot(root).items() if not name.startswith('tmp/')}
                    if before != after:
                        raise AssertionError('Invalid input modified protected guest files')
            if outputs[0] != outputs[1]:
                raise AssertionError(f'Original output: {outputs[0]}\nCandidate output: {outputs[1]}')
            if snapshot(roots[0]) != snapshot(roots[1]):
                raise AssertionError('Guest configuration or image contents differ')
            if (roots[0] / 'bls-args').read_bytes() != (roots[1] / 'bls-args').read_bytes():
                raise AssertionError('BLS arguments differ')
            log = (roots[1] / 'commands.log').read_text()
            if re.search(r'^lsinitrd .*?(kdump|rescue)|^uname |^blkid -c /dev/null /dev/sd[abc]', log, re.MULTILINE):
                raise AssertionError('Rescue identity used or excluded image inspected')
            if case.mode == 'fix' and case.scenario == 'normal':
                fstab = (roots[1] / 'etc/fstab').read_text()
                for expected in ('UUID=swap-uuid none swap', 'UUID=data-uuid /data',
                                 'UUID=data-uuid /azure-data', '/dev/sdc1 /unavailable'):
                    if expected not in fstab:
                        raise AssertionError(f'Missing fstab assertion: {expected}')
        outcome = {'case': case.name, 'status': 'passed'}
    except (OSError, ValueError, AssertionError, subprocess.SubprocessError) as error:
        outcome = {'case': case.name, 'status': 'failed', 'error': str(error)}
    if evidence_directory is not None:
        path = evidence_directory / f'{case.name}.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        trace.update({'result': outcome, 'completed_at': datetime.now(timezone.utc).isoformat()})
        with path.open('x', encoding='utf-8') as stream:
            json.dump(trace, stream, indent=2)
    return outcome


def matrix() -> list[Case]:
    """Return 65 fix-mode, release-detection, fallback and error cases."""
    cases = [Case(distro, state, 'fix') for distro in DISTROS
             for state in ('missing', 'ready', 'builtin')]
    cases += [Case(distro, state, 'fix') for distro in ('ubuntu', 'rhel')
              for state in ('no-hvpci', 'no-nvme')]
    scenarios = ('no-grubby', 'bls-ready', 'defaults-ready', 'grub2-ready', 'grub-ready',
                 'utils-missing-rules', 'utils-etc', 'utils-lib', 'utils-usrlib', 'fallback-ready',
                 'no-grub-file', 'failed-rebuild', 'fail-dracut', 'fail-grub', 'no-fstab-changes')
    cases += [Case('rhel', 'missing', 'fix', scenario) for scenario in scenarios]
    cases += [Case('ubuntu', 'missing', 'fix', scenario) for scenario in ('failed-rebuild', 'fail-update')]
    cases += [Case('ignored', 'missing', 'fix', f'legacy-{release}')
              for release in ('debian_version', 'SuSE-release', 'redhat-release', 'centos-release', 'rocky-release')]
    cases += [Case('arch', 'missing', 'fix')]
    return cases + [Case('ignored', 'missing', 'fix', 'no-release'), Case('ubuntu', 'missing', 'invalid')]


def verify_functions(reference: str, action: str) -> None:
    """Verify unchanged timeout helpers/rules and required repair entrypoints.

    Repair functions have intentionally lost their non-fix branches; compare
    their behavior through fixtures, not a claim of literal source identity.
    """
    for name in ('check_azure_vm_utils', '_install_fallback_udev_rules',
                 'check_nvme_driver', 'nvme_timeout_entries_ready', 'set_rhel7_timeout_defaults',
                 'check_nvme_timeout', 'check_fstab'):
        bodies = []
        for script in (reference, action):
            match = re.search(rf'^{name}\(\) \{{\n.*?^\}}', script, re.MULTILINE | re.DOTALL)
            if match is None:
                raise ValueError(f'Missing original function: {name}')
            bodies.append('\n'.join(line for line in match[0].splitlines()
                                    if not line.lstrip().startswith('#') and 'echo "Usage:' not in line))
        if name in ('nvme_timeout_entries_ready', 'set_rhel7_timeout_defaults') and bodies[0] != bodies[1]:
            raise ValueError(f'Shared timeout helper differs: {name}')
    rules = [re.search(r"^_FALLBACK_UDEV_RULE=.*?^LABEL=.*?'$", text, re.MULTILINE | re.DOTALL) for text in (reference, action)]
    if any(rule is None for rule in rules) or [rule[0] for rule in rules if rule is not None][0] != [rule[0] for rule in rules if rule is not None][1]:
        raise ValueError('Fallback udev rules differ')


def main() -> int:
    """Validate CLI paths, run bounded fixture workers and write the final report.

    Extract the reference literal without importing its module. Create an incomplete
    report first; only a finished run records completed status, hashes, host details
    and sorted case results. Optional evidence adds script copies, per-case traces
    and per-distro summaries. Return 1 for completed runs with failed comparisons.
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True, help='Original extension _linux_script.py; read, never imported.')
    parser.add_argument('--report', type=Path, required=True, help='New JSON report outside the checkout.')
    parser.add_argument('--evidence-dir', type=Path, help='New external directory for raw per-case and per-distro evidence.')
    parser.add_argument('--jobs', type=int, default=min(4, os.cpu_count() or 1))
    args = parser.parse_args()
    if sys.platform != 'linux' or os.geteuid() != 0:
        parser.error('Run as root inside Linux/WSL; fixtures use isolated chroot directories without mounts.')
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    report = args.report.resolve()
    if report.is_relative_to(Path(__file__).resolve().parents[2]):
        parser.error('Keep generated reports outside the checkout')
    evidence = args.evidence_dir.resolve() if args.evidence_dir is not None else None
    if evidence is not None and evidence.is_relative_to(Path(__file__).resolve().parents[2]):
        parser.error('Keep raw evidence outside the checkout')
    tree = ast.parse(args.reference.resolve().read_text(encoding='utf-8'))
    values = [ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign)
              and any(isinstance(target, ast.Name) and target.id == '_LINUX_SCRIPT' for target in node.targets)]
    if len(values) != 1 or not isinstance(values[0], str):
        parser.error('Reference must define one literal _LINUX_SCRIPT string')
    reference = values[0]
    action = (Path(__file__).resolve().parents[2] / 'src/action_implementation/nvme-impl.sh').read_text()
    verify_functions(reference, action)
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open('x', encoding='utf-8') as stream:
        json.dump({'schema_version': 2, 'status': 'incomplete', 'mode': 'offline-fix-fixtures'}, stream)
    if evidence is not None:
        evidence.mkdir(parents=True, exist_ok=False)
        (evidence / 'original-linux-script.sh').write_text(reference, encoding='utf-8')
        (evidence / 'candidate-nvme-impl.sh').write_text(action, encoding='utf-8')
    results = []
    with tempfile.TemporaryDirectory(prefix='alar-nvme-tests-') as temporary:
        parent = Path(temporary)
        prepare_base(parent / 'base')
        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = [pool.submit(compare, parent / 'base', parent, case, reference, action, evidence) for case in matrix()]
            for future in as_completed(futures):
                result = future.result()
                results.append(result)
                if result['status'] == 'failed':
                    print(f"FAIL {result['case']}: {result['error']}", file=sys.stderr)
    failed = sum(result['status'] == 'failed' for result in results)
    document = {'schema_version': 2, 'status': 'completed', 'mode': 'offline-fix-fixtures', 'completed_at': datetime.now(timezone.utc).isoformat(),
                'workers': args.jobs, 'total': len(results), 'passed': len(results) - failed, 'failed': failed,
                'timeout_helpers_match': True, 'fallback_rules_match': True,
                'action_sha256': hashlib.sha256(action.encode()).hexdigest(),
                'reference_sha256': hashlib.sha256(reference.encode()).hexdigest(), 'results': sorted(results, key=lambda row: row['case']),
                'evidence_directory': str(evidence) if evidence is not None else None,
                'execution_host': {'wsl_distribution': os.environ.get('WSL_DISTRO_NAME'), 'python': sys.version,
                                   'os_release': Path('/etc/os-release').read_text()}}
    if evidence is not None:
        for distro in sorted({row['case'].split('/')[0] for row in results}):
            rows = [row for row in document['results'] if row['case'].split('/')[0] == distro]
            summary = {'kind': 'offline-stub-fixture-distro-summary', 'distro_id': distro,
                       'execution_host': document['execution_host'], 'action_sha256': document['action_sha256'],
                       'reference_sha256': document['reference_sha256'], 'results': rows,
                       'raw_case_files': [f"{row['case']}.json" for row in rows], 'live_status': 'not run'}
            with (evidence / f'{distro}.json').open('x', encoding='utf-8') as stream:
                json.dump(summary, stream, indent=2)
    report.write_text(json.dumps(document, indent=2) + '\n', encoding='utf-8')
    print(f"{len(results) - failed}/{len(results)} passed; {args.jobs} parallel workers; report: {report}")
    return int(failed != 0)


if __name__ == '__main__':
    raise SystemExit(main())
