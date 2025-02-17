#!/bin/bash
# In case the resolv.conf isn't set correct use the default resolver
resolv-pre() {
    mv /etc/resolv.conf /etc/resolv.conf.org
    echo "nameserver 168.63.129.16" >/etc/resolv.conf
}

# restore the originail resolv.conf
resolv-after() {
    mv /etc/resolv.conf.org /etc/resolve.conf
}

recover_redhat() {
    if [[ "${DISTROVERSION}" =~ 6 ]]; then
        echo "RedHat 6.x is not supported."
        exit 1
    fi

    resolv-pre
    yum install gdisk -y
    sgdisk -e "${RECOVER_DISK_PATH}"
    grub2-install --target i386-pc "${RECOVER_DISK_PATH}"

    if [[ $? -ne 0 ]]; then
        echo "Failed to install grub2 on ${RECOVER_DISK_PATH}"
        echo "Do you use it on an GEN2 disk?"
        exit 1
    fi
    # Generate both config files.
    # TODO - check if we need to generate both. Newer distro versiondon't require this anymore. Let us create a backup therefore.
    cp /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg.bak
    grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg
    grub2-mkconfig -o /boot/grub2/grub.cfg

    resolv-after
}

recover_suse() {
    resolv-pre
    zypper install -y gptfdisk
    sgdisk -e "${RECOVER_DISK_PATH}"
    grub2-install "{$RECOVER_DISK_PATH}"
    grub2-mkconfig -o /boot/grub2/grub.cfg

    resolv-after
}

recover_ubuntu() {
    resolve-pre

    apt-get install gdisk -y
    apt-get install -y --reinstall -o Dpkg::Options::="--force-confold" grub2-common grub-pc
    sgdisk -e "${RECOVER_DISK_PATH}"
    grub-install "${RECOVER_DISK_PATH}"
    update-grub

    resolv-after
}

recover_azurelinux() {
    resolv-pre

    tdnf install gdisk -y
    tdnf install grub2-pc -y
    sgdisk -e "${RECOVER_DISK_PATH}"
    grub2-install --target i386-pc "${RECOVER_DISK_PATH}"

    if [[ $? -ne 0 ]]; then
        echo "Failed to install grub2 on ${RECOVER_DISK_PATH}"
        echo "Do you use it on an GEN2 disk?"
        exit 1
    fi
    grub2-mkconfig -o /boot/grub2/grub.cfg

    resolv-after
}

if [[ "$isRedHat" == "true" ]]; then
    recover_redhat
fi

if [[ "$isSuse" == "true" ]]; then
    recover_suse
fi

if [[ "$isUbuntu" == "true" || "$isDebian" == "true" ]]; then
    recover_ubuntu
fi

if [[ "$isAzureLinux" == "true" ]]; then
    recover_azurelinux
fi

exit 0
