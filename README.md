# Azure Linux Auto Recover v2

The Azure Linux Auto Recover v2 (ALAR) tool is intended to fix the most common boot issues.

The main intend of this tool is to solve no boot scenarios in the simplest form possible. It is still the obligation of the Adminstrator to re-apply the right configuration after the recovery.

A backup copy of the OS image is always possible in case one needs access to it. This functionality is provided by the `az vm repair extension`


The most common scenarios which are covered at the moment are:

* malformed /etc/fstab 
  * syntax error
  * missing disk
* damaged initrd or missing initrd line in the /boot/grub/grub.cfg
* last installed kernel is not bootable
* serial console and grub serial are not configured well
* GRUB/EFI installation or configuration damaged
* Disk full causing a non-boot scenario, specifically related to auditd configurations.

The following action names need to be be used to get a certain scenario fixed 
### fstab
This action does strip off any lines in the `/etc/fstab` file which are not needed to boot a system. It makes a copy of the original file first. So after the start of the OS the administrator is able to edit the fstab again and correct any errors which didn’t allow a reboot of the system before. This action provides the following additional functionality.
-	If device names are found they get translated to an UUID identifier
-	If '/boot' or '/boot/efi' is missed it/they get added to the fstab configuration
-	The resource disk configuration isn’t removed


### kernel
This action does change the default kernel.
It modifies the configuration so that the previous kernel version gets booted. After the boot the admin is able to replace the broken kernel.

### initrd
This action corrects two issues that can happen when a new kernel gets installed 
1. The grub.cfg file is incorrect created
2. The initrd image is missing or corrupt

### serialconsole
This action enables both the serialconsole and the GRUB serial. Incorrect vaules get overwritten by a set of defaults
The correct setup allows you to see the `GRUB Menu` as well get access to the system via the `Azure Serial Console`.

### grubfix
This action is reinstalling `GRUB` and regenerates the `grub.cfg` file

### efifix
This action is reinstalling the required software to boot from a `GEN2 VM`. The `grub.cfg` is regenerated as well.

### auditd
This action will alter the auditd configuration, replacing any HALT directives in the `/etc/audit/auditd.conf` file. Also in LVM environments, if the volume containing the audit logs is full, and free space is available in the volume group, the logical volume will be extended by 10% of the current size.

### How can I recover my failed VM?
The ALAR tool can be used [standalone](doc/standalone.md) or with the help of the `az vm repair extension` which simpplifies the creation of a recovery VM. 

#### Example ####
    az vm repair create --verbose -g centos7 -n cent7 --repair-username rescue --repair-password 'password!234'

    az vm repair run --verbose -g centos7 -n cent7 --run-id linux-alar2 --parameters initrd --run-on-repair

    az vm repair restore --verbose -g centos7 -n cent7

Either a single recover-operation or multiple operations, i.e., fstab; ‘fstab,initrd’ are possible

**NOTE**
Separate the recover operation with a comma in this case – no spaces allowed!

### Limitations
* Classic VMs are not supported
* ALAR is only supported to utilize an Ubuntu 18.04 (the default) or Ubuntu 20.04 system as the rescue VM.

### Feature
* Support for ADE enabled OS disks is available with the help of the [az vm repair extension](https://learn.microsoft.com/en-us/cli/azure/vm/repair?view=azure-cli-latest). Consult this [document](https://learn.microsoft.com/en-us/troubleshoot/azure/virtual-machines/repair-linux-vm-using-azure-virtual-machine-repair-commands) for further information about its usage.

### Distributions supported
* CentOS/Redhat 6.8 - 9.x

  **NOTE**

  RedHat 9.x requires to use an Ubuntu 20.04 as the recover OS. The creation of the recover VM needs to be performed with this command
  >az vm repair create --verbose -g centos7 -n cent7 --repair-username rescue --repair-password 'password!234’ --distro ubuntu20
* Ubuntu 16.04 LTS, 18.04 LTS, 20.04 LTS, 22.04 LTS
* Suse 12 and 15
* Debain 9, 10, 11

