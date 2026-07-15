# Changelog


All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# 1.4.2 (2026-07-15)
What's New

Bug Fixes & Reliability Improvements

• Mixed SCSI/NVMe controller support — Fixed a crash (core dump) when a VM has both SCSI and NVMe controllers present. ALAR now correctly falls back to SCSI in mixed environments.
• Virtio disk support — Added  /dev/vd*  (virtio) disk detection, broadening compatibility with non-Azure hypervisors and test environments.
• NVMe disk path caching — Prevented a race condition where calling  get_recovery_nvme_disk_path()  after the recovery disk was already mounted could return incorrect results.
• Architecture detection — Replaced fragile  file /bin/bash  inspection (requiring a live mount) with  std::env::consts::ARCH  for reliable x86_64/aarch64 detection.

EFI Recovery Improvements ( efifix )

• RHEL/CentOS: Fixed  kernel-install add  to include the initramfs path; improved EFI  grub.cfg  generation and fstab UUID replacement.
• SUSE: EFI partition is now recreated ( mkfs.vfat  + remount) before reinstalling boot packages; fixed ARM64 package name ( grub2-arm64-efi ).
• Azure Linux: Added version-aware recovery (v3 vs v4) with architecture-specific EFI package handling; added fallback for systems without a dedicated  /boot  partition; more reliable fstab EFI entry update.
• Ubuntu: Fixed non-interactive  apt install  ( -y ) and corrected fstab UUID replacement pattern.

GRUB / initrd Improvements

• BLS (Boot Loader Specification) support —  kernel-impl  and  initrd-impl  now detect  GRUB_ENABLE_BLSCFG=true  and regenerate BLS loader entries via  kernel-install add  before running  grub2-mkconfig . This fixes boot failures on RHEL 8+/Azure Linux with BLS-style GRUB configs.
• RHEL kernel recovery — RHEL 8+ now uses  grubby --set-default  to pin the target kernel and rewrites the EFI  grub.cfg  with the correct boot UUID instead of relying on  GRUB_DEFAULT=1 .
• Added  GRUB_DISABLE_OS_PROBER=true  to prevent OS prober from interfering with config generation in chroot environments.

Script Environment

• Repair scripts now receive  ARCHITECTURE  and  ACTION_DIR  environment variables, enabling architecture-aware logic in shell scripts.
• Script output now streams in real-time to the console (previously buffered until completion).

Code Quality

• Telemetry structs refactored to idiomatic Rust ( snake_case  +  serde rename_all ) removing  #[allow(non_snake_case)]  workarounds.
• Improved error diagnostics:  lsblk -f  output is now included in telemetry when OS partition detection fails.
• Dependency updates ( Cargo.lock ).

## 1.4.1 (2026-03-10)
Rewrote 'fstab' action in python3
  - more flexible handling of LVM and 'spec' field options
Added helpers.py to go with new fstab

## 1.3.3 (2025-12-15)
Enhanced 'sudo' action to include more checks
  - rev sudo-impl.sh to 1.1.0
    - add sudo setuid check
    - add /etc check
Updated helpers.sh
  - rev helpers.sh to 1.2.0
    - adding OS detection, first used in sudo-impl.sh
    - refactor the check functions into a check and fix functions so perms/owners can check w/o fixing

## 1.3.2 (2025-12-12)
  - Minimal refactoring of the code.
  - Add 'partx' support as replacement for sgdisk on RHEL 10

## 1.3.1 (2025-12-08)
Support for Telemetry is added. Just basic information get tracked:
 - Repair and Recovery VM distro name and version
 - action name
 - architecture
 - What initiator (CLI, RecoverVM, SelfHelp)
 - Any error logged
These information assist to improve existing actions and the base framework.

- Added support for the NVME controller type
- The repair of a LVM based recover OS disk with the help of a recover VM which is also
  LVM based is limited. Only supported is RHEL version < 9


## 1.1.0 (2025-10-31)
Added sudo implementation 1.0.0 and updated helpers.sh to 1.1.0 with related functions

## 1.0.7 (2025-04-30)
Fixed issue #22 auditd action not recognized. This was a spelling issue in the main code.

## 1.0.6 (2025-04-30)
Finalized the support for AzureLinux. No new features added.

## 1.0.5 (2025-03-10)
Several bug fixes and improvments added to ALAR base code and the action scripts.
Added support for AzureLinux but not finalized. Needs further validation before officially documented.

## 1.0.0 (2024-10-31)
No new added functionality. Only minor changes added or where necessary bugs got fixed to move to the 1.0.0 version

## 0.9.0 (2024-10-28)

### Changed
ALAR version 0.9.0 represents a redesign of ALAR which was coming into age and not flexible enough to cope with different distros and their disk layouts.
The new version is distro agnostic. Which means we don't rely on any predictions what the disk layout may look like. With the new design ALAR should be able to cope
with any kind of disk layout. The main focus for the redesign of ALAR is to use ALAR standalone without the usage of the vm-repair extension. And allow to get the system being recovered from an existing VM.

### Added
- Support for ADE. Password to decrypt the disk gets read from the BEK disk automatically if available.
  Otherwise the password can be passed over via the new option '--ade-password'
- Instead of the default disk (LUN 0) to get recovered by ALAR a different disk can be used instead.
  Use the new option '--custom-recover-disk'
- The action scripts are part of the binary build. It is not required to get the downloaded
  from the GIT repository. This may be of help for those conditions where access to the Internet isn't permitted. But if required it is possible to download them with the help of the new option '--download-action-scripts'. This may be handy if there is a new bug fix available in the repo but no new build got generated.
- Logging functionality added. Use RUST_LOG = debug|error|info
