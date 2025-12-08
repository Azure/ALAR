# Changelog


All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
