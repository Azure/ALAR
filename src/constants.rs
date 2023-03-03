pub(crate) static RESCUE_DISK: &str = "/dev/disk/azure/scsi1/lun0";
pub(crate) static RESCUE_ROOT: &str = "/srv/rescue-root/";
pub(crate) static RESCUE_ROOT_RUN: &str = "/srv/rescue-root/run";
pub(crate) static RESCUE_ROOT_BOOT: &str = "/srv/rescue-root/boot";
pub(crate) static RESCUE_ROOT_BOOT_EFI: &str = "/srv/rescue-root/boot/efi";
pub(crate) static RESCUE_ROOT_USR: &str = "/srv/rescue-root/usr";
pub(crate) static RESCUE_ROOT_VAR: &str = "/srv/rescue-root/var";
pub(crate) static SUPPORT_FILESYSTEMS: &str = "dev proc sys tmp dev/pts";
pub(crate) static ASSERT_PATH: &str = "/tmp/assert";
pub(crate) static REDHAT_RELEASE: &str = "/tmp/assert/etc/redhat-release";
pub(crate) static OS_RELEASE: &str = "/tmp/assert/etc/os-release";
pub(crate) static OSENCRYPT_PATH: &str = "/dev/mapper/osencrypt";
pub(crate) static ACTION_IMPL_DIR: &str = "/tmp/action_implementation";
pub(crate) static INVESTIGATEROOT_DIR: &str = "/investigateroot";
pub(crate) static INVESTIGATEROOT_BOOT_DIR: &str = "/investigateroot/boot";
pub(crate) static INVESTIGATEROOT_EFI_DIR: &str = "/investigateroot/boot/efi";
pub(crate) static CHROOT_CLI: &str = "chroot-cli";
pub(crate) static TARBALL: &str = "https://github.com/Azure/ALAR/tarball/master";
