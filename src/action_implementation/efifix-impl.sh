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
    yum reinstall -y grub2-efi-x64 shim-x64
    yum reinstall grub2-common -y
    
    GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg
    GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg
    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    new_efi_uuid=$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))
    sed -i "s/$uuid_to_be_replaced/UUID=$new_efi_uuid/" /etc/fstab
    
    resolv-after
}

recover_suse() {
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
    zypper remove -y grub2-x86_64-efi
    zypper install -y grub2-x86_64-efi
    zypper remove -y shim
    zypper install -y shim
    cp /etc/default/grub.rpmsave /etc/default/grub
    shim-install
    

    boot_uuid=$(blkid -s UUID -o value $(findmnt /boot -o SOURCE -n))
echo "search --no-floppy --fs-uuid --set=dev $boot_uuid" >  /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "sles")/grub.cfg   
echo 'set prefix=($dev)/grub2
export $prefix
configfile $prefix/grub.cfg' >>  /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "sles")/grub.cfg

    
    grub2-mkconfig -o /boot/grub2/grub.cfg
    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    new_efi_uuid=$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))
    sed -i "s/$uuid_to_be_replaced/UUID=$new_efi_uuid/" /etc/fstab

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

    # The UUID of the boot partition is hardcoded in the grub.cfg file
    # This is a workaround to replace it with the correct UUID
    # The UUID of the boot partition can be found by running the following command:
    # lsblk -f -o UUID $(findmnt /boot -o SOURCE -n) -n
    # The output of this command will be used to replace the hardcoded UUID in the grub.cfg file 


    boot_uuid=$(blkid -s UUID -o value $(findmnt /boot -o SOURCE -n))
    echo "search -n -u $boot_uuid -s" >  grub.cfg   
    echo 'set prefix=($root)/grub2
export $prefix
configfile $prefix/grub.cfg' >>  grub.cfg

    cd /

    uuid_to_be_replaced=$(awk '/efi/ {print($1)}' /etc/fstab)
    new_efi_uuid=$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))
    sed -i "s/$uuid_to_be_replaced/UUID=$new_efi_uuid/" /etc/fstab

    # Load the script and run the AzureLinux specific parts
    # We still run in a chroot context with specific environment variables set.
    bash /tmp/action_implementation/initrd-impl.sh
    grub2-mkconfig -o /boot/grub2/grub.cfg

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
    sed -i "s/$uuid_to_be_replaced/UUID=$new_efi_uuid/" /etc/fstab

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
