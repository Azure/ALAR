#!/bin/bash

# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.

#include the common functions and variables
source "${ACTION_DIR}/helpers.sh"

# In case the resolv.conf isn't set correct use the default resolver
resolv-pre() {
    cp /etc/resolv.conf /etc/resolv.conf.org
    echo "nameserver 168.63.129.16" >> /etc/resolv.conf
    echo "nameserver 8.8.8.8" >> /etc/resolv.conf
}

# restore the originail resolv.conf
resolv-after() {
    mv /etc/resolv.conf.org /etc/resolv.conf
}

recover_redhat() {
    resolv-pre

    efi_part_path="$(findmnt -n -o SOURCE /boot/efi)"
    if [[ -z "${efi_part_path}" ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi
    # In case the efi partition got deleted by accident we need to recreate it and reinstall the grub2-efi and shim packages to be able to boot again.
    umount "$efi_part_path"
    mkfs.vfat -F16 "$efi_part_path"
    mount "$efi_part_path" /boot/efi


    if [[ "${ARCHITECTURE}" == "x86_64" ]]; then
        yum reinstall -y grub2-efi-x64 shim-x64
    else
        yum reinstall -y grub2-efi-aa64 shim-aa64
    fi

   yum reinstall grub2-common -y

   DISTRO_VERSION=$(source /etc/os-release; echo "${VERSION%.*}")
    if [[ "${DISTRO_VERSION}" == "7" ]]; then
        GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg
        GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/efi/EFI/"$(ls /boot/efi/EFI | grep -i -E 'centos|redhat')"/grub.cfg
    else
        # Verify the system does use the BLS configuration style.
        if grep -q 'GRUB_ENABLE_BLSCFG=true' /etc/default/grub ; then
            # Regenerate the loader entries for all installed kernels. 
            for k in /lib/modules/*; do
                ver=$(basename "$k")
                kernel-install add "$ver" "/boot/vmlinuz-$ver" "/boot/initramfs-$ver.img"
            done
        fi
    GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg

    # The grub.cfg file in the EFI partition has a hardcoded UUID for the boot partition. We need to replace it with the correct UUID to be able to boot again.
        vendor_dir=$(ls /boot/efi/EFI | grep -i -E "centos|redhat|almalinux")  
        boot_uuid=$(blkid -s UUID -o value $(findmnt /boot -o SOURCE -n))  

        {
            printf 'search --no-floppy --fs-uuid --set=dev %s\n' "${boot_uuid}"
            printf 'set prefix=($dev)/grub2\n'
            printf 'export $prefix\n'
            printf 'configfile $prefix/grub.cfg\n'
        } > "/boot/efi/EFI/${vendor_dir}/grub.cfg"
    fi 

    # Also replace the UUID in the fstab file to make sure that the system can find the EFI partition to mount it at boot time.
    uuid_to_be_replaced="$(awk '/efi/ {print($1)}' /etc/fstab)"
    uuid_to_be_replaced="${uuid_to_be_replaced//UUID=}"
    uuid_to_be_replaced="${uuid_to_be_replaced//\"}"
    new_efi_uuid="$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))"
    sed -i "s/$uuid_to_be_replaced/$new_efi_uuid/" /etc/fstab 
    
    resolv-after
} # End of recover_redhat

recover_suse() {
    resolv-pre

    efi_part_path="$(findmnt -n -o SOURCE /boot/efi)"
    if [[ -z "${efi_part_path}" ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi

    # In case the efi partition got deleted by accident we need to recreate it and reinstall the grub2-efi and shim packages to be able to boot again.
    umount "$efi_part_path"
    mkfs.vfat -F16 "$efi_part_path"
    mount "$efi_part_path" /boot/efi
    
    if [[ "${ARCHITECTURE}" == "x86_64" ]]; then
            zypper remove -y grub2-x86_64-efi
            zypper install -y grub2-x86_64-efi
            zypper remove -y shim
            zypper install -y shim
    else
            zypper remove -y grub2-branding-SLE
            zypper install -y grub2-branding-SLE
            zypper remove -y  grub2-arm64-efi
            zypper install -y grub2-arm64-efi
            zypper remove -y shim
            zypper install -y shim
    fi

    grub2-install --target=arm64-efi --efi-directory=/boot/efi
    shim-install

   # Generate the grub.cfg file.
    create_suse_grub_cfg 

    uuid_to_be_replaced="$(awk '/efi/ {print($1)}' /etc/fstab)"
    uuid_to_be_replaced="${uuid_to_be_replaced//UUID=}"
    uuid_to_be_replaced="${uuid_to_be_replaced//\"}"
    new_efi_uuid="$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))"
    sed -i "s/$uuid_to_be_replaced/$new_efi_uuid/" /etc/fstab

    resolv-after
} # End of recover_suse


recover_azurelinux() {
    resolv-pre

    efi_part_path="$(findmnt -n -o SOURCE /boot/efi)"
    if [[ -z "${efi_part_path}" ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi

    # install the missing dosfstools package
    # we need it to get the mkfs.vfat command
    dnf install dosfstools glibc-gconv-extra -y

    umount "$efi_part_path"
    mkfs.vfat -F16 "$efi_part_path"
    mount "$efi_part_path" /boot/efi

    # VERSION_ID may be "3"/"4" or "3.0"/"4.0"; use major version for branching.
    distro_major="${DISTROVERSION%%.*}"

    if [[ "${distro_major}" == "3" ]]; then
    # reinstall the grub2-efi and shim packages
    # install the grub2-efi package if it is not installed
    # There is no package name difference between the x86_64 and aarch64 architectures for the grub2-efi and shim package in Azure Linux 3.
    # So we can just install it without checking the architecture.

    dnf install -y grub2-efi 
    dnf reinstall -y grub2-efi 
    dnf reinstall -y grub2-efi-binary 
    dnf install -y shim
    fi

    if [[ "${distro_major}" == "4" ]]; then
        if [[ "${ARCHITECTURE}" == "x86_64" ]]; then
            dnf reinstall -y efi-filesystem 
            dnf reinstall -y grub2-efi-x64
            dnf reinstall -y grub2-efi-x64-modules
            dnf reinstall -y shim-x64 
        elif [[ "${ARCHITECTURE}" == "aarch64" ]]; then
            dnf reinstall -y efi-filesystem 
            dnf reinstall -y grub2-efi-aa64 
            dnf reinstall -y grub2-efi-aa64-modules 
            dnf reinstall -y shim-aa64
        else
            echo "Unsupported architecture for Azure Linux EFI recovery: ${ARCHITECTURE}"
            exit 1
        fi
    fi
    
    cd /boot/efi/EFI/azurelinux

    # The UUID of the boot partition is hardcoded in the grub.cfg file
    # This is a workaround to replace it with the correct UUID
    # The UUID of the boot partition can be found by running the following command:
    # lsblk -f -o UUID $(findmnt /boot -o SOURCE -n) -n
    # The output of this command will be used to replace the hardcoded UUID in the grub.cfg file 

    NO_BOOT_PARTITION="false"
    BOOT_UUID="$(blkid -s UUID -o value "$(findmnt /boot -o SOURCE -n)")"
    # IF there is no boot partition take the UUID from the root partition.
    if [[ -z "${BOOT_UUID}" ]]; then 
        BOOT_UUID="$(blkid -s UUID -o value "$(findmnt / -o SOURCE -n)")"
        NO_BOOT_PARTITION="true"
    fi

    if [[ ${NO_BOOT_PARTITION} == "false" ]]; then
        (
        printf 'search --no-floppy --fs-uuid --set=root %s\n' "${BOOT_UUID}"
        printf 'set prefix=($root)/grub2\n'
        printf 'configfile ($root)/grub2/grub.cfg\n'
        ) > grub.cfg
    else
    {
        (
        printf 'search --no-floppy --fs-uuid --set=root %s\n' "${BOOT_UUID}"
        printf 'set prefix=($root)/boot/grub2\n'
        printf 'configfile ($root)/boot/grub2/grub.cfg\n'
        ) > grub.cfg
    }
    fi

    cd /

    # Replace the UUID in the fstab file to make sure that the system can find the EFI partition to mount it at boot time.
    uuid_to_be_replaced="$(awk '/efi/ {print($1)}' /etc/fstab)"
    new_efi_uuid="$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))"  

    # Deleting and creating the entry is more reliable than replacing it on Azure Linux. 
    sed -i '/efi/d' /etc/fstab
    echo "UUID=$new_efi_uuid /boot/efi vfat defaults 0 0" >> /etc/fstab

    resolv-after
} # End of recover_azurelinux

recover_ubuntu() {
    resolv-pre
    
    efi_part_path="$(findmnt -n -o SOURCE /boot/efi)"
    if [[ -z "${efi_part_path}" ]]; then 
        echo "No EFI partition found"
        echo "Aborting! Are you running it on a GEN1 image?"
        exit 1
    fi


    umount "$efi_part_path"
    mkfs.vfat -F16 "$efi_part_path"
    mount "$efi_part_path" /boot/efi

    if [[ "${ARCHITECTURE}" == "x86_64" ]]; then
        apt update
        apt install --reinstall grub-efi-amd64-bin grub-efi-amd64-signed shim-signed efibootmgr -y
        grub-install --efi-directory=/boot/efi --target=x86_64-efi 
    else
        apt update
        apt install --reinstall grub-efi-arm64-bin grub-efi-arm64-signed shim-signed efibootmgr -y
        grub-install --efi-directory=/boot/efi --target=arm64-efi 
    fi

    update-grub
  
    uuid_to_be_replaced="$(awk '/efi/ {print($1)}' /etc/fstab)"
    uuid_to_be_replaced="${uuid_to_be_replaced//UUID=}"
    uuid_to_be_replaced="${uuid_to_be_replaced//\"}"
    new_efi_uuid="$(blkid -s UUID -o value $(findmnt /boot/efi -o SOURCE -n))"
    # Depending on the distro image the fstab file can contain either the UUID or the LABEL of the EFI partition. We need to replace it with the correct one to make sure that the system can find the EFI partition to mount it at boot time.
    if grep -q '^LABEL.*' <<< "${uuid_to_be_replaced}" ; then  
        fatlabel $efi_part_path UEFI
    else
        sed -i "s/UUID=$uuid_to_be_replaced/UUID=$new_efi_uuid/" /etc/fstab
    fi

    resolv-after
} # End of recover_ubuntu

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
