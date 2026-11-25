# Linux NVMe action tests

These are offline regression tests for the built-in `nvme` action. They do not
deploy VMs, contact Azure or SSH, mount filesystems, install packages, or add
runtime health checks. Session ledgers, live deployment helpers, quota tooling
and raw cloud evidence are not part of this suite.

## Requirements

- Linux root, including root inside WSL, and Python 3.10+.
- Existing `/usr/sbin/chroot`, `ldd`, Bash, coreutils, grep, sed, awk, diff and
  find. The runner copies the required tools and their interpreters/libraries
  into disposable chroots; missing prerequisites fail the run.
- Each guest uses fake files and stubbed device/repair commands. Tests never
  access real guest disks or use the rescue host's device identities.

Non-Linux or non-root hosts skip the chroot test classes. A skipped suite is not
successful execution or live NVMe acceptance.

## Self-contained regression suite

From the repository root:

```bash
sudo python3 -B -m unittest discover -s tests/nvme -p 'test_*.py' -v
```

The 25 tests need only this ALAR checkout, not a sibling CLI repository:

| File | Coverage |
| --- | --- |
| [test_offline.py](test_offline.py) | Seven guest-kernel, multi-kernel, explicit swap mapping, ambiguous device, invalid/missing map and missing-kernel regressions. |
| [test_efi_timeout.py](test_efi_timeout.py) | Thirteen RHEL 7 standalone EFI regressions: stale configurations, exact/all-kernel timeout tokens, nonmutation, idempotence, broken links, redirect stubs, BIOS/modern releases and failed/ineffective generation. |
| [test_runner.py](test_runner.py) | Five harness regressions: raw success/failure evidence, equally broken scripts, unexpected success and copied shebang interpreters. |

EFI cases extract the actual action function bodies instead of maintaining a
second copy of the timeout algorithm.

## Optional shared-source comparison

[run_tests.py](run_tests.py) compares 191 isolated scenarios across 12 distro IDs,
driver states, check/fix/dry-run modes, legacy release files, GRUB/BLS/udev
variants, image exclusions and missing/failing tools. It first compares shared
function bodies and fallback rules, allowing only the declared offline adapters:
installed guest kernels, explicit guest-device resolution, deferred udev reload,
and ALAR's fix default/check-only flag.

Supply the Azure CLI extension source explicitly. It is parsed with Python's
AST, never imported. The reference used for this implementation includes the
RHEL 7 EFI correction:
[`_linux_script.py` at commit `2ac43a5b`](https://github.com/jonathanbrenes/azure-cli-extensions/blob/2ac43a5b1f338177a2da1c114cae6c674d03caec/src/nvme-conversion/azext_nvme_conversion/_linux_script.py).
An older or changed reference that fails function comparison is not equivalent
coverage and must not be silently substituted.

```bash
stamp=$(date +%Y%m%d-%H%M%S)-$$
sudo python3 -B tests/nvme/run_tests.py \
  --reference /path/to/azure-cli-extensions/src/nvme-conversion/azext_nvme_conversion/_linux_script.py \
  --report "/tmp/alar-nvme-${stamp}.json" \
  --evidence-dir "/tmp/alar-nvme-${stamp}-evidence" \
  --jobs 4
```

Reports/evidence must be new paths outside the checkout. Workers are independent,
default to at most four, and each shell phase has a 60-second timeout. The runner
rejects unexpected exits, including equally broken scripts, and asserts that
check/dry-run leave protected guest files unchanged.

The JSON report records completion status, actual counts, source hashes and
per-case results. Optional evidence retains both scripts, raw stdout/stderr,
exits, command transcripts, before/after contents and modes, and distro summaries.
Only comparison output is normalized; raw evidence is preserved. An incomplete
report, a printed error with exit zero, or a passing fixture is not successful
NVMe boot certification. Exit 0 means all comparisons passed; 1 means completed
comparisons failed; argument errors return 2. Setup failures also fail the run.

## Build checks

Use the repository's existing Rust toolchain/dependencies:

```bash
bash -n src/action_implementation/nvme-impl.sh
cargo test --locked
cargo build --release --locked
```

The action is embedded and materialized through the existing built-in script
mechanism; there are no new Rust dependencies or changes to action execution,
disk discovery or ADE handling.

## Completed live coverage and boundaries

- The x64 campaign exercised the action, actual SCSI/NVMe boots, repeat behavior
  and timeout faults. RHEL 7.9's standalone EFI correction passed real NVMe boot
  and effective-timeout-240 checks after the original false-readiness failure.
- Native ARM64 GNU and static musl builds succeeded. The static candidate passed
  full action phases, actual boots, runtime timeout checks, repeat and fault tests
  on Ubuntu 26.04, Debian 13 and Oracle Linux 10.2 ARM64.
- Oracle Linux 8.10, 9.8 and 9.7 x64 passed full action phases, 12 actual boots,
  six runtime-timeout-240 assertions, repeat and fault checks **only with a
  separate temporary framework candidate**. Its patch was reverted. The
  unchanged framework can still misclassify those plaintext layouts as ADE
  before reaching this action; no framework fix is included in this change.
- Genuine ADE recovery with valid, missing or wrong credentials was not
  live-tested. Existing ADE code was not changed.
- Reliable post-conversion SSH remains unverified. Run Command and actual boot
  evidence do not certify SSH connectivity.
- Legacy SCSI-only image component tests are not NVMe conversion certification.
  Windows and universal image/SKU compatibility are out of scope.

Live results describe the tested candidates, not an automatic test run by this
suite. Full logs, candidate/source hashes and per-image evidence are retained
separately; no local machine paths, credentials or cloud resource inventories
are required to run the checked-in tests.
