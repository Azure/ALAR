#!/bin/bash

# The intention of the fstab action is to remove any line which isn't required to allow a successfully boot of the system.
# After the fstab got altered and the system was able to be booted, the administrator needs to reapply the previous mount lines. 
# With a special attention on the right UUID value of the filesystem to be mounted and the option 'nowait'added to he list of options.

# In case the resolv.conf isn't set correct use the default resolver
resolv-pre() {
    mv /etc/resolv.conf /etc/resolv.conf.org
    echo "nameserver 168.63.129.16" >/etc/resolv.conf
}

# restore the originail resolv.conf
resolv-after() {
    mv /etc/resolv.conf.org /etc/resolv.conf
}

# Save the existing fstab file
timestamp=$(date +%s)
mv -f /etc/fstab{,.copy-${timestamp}}
fstab_org="/etc/fstab.copy-${timestamp}"

resolv-pre
# For Debian we need to instal gawk first. It comes only with mawk
if [[ -f /usr/bin/apt ]]; then
    apt-get install -qq -y gawk
fi
resolv-after


boot_efi_mnt() {
    fstab_boot=$(awk '/[[:space:]]+\/boot[[:space:]]+/ {print}' ${fstab_org})
    # Non every distro has /boot on a seperate disk/partition
    # Hence let us verify this before performing any unnecessary modification
    if [[ -n ${fstab_boot} ]]; then 
        # A mount entry for boot is defiend is it UUID based?
        if [[ "$fstab_boot" =~ ^[[:space:]]*UUID.*  ]]; then
            echo "$fstab_boot" >> /etc/fstab
        else
            # It is device name based, let us convert it to UUID based
            fstab_boot_dev=$(awk '{print $1}'<<< "$fstab_boot")
            #fstab_boot_uuid=$(blkid -o value -s UUID $(awk '{print $1}'<<< "$fstab_boot"))
            # Variable boot_part_path is set by ALAR
            fstab_boot_uuid=$(blkid -o value -s UUID ${boot_part_path})
            sed "s|$fstab_boot_dev|UUID=$fstab_boot_uuid|" <<< $fstab_boot >> /etc/fstab
        fi
    else
        # We need to add the /boot to the fstab if we have a boot partition
        if [[ -n ${boot_part_path} ]]; then
            boot_part_fs=$(lsblk -l -o FSTYPE ${boot_part_path} | tail -n 1)
            echo "UUID=$(blkid -o value -s UUID ${boot_part_path}) /boot ${boot_part_fs} defaults 0 0" >> /etc/fstab
        fi
    fi


    fstab_efi=$(awk '/[[:space:]]+\/boot\/efi[[:space:]]+/ {print}' ${fstab_org})
    if [[  -n ${fstab_efi} ]]; then 

        if [[ "$fstab_efi" =~ ^[[:space:]]*UUID.*  ]]; then
            echo "$fstab_efi" >> /etc/fstab
        else
            fstab_efi_dev=$(awk '{print $1}'<<< "$fstab_efi")
            # Variable efi_part_path is set by ALAR
            fstab_efi_uuid=$(blkid -o value -s UUID ${efi_part_path})
            sed "s|$fstab_efi_dev|UUID=$fstab_efi_uuid|" <<< $fstab_efi >> /etc/fstab
        fi
    else
        # In this branch we need to add the /boot/efi to the fstab. If we have a efi partition
        if [[ -n  ${efi_part_path} ]]; then
            efi_part_fs=$(lsblk -l -o FSTYPE ${efi_part_path} | tail -n 1)
            echo "UUID=$(blkid -o value -s UUID ${efi_part_path}) /boot/efi ${efi_part_fs} defaults 0 0" >> /etc/fstab
        fi
    fi

    # The resource-disk we include in this function as well
    awk '/\/dev\/disk\/cloud\/azure_resource-part1/ {print}' ${fstab_org} >>/etc/fstab
}

if [[ ${isLVM} != "true" ]]; then
    fstab_root=$(awk '/[[:space:]]+\/[[:space:]]+/ {print}' ${fstab_org}) 
    if [[ "$fstab_root" =~ ^[[:space:]]*UUID.*  ]]; then
        echo "$fstab_root" >> /etc/fstab
    else
        fstab_root_dev=$(awk '{print $1}'<<< "$fstab_root")
        fstab_root_uuid=$(blkid -o value -s UUID $(awk '{print $1}'<<< "$fstab_root"))
        sed "s|$fstab_root_dev|UUID=$fstab_root_uuid|" <<< $fstab_root >> /etc/fstab
    fi
    boot_efi_mnt
else
    awk '/rootvg-rootlv/ {print}' ${fstab_org} >>/etc/fstab
    boot_efi_mnt    
    awk '/rootvg-homelv/ {print}' ${fstab_org} >>/etc/fstab
    awk '/rootvg-optlv/ {print}' ${fstab_org} >>/etc/fstab
    awk '/rootvg-tmplv/ {print}' ${fstab_org} >>/etc/fstab
    awk '/rootvg-usrlv/ {print}' ${fstab_org} >>/etc/fstab
    awk '/rootvg-varlv/ {print}' ${fstab_org} >>/etc/fstab
fi

echo "Content of fstab after running the script -->"
cat /etc/fstab

exit 0
