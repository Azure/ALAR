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
    resolv-pre
    device=$(cut -c -$((${#boot_part_path} - 1)) <<<$boot_part_path)
    umount $efi_part_path
    mkfs.vfat -F16 $efi_part_path
    mount $efi_part_path /boot/efi
    yum reinstall -y grub2-efi shim
    grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg
    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    read -ra EFI_DISK <<<$(blkid $efi_part_path)
    new_uuid=$(for i in "${EFI_DISK[@]}"; do grep ^UUID= <<<$i; done)
    sed -i "s/$uuid_to_be_replaced/$new_uuid/" /etc/fstab
    
    resolv-after
}

recover_suse() {
    resolv-pre
    device=$(cut -c -$((${#boot_part_path} - 1)) <<<$boot_part_path)
    umount $efi_part_path
    mkfs.vfat -F16 $efi_part_path
    mount $efi_part_path /boot/efi
    zypper remove -y grub2
    zypper install -y grub2
    zypper install -y shim
    cp /etc/default/grub.rpmsave /etc/default/grub
    shim-install
    grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "sles")/grub.cfg
    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    read -ra EFI_DISK <<<$(blkid $efi_part_path)
    new_uuid=$(for i in "${EFI_DISK[@]}"; do grep ^UUID= <<<$i; done)
    sed -i "s/$uuid_to_be_replaced/$new_uuid/" /etc/fstab

    resolv-after
}

recover_ubuntu() {
    resolve-pre

    umount $efi_part_path
    mkfs.vfat -F16 $efi_part_path
    mount $efi_part_path /boot/efi
    apt-get install -y --reinstall grub-efi
    grub-install --efi-directory=/boot/efi --target=x86_64-efi $device
    update-grub
    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    read -ra EFI_DISK <<<$(blkid $efi_part_path)
    new_uuid=$(for i in "${EFI_DISK[@]}"; do grep ^UUID= <<<$i; done)
    sed -i "s/$uuid_to_be_replaced/$new_uuid/" /etc/fstab

    resolv-after
}

if [[ "$isRedHat" == "true" ]]; then
    recover_redhat
fi

if [[ "$isSuse" == "true" ]]; then
    recover_suse
fi

if [[ "$isUbuntu" == "true" ]]; then
    recover_ubuntu
fi

exit 0
