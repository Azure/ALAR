#!/bin/bash
# recover logic for handling and initrd or kernel problem
#

recover_suse() {
    # Kernel will be one of the following styles of files - vmlinux-5.14.21-150400.14.31-azure vmlinuz-4.12.14-6.43-azure vmlinuz-4.12.14-95.68-default
    # initrd will similarly be - initrd-5.14.21-150400.14.31-azure, initrd-4.12.14-6.43-azure, initrd-4.12.14-95.68-default
    # there should be a link from 'vmlinuz' and 'initrd' to one of those kernels and corresponding initrd.
    # -- try to use the links above to find the kernel version, failing that logic, look for the last package installed
    KERNFILE=$(basename $(realpath -e /boot/vmlinuz))
    RET=$?

    # Set these vars up first, for var scope. if $KERNFILE is garbage we'll fix them in the if|fi block below
    KERNVER=$(echo "${KERNFILE%-*}" | sed 's/vmlinuz-//')
    KERNBASE=$(echo "${KERNFILE##*-}")

    if [[ $RET != 0 ]]; then
        # We probably didn't have a link for some reason, so fall back to using the package name
        KERNNAME=$(rpm -qa name="kernel-*" --last | head -n 1 | cut -f 1 -d " " | sed 's/kernel-//' | sed 's/.1.x86_64//')
        KERNVER=$(echo $KERNNAME | cut -d '-' -f 2-)
        KERNBASE=$(echo $KERNNAME | cut -d '-' -f 1)
        KERNFILE=vmlinuz-$KERNVER-$KERNBASE
    fi
    
    INITRD=$(echo $KERNFILE | sed 's/vmlinuz/initrd/')
    # Get sure that all required modules are loaded
    dracut -f -v  --add-drivers "hv_vmbus hv_netvsc hv_storvsc" /boot/$INITRD ${KERNVER}-$KERNBASE
    # recreate the initrd link  (do this in pwd instead of a absolute path link)
    ln -s /boot/$INITRD /boot/initrd
    grub2-mkconfig -o /boot/grub2/grub.cfg
}

recover_ubuntu() {
    if [[ ! -e /var/log/dpkg.log ]]; then
    # if this file is empty we have to assume that we have a vanilla system. Only one kernel available
        kernel_version=$(ls /boot/vmlinuz-*)
        kernel_version=${kernel_version#/boot/vmlinuz-}
    else
        kernel_version=$( zgrep linux-image /var/log/dpkg.log* | grep installed  | cut -d' ' -f5 | cut -d':' -f1 | sed -e 's/linux-image-//' | grep ^[1-9] | sort -V | tail -n 1)
    fi
    # This is needed on Debian only
    if [[ -e /boot/initrd.img-${kernel_version} ]]; then
            rm /boot/initrd.img-${kernel_version}
    fi
    # Get sure that all required modles are loaded
        echo "hv_vmbus" >> /etc/initramfs-tools/modules
        echo "hv_storvsc" >> /etc/initramfs-tools/modules
        echo "hv_netvsc" >> /etc/initramfs-tools/modules

    update-initramfs -k "$kernel_version" -c
    grub-mkconfig -o /boot/grub/grub.cfg
    grub-mkconfig -o /boot/efi/EFI/ubuntu/grub.cfg

}

#
# Should handle all redhat based distros
#
recover_redhat() {
    kernel_version=$(sed -e "s/kernel-//" <<< $(rpm -q kernel --last  | head -n 1 | cut -f1 -d' '))
    if [[ "$isRedHat6" == "true" ]]; then
        # verify the grub.conf and correct it if needed
        cd "$tmp_dir"
        awk -f alar-fki/grub.awk /boot/grub/grub.conf
        # rebuild the initrd
        dracut -f /boot/initramfs-"${kernel_version}".img "$kernel_version"
    else
        depmod ${kernel_version}
        # Get sure that all required modules are loaded
        dracut -f -v  --add-drivers "hv_vmbus hv_netvsc hv_storvsc" /boot/initramfs-${kernel_version}.img ${kernel_version}
        # Recreate the the grub.cfg, it could be the initrd line is missing
        grub2-mkconfig -o /boot/grub2/grub.cfg
        #grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg
    fi

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
