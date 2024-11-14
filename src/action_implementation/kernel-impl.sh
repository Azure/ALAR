#!/bin/bash
# The main intention is to roll back to the previous working kernel
# We do this by altering the grub configuration
# This is different for RedHat based distros and Ubuntu/SUSE distros
# Ubuntu and SLES use sub-menues
# Variables are set by action.rs

if [[ ${isRedHat} == "true" ]]; then
	# verify whether GRUB_DEFAULT is available
	grep -q 'GRUB_DEFAULT=.*' /etc/default/grub || echo 'GRUB_DEFAULT=saved' >>/etc/default/grub

	# set to previous kernel
	sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT=1/' /etc/default/grub

	# Generate both config files.
	# TODO - check if we need to generate both. Newer distro version don't require this anymore. Let us create a backup therefore.
	cp /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg.bak
	grub2-mkconfig -o /boot/efi/EFI/$(ls /boot/efi/EFI | grep -i -E "centos|redhat")/grub.cfg
	grub2-mkconfig -o /boot/grub2/grub.cfg

	# enable sysreq
	echo "kernel.sysrq = 1" >>/etc/sysctl.conf
fi

if [[ ${isUbuntu} == "true" ]]; then
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

	# set to previous kernel
	sed -i -e 's/GRUB_DEFAULT=.*/GRUB_DEFAULT=2/' /etc/default/grub
	grub2-mkconfig -o /boot/grub2/grub.cfg
fi

# For reference --> https://www.linuxsecrets.com/2815-grub2-submenu-change-boot-order

exit 0
