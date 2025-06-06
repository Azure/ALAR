#!/bin/bash

#
# serialconsole-impl is responsible to set the configuration for the serialconsole
# correct in case this is missing in a VM image.
# It also enables sysreq to allow a reboot from the Portal
#

enable_sysreq() {
    if [[ $isRedHat == "true"  ]]; then
        echo "kernel.sysrq = 1" >> /etc/sysctl.d/90-alar2.conf
    else
        echo "kernel.sysrq = 1" >> /etc/sysctl.conf
    fi
}

alter_serial_properties() {
    # The aim of this funtion is to provide access to the grub_serial console
    # as well allow the OS to communicate with the serialconsole
    # Just simple append operations are used in this case. Which is enough to gt access to a system
    # if further adjusting is required the administrator needs to perform these steps later on after he/she got access to the system
    
    echo "# Inserted by Azure Linux Autorecovery Tool" >> $grub_file
    echo "# -----------------------------------------" >> $grub_file
    echo "GRUB_TIMEOUT=10" >> $grub_file
    echo 'GRUB_CMDLINE_LINUX="USE_BY_UUID_DEVICE_NAMES=1 rootdelay=300 multipath=off net.ifnames=0 console=ttyS0,115200n8 earlyprintk=ttyS0,115200 console=tty1 earlyprintk=ttyS1"' >> $grub_file
    echo 'GRUB_SERIAL_COMMAND="serial --speed=9600 --unit=0 --word=8 --parity=no --stop=1"' >> $grub_file
    echo 'GRUB_TIMEOUT_STYLE=""' >> $grub_file
}

serial_fix_suse_redhat () {
    if [[ ${DISTROVERSION} =~ 6 ]]; then 
        echo "Configuring the serialconsole for RedHat 6.x is not implemented"
        exit 1
    fi
    
    grub_file="/etc/default/grub"
    enable_sysreq
    
    if [[ -f $grub_file ]]; then
        alter_serial_properties
    else
        # file does not exist
        touch $grub_file
    cat << EOF > $grub_file
GRUB_TIMEOUT=30
GRUB_DISTRIBUTOR="$(sed 's, release .*$,,g' /etc/system-release)"
GRUB_DEFAULT=saved
GRUB_DISABLE_SUBMENU=true
GRUB_TERMINAL="serial"
GRUB_SERIAL_COMMAND="serial --speed=19600 --unit=0 --word=8 --parity=no --stop=1"
GRUB_CMDLINE_LINUX="USE_BY_UUID_DEVICE_NAMES=1 rootdelay=300 multipath=off net.ifnames=0 console=ttyS0,115200n8 earlyprintk=ttyS0,115200 console=tty1 earlyprintk=ttyS1"
GRUB_DISABLE_RECOVERY="true"
EOF
    fi
    
    # update grub
    if [[ $isRedHat == "true" ]]; then
        distro=$(grep '^ID=' /etc/os-release | cut -d '=' -f2 | cut -d'"' -f2)
        if [[ ${distro} == "rhel" ]]; then
            distro="redhat" ;
        fi
        grub2-mkconfig -o /boot/efi/EFI/${distro}/grub.cfg
    fi
    
    grub2-mkconfig -o /boot/grub2/grub.cfg
}

# REDHAT/CENTOS PART
if [[ "$isRedHat" == "true" ]]; then
    serial_fix_suse_redhat
fi

# SUSE PART
if [[ "$isSuse" == "true" ]]; then
    serial_fix_suse_redhat
fi

# AzureLinux PART
if [[ "$isAzureLinux" == "true" ]]; then
    serial_fix_suse_redhat
fi


# UBUNTU PART
if [[ "$isUbuntu" == "true" ]]; then
    grub_file="/etc/default/grub.d/50-cloudimg-settings.cfg"
    enable_sysreq
    
    if [[ -f $grub_file ]]; then
        alter_serial_properties
        update-grub
    else
        # file does not exist
        touch $grub_file
    cat << EOF > $grub_file
# Set the default commandline
GRUB_CMDLINE_LINUX="USE_BY_UUID_DEVICE_NAMES=1 rootdelay=300 multipath=off net.ifnames=0 console=ttyS0,115200n8 earlyprintk=ttyS0,115200 console=tty1 earlyprintk=ttyS1"
GRUB_CMDLINE_LINUX_DEFAULT=""

# Set the grub console type
GRUB_TERMINAL=serial

# Set the serial command
GRUB_SERIAL_COMMAND="serial --speed=9600 --unit=0 --word=8 --parity=no --stop=1"

# Set the recordfail timeout
GRUB_RECORDFAIL_TIMEOUT=30

# Wait briefly on grub prompt
GRUB_TIMEOUT=10
EOF
        # update grub
        update-grub
    fi
fi