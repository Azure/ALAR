# ALAR
#### Azure Linux Auto Recover

Azure Linux Auto Recover (ALAR) is a tool to assist with the most common issues to boot a Linux based VM. ALAR is designed to be used on the CLI from another Azure based Linux VM, in order to recover the OS disk from a VM which had a boot issue. It can also be used together with 'the az vm repair' extension to automate most of the VM repair creation.

The scenarios the tool can assist you are:

* malformed /etc/fstab
  * syntax error
  * missing disk
* damaged initrd or /boot/grub/grub.cfg is missing the right setup
* last installed kernel is not bootable
* serial console and grub serial are not configured well
* GRUB/EFI installation or configuration damaged
* Disk full causing a non-boot scenario, specifically related to auditd configurations.

It has also the following features
* does support ADE. Either by decrypting the device, to be recovered, automatically
  or with the help of an ADE encryption key passed over to the tool: `--ade-password <password>`
* A custom recover disk path can be specified if `LUN0`is already occupied: `--custom-recover-disk`
* By default all action scripts are incorporated into the ALAR tool. This can be of help
  if no access to the internet does exists. Though, if required the action scripts can be downloaded with the help of the flag `--download-action-scripts`
  this may be handy if a new action is available or an existing one got improved.
* A special action `chroot-cli` allows to fix things manually if the available action scripts aren't of the right choice. All things get setup automatically. The user gets automatically placed in a terminal belonging to the associated chroot session.
This option can't be used together with 'az vm repair run'

### What actions are available
#### fstab
This action does strip off any lines in the `/etc/fstab` file which are not needed to boot a system. It makes a copy of the original file first. So, after the start of the OS the administrator is able to edit the fstab again and correct any errors which didn’t allow a reboot of the system before. This action provides the following additional functionality.
-	If device names are found they get translated to an UUID identifier
-	If '/boot' or '/boot/efi' is missed it/they get added to the fstab configuration
-	The resource disk configuration isn’t removed

#### kernel
This action does change the default kernel.
It modifies the configuration so that the previous kernel version gets booted. After the boot the admin is able to replace the broken kernel.

#### initrd
This action corrects two issues that can happen when a new kernel gets installed
1. The grub.cfg file is incorrect created
2. The initrd image is missing or corrupt

#### serialconsole
This action enables both the serialconsole and the GRUB serial. Incorrect vaules get overwritten by a set of defaults
The correct setup allows you to see the `GRUB Menu` as well get access to the system via the `Azure Serial Console`.

#### nvme
This action prepares the attached Linux guest for NVMe using the Linux preparation
logic from the Azure CLI `nvme-conversion` extension. Run it with `alar nvme`.
It preserves the original Linux script's checks and conditional fixes: built-in
and initramfs driver detection, NVMe/Hyper-V PCI driver preparation,
`nvme_core.io_timeout=240`, optional BLS updates, fallback Azure NVMe udev rules,
and SCSI-to-UUID fstab conversion. Existing ready configurations are skipped.
The original distro-specific branches, legacy release-file detection, image
exclusions and diagnostic/error reporting are retained; no additional health
checks or Azure VM size/controller changes are introduced.

The script retains branches for Ubuntu, Debian, RHEL/CentOS/Rocky/AlmaLinux,
SLES, Oracle Linux, Azure Linux and Mariner. It uses installed guest kernels
rather than the running rescue kernel. Live udev reload is deferred to the guest's
next boot. Script coverage does not guarantee that every image or VM size supports
NVMe.

On RHEL/CentOS 7, a standalone EFI configuration selected by `/etc/grub2-efi.cfg`
is checked and regenerated at that actual target. Every kernel entry must have
exactly one `nvme_core.io_timeout=240` argument. Broken links, redirect stubs and
failed or ineffective regeneration are reported rather than accepted from an
unrelated GRUB configuration. Other release and BIOS paths retain the shared
script's behavior.

Fstab device identities are resolved from mounted guest filesystems, including
other partitions on those same disks. Unmounted data disks and swap can also be
resolved with an explicit, guest-visible device map. Set `ALAR_NVME_DEVICE_MAP`
to a file containing one original-device rescue-device pair per line, for example:

```text
/dev/sda /dev/sdd
/dev/disk/azure/scsi1/lun0 /dev/nvme1n1
```

Mappings can name whole disks or individual partitions. Use only independently
identified attached guest devices, not rescue OS devices. For example, with the
map saved on the rescue VM as `/tmp/nvme-device-map`, run
`ALAR_NVME_DEVICE_MAP=/tmp/nvme-device-map alar nvme`.
Unresolved devices are left unchanged with a warning; the action does not guess
that an old guest device name still identifies the same disk on the rescue VM.
The original fstab backup remains `/etc/fstab.bak`.

ALAR always applies the conditional fixes. Within an already prepared guest
chroot, `bash /tmp/action_implementation/nvme-impl.sh -fix` is equivalent to
running the script without arguments. Check-only and dry-run modes are not
supported; no additional flags are added to ALAR's CLI.

**Known framework limitation:** some Oracle Linux disk layouts are classified as
ADE-encrypted before ALAR reaches the action. This action does not change disk
discovery or ADE handling and cannot bypass that limitation.

The original fix-path diagnostic and exit behavior is preserved: review the
action output, not just ALAR's exit status, when evaluating preparation results.

#### grubfix
This action is reinstalling `GRUB` and regenerates the `grub.cfg` file

#### efifix
This action is reinstalling the required software to boot from a `GEN2 VM`. The `grub.cfg` is regenerated as well.

#### auditd
This action will alter the auditd configuration, replacing any HALT directives in the `/etc/audit/auditd.conf` file. Also in LVM environments, if the volume containing the audit logs is full, and free space is available in the volume group, the logical volume will be extended by 10% of the current size.

#### sudo
The `sudo` action will reset the permissions on the `/etc/sudoers` file and all files in `/etc/sudoers.d` to the required 0440 modes as well as check other best practices.  A basic check is run to detect and report on duplicate user entries and move *only* the `/etc/sudoers.d/waagent` file if it is found to conflict with other files.

#### corrupt
This action does not do any processing on its own in the initial version, it is a vehicle for calling the built-in ALAR functionality
for mounting and doing some light error checking when the chroot environment is assembled.  If some further checking is required this
can be built into a future version.

### How to use ALAR
ALAR can be used either from the CLI of an existing Azure VM or with the help of the
vm-repair extension for the Azure CLI tool.

#### From a SHELL prompt
In the simplest form: `alar <action-name>` i.e. `alar fstab`
If a specific disk and the ADE disk-encryption key is required: `alar <action-name> --custom-recover-disk <disk> --ade-password <key>` i.e. `# alar initrd --custom-recover-disk /deV/sdd --ade-password "password in base64 format"`

#### From the Azure CLI
Utilizing ALAR with the help of the Azure CLI is quite simple.
Create at fist a recover VM. We assume your VM named suse15 in the resource-group
  `az vm repair create --verbose -g suse15_group -n suse15 --repair-username rescue --repair-password 'password!234'`

In the next step the `run` command option is used to execute ALAR on the repair VM created before.

  `az vm repair run --verbose -g suse15_group -n suse15 --run-id linux-alar2 --parameters initrd --run-on-repair`

As a final step the `restore` command is used
  `az vm repair restore --verbose -g suse15_group -n suse15`

Detailed information about the `vm-repair extension` is documented at https://learn.microsoft.com/en-us/cli/azure/vm/repair?view=azure-cli-latest

#### What to do if more than one ACTION is require?
If more than one action has to be applied this is possible as well. Pass over both to ALAR separated by a comma i.e. ‘fstab,initrd’

**NOTE**
No spaces allowed!

## LICENSE
Licensed under either of
* Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)