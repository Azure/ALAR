pub(crate) static RESCUE_DISK: &str = "/dev/disk/azure/scsi1/lun0";
pub(crate) static RESCUE_BEK: &str = "/srv/rescue-bek/";
pub(crate) static RESCUE_BEK_BOOT: &str = "/srv/rescue-bek-boot";
pub(crate) static RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME: &str = "/srv/rescue-bek/LinuxPassPhraseFileName";
pub(crate) static RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME: &str = "/tmp/LinuxPassPhraseFileName";
pub(crate) static ASSERT_PATH: &str = "/tmp/assert";
pub(crate) static ASSERT_PATH_USR: &str = "/tmp/assert/usr";
pub(crate) static OS_RELEASE: &str = "/tmp/assert/etc/os-release";
pub(crate) static ADE_OSENCRYPT_PATH: &str = "/dev/mapper/rescueencrypt";
pub(crate) static INVESTIGATEROOT_DIR: &str = "/investigateroot";
pub(crate) static RESCUE_ROOTVG: &str = "rootvg";
pub(crate) static ROOTVG_ROOTLV: &str = "/dev/rootvg/rootlv";
pub(crate) static ROOTVG_USRLV: &str = "/dev/rootvg/usrlv";
pub(crate) static RESCUE_ADE_USRLV: &str = "/dev/rootvg/usrlv";
pub(crate) static RESCUE_ADE_ROOTLV: &str = "/dev/rootvg/rootlv";
pub(crate) static RESCUE_ROOT_BOOT: &str = "/srv/rescue-root/boot";
pub(crate) static RESCUE_ROOT_BOOT_EFI: &str = "/srv/rescue-root/boot/efi";
pub(crate) static SUPPORT_FILESYSTEMS: &str = "dev proc sys tmp dev/pts run";
pub(crate) static ACTION_IMPL_DIR: &str = "/tmp/action_implementation";
pub(crate) static CHROOT_CLI: &str = "chroot-cli";
pub(crate) static TARBALL: &str = "https://github.com/Azure/ALAR/tarball/master";
pub(crate) static RESCUE_ROOT: &str = "/srv/rescue-root/";
// Our builtin action scripts
pub(crate) static AUDIT_IMPL_FILE: &str =  include_str!("action_implementation/auditd-impl.sh");
pub(crate) static EFIFIX_IMPL_FILE: &str =  include_str!("action_implementation/efifix-impl.sh");
pub(crate) static FSTAB_IMPL_FILE: &str =  include_str!("action_implementation/fstab-impl.sh");
pub(crate) static GRUB_AKW_FILE: &str =  include_str!("action_implementation/grub.awk");
pub(crate) static GRUBFIX_IMPL_FILE: &str =  include_str!("action_implementation/grubfix-impl.sh");
pub(crate) static HELPERS_SH_FILE: &str =  include_str!("action_implementation/helpers.sh");
pub(crate) static INITRD_IMPL_FILE: &str =  include_str!("action_implementation/initrd-impl.sh");
pub(crate) static KERNEL_IMPL_FILE: &str =  include_str!("action_implementation/kernel-impl.sh");
pub(crate) static SAFE_EXIT_FILE: &str =  include_str!("action_implementation/safe-exit.sh");
pub(crate) static SERIALCONSOLE_IMPL_FILE: &str =  include_str!("action_implementation/serialconsole-impl.sh");
pub(crate) static TEST_IMPL_FILE: &str =  include_str!("action_implementation/test-impl.sh");