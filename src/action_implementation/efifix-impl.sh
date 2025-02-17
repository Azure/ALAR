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

    efi_part_path=$(findmnt -n -o SOURCE /boot/efi)
    if [[ -z ${efi_part_path}  ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi

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

recover_azurelinux() {
    resolv-pre

    efi_part_path=$(findmnt -n -o SOURCE /boot/efi)
    if [[ -z ${efi_part_path}  ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi

    # install the missing dosfstools package
    # we need it to get the mkfs.vfat command
    dnf install dosfstools -y

    umount $efi_part_path
    mkfs.vfat -F16 $efi_part_path
    mount $efi_part_path /boot/efi
    # reinstall the grub2-efi and shim packages
    # install the grub2-efi package if it is not installed
    dnf install grub2-efi -y
    dnf reinstall -y grub2-efi 
    dnf reinstall grub2-efi-binary -y
    dnf install shim -y
    dnf reinstall shim -y
    mkdir -p /boot/efi/boot/grub2
    cd /boot/efi/boot/grub2
    echo "search -n -u 514db688-ab4b-4373-82f8-ca29317b6879 -s" >> grub.cfg
    echo "# For images using grub2-mkconfig, $prefix is the variable" >> grub.cfg
    echo "# grub expects to be populated with the proper path to the grub.cfg, grubenv." >> grub.cfg
    echo "#    - $prefix: the path to /boot/grub2/ relative to the bootUUID" >> grub.cfg
    echo 'set prefix=($root)/"grub2"' >> grub.cfg
    echo 'configfile $prefix/grub.cfg' >> grub.cfg

    # The UUID of the boot partition is hardcoded in the grub.cfg file
    # This is a workaround to replace it with the correct UUID
    # The UUID of the boot partition can be found by running the following command:
    # lsblk -f -o UUID $(findmnt /boot -o SOURCE -n) -n
    # The output of this command will be used to replace the hardcoded UUID in the grub.cfg file 
    boot_part_uuid=$(lsblk -f -o UUID $(findmnt /boot -o SOURCE -n) -n)
    sed -i "s/514db688-ab4b-4373-82f8-ca29317b6879/$boot_part_uuid/" grub.cfg
    cd /

    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    read -ra EFI_DISK_INFO <<<$(blkid $efi_part_path)
    new_uuid=$(for i in "${EFI_DISK_INFO[@]}"; do grep ^UUID= <<<$i; done)
    sed -i "s/$uuid_to_be_replaced/$new_uuid/" /etc/fstab
    grub2-mkconfig -o /boot/grub2/grub.cfg


    ####
    #### Für EFI muss die grub.cfg kopiert werden. Das default setup funktioniert nicht. Muss überprüft werden!
    ####
    
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

if [[ "$isAzureLinux" == "true" ]]; then
    recover_azurelinux
fi

exit 0
