#!/bin/bash

# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.

# The main intention is to roll back to the previous working kernel
# We do this by altering the grub configuration
# This is different for RedHat based distros and Ubuntu/SUSE distros
# Ubuntu and SLES use sub-menues
# Variables are set by action.rs

if [[ ${isRedHat} == "true" ]]; then
        
    DISTRO_VERSION=$(source /etc/os-release; echo "${VERSION%.*}")
	# Get NOT the latest kernel
	KERNEL_VERSION=$(sed -e "s/kernel-//" <<<$(rpm -q kernel | head -n 1 | cut -f1 -d' '))

	if [[ "${DISTRO_VERSION}" == "7" ]]; then
    	# verify whether GRUB_DEFAULT is available
    	grep -q 'GRUB_DEFAULT=.*' /etc/default/grub || echo 'GRUB_DEFAULT=saved' >>/etc/default/grub
    
    	# set to previous kernel
    	sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT=1/' /etc/default/grub
	
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
		
		grubby --set-default /boot/vmlinuz-"${KERNEL_VERSION}"   
    fi
    
    # enable sysreq
    echo "kernel.sysrq = 1" >>/etc/sysctl.conf
fi

if [[ ${isUbuntu} == "true" || ${isDebian}  == "true" ]]; then
    # verify whether GRUB_DEFAULT is available
    grep -q 'GRUB_DEFAULT=.*' /etc/default/grub || echo 'GRUB_DEFAULT=saved' >>/etc/default/grub
    
    # set to previous kernel
    sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT="1>2"/' /etc/default/grub
    update-grub
fi

if [[ ${isSuse} == "true" ]]; then
    # verify whether GRUB_DEFAULT is available
    grep -q 'GRUB_DEFAULT=.*' /etc/default/grub || echo 'GRUB_DEFAULT=saved' >>/etc/default/grub
    
    # set to previous kernel
    sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT="1>2"/' /etc/default/grub
    grub2-mkconfig -o /boot/grub2/grub.cfg
fi

if [[ ${isAzureLinux} == "true" ]]; then
    # verify whether GRUB_DEFAULT is available
    grep -q 'GRUB_DEFAULT=.*' /etc/default/grub || echo 'GRUB_DEFAULT=saved' >>/etc/default/grub
    
    # downgrade kernel version
    dnf downgrade kernel -y
    sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT=2/' /etc/default/grub
    grub2-mkconfig -o /boot/grub2/grub.cfg
fi

# For reference --> https://www.linuxsecrets.com/2815-grub2-submenu-change-boot-order

exit 0
