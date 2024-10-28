# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.9.0 (2024-10-28)

### Changed
ALAR version 0.9.0 represents a redesign of ALAR which where coming into age and not flexible enough to cope with different distros and their disk layouts.
The new version is distro agnostic. Which means we don't rely on any predictions what the disk layout may look like. With the new design ALAR should be able to cope 
with ay kind of disk layout. The main focus for the redesign of ALAR is to use ALAR standalone without the usage of the vm-repair extension. And allow to get the 
a system being recovered from an existing VM.

### Added
- Support for ADE. Password to decrypt the disk gets read from the BEK disk automatically if available.
  Otherwise the password can be passed over via the new option '--ade-password'
- Instead of the default disk (LUN 0) to get recovered by ALAR a different disk can be used instead.
  Use the new option '--custom-recover-disk'
- The action scripts are part of the binary build. It is not required to get the downloaded from the GIT repository.
  This may be of help for those conditions where access to the Internet isn't permitted. But if required it is possible
  to download them with the help of the new option '--download-action-scripts'. This may be handy if there is a new bug fix available in the repo but no new build got generated.
