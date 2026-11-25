#!/bin/bash

# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.

# Linux script from the nvme-conversion CLI extension, with offline guest
# kernel/device resolution and fix-only behavior for ALAR.

guest_kernel_version() {
    printf '%s\n' "$guest_kernel"
}

scsi_device_parts() {
    if [[ "$1" =~ ^(/dev/sd[a-z]+)([0-9]*)$ ]]; then
        printf '%s %s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    elif [[ "$1" =~ ^(/dev/disk/azure/scsi[0-9]+/lun[0-9]+)(-part([0-9]+))?$ ]]; then
        printf '%s %s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[3]}"
    else
        return 1
    fi
}

guest_partition_device() {
    local disk partition="$2"
    disk=$(readlink -f "$1") || return 1
    if [ -z "$partition" ]; then
        printf '%s\n' "$disk"
    elif [[ "$disk" =~ [0-9]$ ]]; then
        printf '%sp%s\n' "$disk" "$partition"
    else
        printf '%s%s\n' "$disk" "$partition"
    fi
}

resolve_guest_device() {
    local device="$1" disk partition original target extra parts
    parts=$(scsi_device_parts "$device") || return 1
    read -r disk partition <<< "$parts"

    # Explicit identities also cover unmounted data disks and swap. The map is
    # guest-visible, with one original-device rescue-device pair per line.
    if [ -n "${ALAR_NVME_DEVICE_MAP:-}" ]; then
        if [ ! -r "$ALAR_NVME_DEVICE_MAP" ]; then
            echo "[ERROR] Cannot read device map: $ALAR_NVME_DEVICE_MAP" >&2
            return 1
        fi
        while read -r original target extra || [ -n "$original" ]; do
            [[ -z "$original" || "$original" == \#* ]] && continue
            if [[ "$original" != /dev/* || "$target" != /dev/* || -n "$extra" ]]; then
                echo "[ERROR] Invalid device map entry: $original $target $extra" >&2
                return 1
            fi
            if [ "$original" = "$device" ]; then
                printf '%s\n' "$target"
                return
            fi
            if [[ -n "$disk" && "$original" = "$disk" ]]; then
                guest_partition_device "$target" "$partition"
                return
            fi
        done < "$ALAR_NVME_DEVICE_MAP"
    fi

    local spec mountpoint rest mounted_device parent source_disk source_partition
    while read -r spec mountpoint rest || [ -n "$spec" ]; do
        [[ -z "$spec" || "$spec" == \#* || "$mountpoint" = none ]] && continue
        if ! parts=$(scsi_device_parts "$spec"); then
            continue
        fi
        read -r source_disk source_partition <<< "$parts"
        [[ -n "$source_disk" && "$source_disk" = "$disk" ]] || continue
        if ! mounted_device=$(findmnt -rn -o SOURCE --mountpoint "$(printf '%b' "$mountpoint")"); then
            continue
        fi
        mounted_device="${mounted_device%%\[*}"
        if [ "$spec" = "$device" ]; then
            printf '%s\n' "$mounted_device"
            return
        fi
        parent=$(lsblk -dnro PKNAME "$mounted_device") || return 1
        if [[ -n "$parent" && "$parent" != *$'\n'* ]]; then
            guest_partition_device "/dev/$parent" "$partition"
            return
        fi
    done < /etc/fstab
    return 1
}

guest_blkid() {
    local device
    if ! device=$(resolve_guest_device "$1"); then
        echo "[WARNING] Cannot map guest device $1 to an attached disk. Supply ALAR_NVME_DEVICE_MAP for unmounted or ambiguous devices." >&2
        return 1
    fi
    command blkid -c /dev/null "$device"
}

distro=""

# Function to display usage
usage() {
    echo "Usage: $0 [-fix]"
    exit 1
}

# Parse command line arguments
while [ $# -gt 0 ]; do
    case "$1" in
        -fix)
            ;;
        *)
            usage
            ;;
    esac
    shift
done

# Determine the Linux distribution
if [ -f /etc/os-release ]; then
    source /etc/os-release
    distro="$ID"
elif [ -f /etc/debian_version ]; then
    distro="debian"
elif [ -f /etc/SuSE-release ]; then
    distro="suse"
elif [ -f /etc/redhat-release ]; then
    distro="redhat"
elif [ -f /etc/centos-release ]; then
    distro="centos"
elif [ -f /etc/rocky-release ]; then
    distro="rocky"
else
    echo "[ERROR] Unsupported distribution."
    exit 1
fi
echo "[INFO] Operating system detected: $distro"

# uname reports the rescue kernel even inside a chroot. Check the original
# driver logic against every installed guest kernel instead.
guest_kernels=()
for modules_dir in /lib/modules/*; do
    [ -d "$modules_dir" ] && guest_kernels+=("${modules_dir##*/}")
done
if [ "${#guest_kernels[@]}" -eq 0 ]; then
    echo "[ERROR] No installed guest kernels found in /lib/modules."
    exit 1
fi
# Function to check if azure-vm-utils is installed
check_azure_vm_utils() {
    echo "[INFO] Checking for azure-vm-utils..."

    if command -v azure-nvme-id &>/dev/null; then
        echo "[INFO] azure-vm-utils is installed (azure-nvme-id found)."
        if [ -f /etc/udev/rules.d/80-azure-disk.rules ] || [ -f /lib/udev/rules.d/80-azure-disk.rules ] || [ -f /usr/lib/udev/rules.d/80-azure-disk.rules ]; then
            echo "[INFO] 80-azure-disk.rules is present. NVMe disk symlinks and io_timeout will be managed by udev."
        else
            echo "[WARNING] azure-nvme-id found but 80-azure-disk.rules is missing. Udev symlinks may not work after conversion."
        fi
    else
        echo "[WARNING] azure-vm-utils is not installed."
        echo "[WARNING] After conversion, /dev/disk/azure/ symlinks for NVMe disks may not be available."
        echo "[WARNING] Install azure-vm-utils from https://github.com/Azure/azure-vm-utils for best NVMe experience."
        _install_fallback_udev_rules
    fi
}

# Fallback udev rule for NVMe disks when azure-vm-utils is not available.
# Provides: io_timeout=240s, /dev/disk/azure/root, /dev/disk/azure/data/by-lun/N
# Does NOT provide: by-name, by-serial, by-index (those require azure-nvme-id binary)
# Named 99- so that 80-azure-disk.rules takes precedence if azure-vm-utils is later installed.
_FALLBACK_UDEV_RULE='# Fallback Azure NVMe udev rules (installed by az nvme-conversion)
# Remove this file after installing azure-vm-utils (which provides 80-azure-disk.rules)
ACTION!="add|change", GOTO="azure_nvme_fallback_end"
SUBSYSTEM!="block", GOTO="azure_nvme_fallback_end"
KERNEL!="nvme*", GOTO="azure_nvme_fallback_end"
ENV{ID_MODEL}!="MSFT NVMe Accelerator v1.0", GOTO="azure_nvme_fallback_end"

# Set io_timeout to 240 seconds for remote NVMe disks
ENV{DEVTYPE}=="disk", ATTRS{nsid}=="?*", ATTR{queue/io_timeout}="240000"

# OS disk: namespace ID 1
KERNEL=="nvme*[0-9]n1", ENV{DEVTYPE}=="disk", SYMLINK+="disk/azure/root"
KERNEL=="nvme*[0-9]n1p[0-9]*", ENV{DEVTYPE}=="partition", SYMLINK+="disk/azure/root-part%n"

# Data disks: namespace ID 2+ maps to LUN = nsid - 2
KERNEL=="nvme*[0-9]n*[0-9]", ENV{DEVTYPE}=="disk", ATTRS{nsid}!="1", PROGRAM="/bin/sh -ec '\''echo $(($(cat /sys/class/block/%k/nsid) - 2))'\''", SYMLINK+="disk/azure/data/by-lun/%c"
KERNEL=="nvme*[0-9]n*[0-9]p[0-9]*", ENV{DEVTYPE}=="partition", ATTRS{nsid}!="1", PROGRAM="/bin/sh -ec '\''echo $(($(cat /sys/class/block/$(echo %k | sed s/p[0-9]*$//)/nsid) - 2))'\''", SYMLINK+="disk/azure/data/by-lun/%c-part%n"

LABEL="azure_nvme_fallback_end"'

_install_fallback_udev_rules() {
    local _target="/etc/udev/rules.d/99-azure-nvme-fallback.rules"

    if [ -f "$_target" ]; then
        echo "[INFO] Fallback udev rule already installed at $_target"
        return
    fi

    echo "[INFO] Installing fallback NVMe udev rule at $_target"
    echo "$_FALLBACK_UDEV_RULE" > "$_target"
    echo "[INFO] Offline guest: udev rules will load at the next boot."
    echo "[INFO] Fallback udev rule installed. Provides:"
    echo "[INFO]   - io_timeout=240s for NVMe remote disks"
    echo "[INFO]   - /dev/disk/azure/root symlink"
    echo "[INFO]   - /dev/disk/azure/data/by-lun/N symlinks"
    echo "[INFO]   Note: by-name/by-serial/by-index require azure-vm-utils"
}

# Function to check if NVMe driver is in initrd/initramfs or built into the kernel
check_nvme_driver() {
    echo "[INFO] Checking if NVMe driver is available for boot..."

    # Check if nvme is compiled directly into the kernel (built-in)
    if grep -qw nvme "/lib/modules/$(guest_kernel_version)/modules.builtin" 2>/dev/null; then
        echo "[INFO] NVMe driver is built into the kernel. No initramfs entry needed."
        return 0
    fi

    echo "[INFO] NVMe is not built-in. Checking initrd/initramfs..."
    case "$distro" in
        ubuntu|debian)
            _initramfs_ok=true
            if ! lsinitramfs /boot/initrd.img-* 2>/dev/null | grep -q nvme; then
                echo "[WARNING] NVMe driver not found in initrd/initramfs."
                _initramfs_ok=false
            fi
            if ! lsinitramfs /boot/initrd.img-* 2>/dev/null | grep -qE 'hv_pci|pci.hyperv'; then
                echo "[WARNING] pci-hyperv/hv_pci driver not found in initrd/initramfs (required for Azure NVMe)."
                _initramfs_ok=false
            fi
            if $_initramfs_ok; then
                echo "[INFO] NVMe driver found in initrd/initramfs."
            else
                if modinfo -k "$guest_kernel" nvme &>/dev/null; then
                    echo "[INFO] NVMe module exists on disk."
                fi
                echo "[INFO] Adding NVMe/pci-hyperv drivers to initrd/initramfs..."
                update-initramfs -u -k all
                if lsinitramfs /boot/initrd.img-* | grep -q nvme; then
                    echo "[INFO] NVMe driver added successfully."
                else
                    echo "[ERROR] Failed to add NVMe driver to initrd/initramfs."
                fi
            fi
            ;;
        redhat|rhel|centos|rocky|almalinux|azurelinux|mariner|suse|sles|ol)
            # Check ALL installed initramfs images, not just the running kernel
            _nvme_missing=false
            for _img in /boot/initramfs-*.img; do
                [ -f "$_img" ] || continue
                [[ "$_img" == *kdump* ]] && continue
                [[ "$_img" == *rescue* ]] && continue
                if ! lsinitrd "$_img" 2>/dev/null | grep -q nvme; then
                    echo "[WARNING] NVMe driver not found in $_img"
                    _nvme_missing=true
                fi
                if ! lsinitrd "$_img" 2>/dev/null | grep -qE 'hv_pci|pci.hyperv'; then
                    echo "[WARNING] pci-hyperv/hv_pci driver not found in $_img (required for Azure NVMe)"
                    _nvme_missing=true
                fi
            done
            if ! $_nvme_missing; then
                echo "[INFO] NVMe and pci-hyperv drivers found in initrd/initramfs."
            else
                if modinfo -k "$guest_kernel" nvme &>/dev/null; then
                    echo "[INFO] NVMe module exists on disk but is not in all initramfs images."
                fi
                echo "[INFO] Adding NVMe driver to initrd/initramfs (all kernels)..."
                mkdir -p /etc/dracut.conf.d
                echo 'add_drivers+=" nvme nvme-core pci-hyperv "' | tee /etc/dracut.conf.d/nvme.conf > /dev/null
                dracut -f --regenerate-all
                _verify_ok=true
                for _img in /boot/initramfs-*.img; do
                    [ -f "$_img" ] || continue
                    [[ "$_img" == *kdump* ]] && continue
                    [[ "$_img" == *rescue* ]] && continue
                    if ! lsinitrd "$_img" 2>/dev/null | grep -q nvme; then
                        echo "[ERROR] NVMe driver still missing in $_img after rebuild."
                        _verify_ok=false
                    fi
                    if ! lsinitrd "$_img" 2>/dev/null | grep -qE 'hv_pci|pci.hyperv'; then
                        echo "[ERROR] pci-hyperv/hv_pci driver still missing in $_img after rebuild."
                        _verify_ok=false
                    fi
                done
                if $_verify_ok; then
                    echo "[INFO] NVMe driver added successfully."
                else
                    echo "[ERROR] Failed to add NVMe driver to all initramfs images."
                fi
            fi
            ;;
        *)
            echo "[ERROR] Unsupported distribution for NVMe driver check."
            return 1
            ;;
    esac
}

# Function to check nvme_core.io_timeout parameter
nvme_timeout_entries_ready() {
    awk '
        /^[[:space:]]*linux(efi|16)?[[:space:]]+/ {
            seen = 1
            count = 0
            for (i = 1; i <= NF; i++) {
                if ($i ~ /^nvme_core[.]io_timeout=/) {
                    count++
                    if ($i != "nvme_core.io_timeout=240") bad = 1
                }
            }
            if (count != 1) bad = 1
        }
        END { exit !(seen && !bad) }
    ' "$1"
}

set_rhel7_timeout_defaults() {
    if ! grep -q '^GRUB_CMDLINE_LINUX="' "$1"; then
        echo "[ERROR] Expected quoted GRUB_CMDLINE_LINUX in $1."
        return 1
    fi
    if ! sed -i -E '/^GRUB_CMDLINE_LINUX="/ { s/(^|[[:space:]"])nvme_core[.]io_timeout=[^[:space:]"]+/\1/g; s/^(GRUB_CMDLINE_LINUX=")/\1nvme_core.io_timeout=240 /; }' "$1"; then
        echo "[ERROR] Failed to update timeout defaults in $1."
        return 1
    fi
}

check_nvme_timeout() {
    echo "[INFO] Checking nvme_core.io_timeout parameter..."

    local _rhel7_efi_config=""
    case "$distro" in
        redhat|rhel|centos)
            if [ -r /etc/os-release ] &&
               ( . /etc/os-release; [ "${VERSION_ID%%.*}" = 7 ] ) &&
               { [ -e /etc/grub2-efi.cfg ] || [ -L /etc/grub2-efi.cfg ]; }; then
                if ! _rhel7_efi_config=$(readlink -e /etc/grub2-efi.cfg); then
                    echo "[ERROR] Cannot resolve guest EFI configuration /etc/grub2-efi.cfg."
                    return 1
                fi
                case "$_rhel7_efi_config" in
                    /boot/efi/EFI/*/grub.cfg) ;;
                    *) echo "[ERROR] Unsupported guest EFI configuration: $_rhel7_efi_config"; return 1 ;;
                esac
                # Older RHEL EFI uses a full config, not the modern redirect stub.
                if ! grep -Eq '^[[:space:]]*linux(efi|16)?[[:space:]]+' "$_rhel7_efi_config"; then
                    echo "[ERROR] Expected standalone kernel entries in $_rhel7_efi_config; refusing to overwrite."
                    return 1
                fi
            fi
            ;;
    esac

    # Build grub file list dynamically for verification
    _grub_check_files="/etc/default/grub"
    if [ -f /boot/grub2/grub.cfg ]; then
        _grub_check_files="$_grub_check_files /boot/grub2/grub.cfg"
    elif [ -f /boot/grub/grub.cfg ]; then
        _grub_check_files="$_grub_check_files /boot/grub/grub.cfg"
    fi

    if { [ -n "$_rhel7_efi_config" ] && nvme_timeout_entries_ready "$_rhel7_efi_config"; } ||
       { [ -z "$_rhel7_efi_config" ] && grep -q "nvme_core.io_timeout=240" $_grub_check_files 2>/dev/null; }; then
        echo "[INFO] nvme_core.io_timeout is set to 240."
    elif [ -z "$_rhel7_efi_config" ] && command -v grubby &>/dev/null && grubby --info=ALL 2>/dev/null | grep -q "nvme_core.io_timeout=240"; then
        echo "[INFO] nvme_core.io_timeout is set to 240 (BLS entries)."
    else
        echo "[WARNING] nvme_core.io_timeout is not set to 240."
        echo "[INFO] Setting nvme_core.io_timeout to 240..."
        case "$distro" in
            ubuntu)
                sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/GRUB_CMDLINE_LINUX_DEFAULT="nvme_core.io_timeout=240 /g' /etc/default/grub
                GRUB_DISABLE_OS_PROBER=true update-grub
                ;;
            debian)
                sed -i 's/GRUB_CMDLINE_LINUX="/GRUB_CMDLINE_LINUX="nvme_core.io_timeout=240 /g' /etc/default/grub
                GRUB_DISABLE_OS_PROBER=true update-grub
                ;;
            suse|sles|opensuse*)
                if [ -f /etc/default/grub ]; then
                    sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/GRUB_CMDLINE_LINUX_DEFAULT="nvme_core.io_timeout=240 /g' /etc/default/grub
                    GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg
                else
                    echo "[ERROR] /etc/default/grub not found."
                    return 1
                fi
                ;;
            redhat|rhel|centos|rocky|almalinux)
                if [ -f /etc/default/grub ]; then
                    if [ -n "$_rhel7_efi_config" ]; then
                        set_rhel7_timeout_defaults /etc/default/grub || return 1
                        if ! GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o "$_rhel7_efi_config"; then
                            echo "[ERROR] Failed to regenerate $_rhel7_efi_config."
                            return 1
                        fi
                    else
                        sed -i 's/GRUB_CMDLINE_LINUX="/GRUB_CMDLINE_LINUX="nvme_core.io_timeout=240 /g' /etc/default/grub
                        GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg
                    fi
                    # Update BLS entries if applicable (RHEL 8+, AlmaLinux 8+)
                    if grep -q "GRUB_ENABLE_BLSCFG=true" /etc/default/grub 2>/dev/null; then
                        if command -v grubby &>/dev/null; then
                            grubby --update-kernel=ALL --args="nvme_core.io_timeout=240"
                            echo "[INFO] Updated BLS entries via grubby."
                        fi
                    fi
                else
                    echo "[ERROR] /etc/default/grub not found."
                    return 1
                fi
                ;;
            ol|azurelinux|mariner)
                if [ -f /etc/default/grub ]; then
                    sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/GRUB_CMDLINE_LINUX_DEFAULT="nvme_core.io_timeout=240 /g' /etc/default/grub
                    GRUB_DISABLE_OS_PROBER=true grub2-mkconfig -o /boot/grub2/grub.cfg
                    # Update BLS entries if applicable (OL 8.10+)
                    if grep -q "GRUB_ENABLE_BLSCFG=true" /etc/default/grub 2>/dev/null; then
                        if command -v grubby &>/dev/null; then
                            grubby --update-kernel=ALL --args="nvme_core.io_timeout=240"
                            echo "[INFO] Updated BLS entries via grubby."
                        fi
                    fi
                else
                    echo "[ERROR] /etc/default/grub not found."
                    return 1
                fi
                ;;
            *)
                echo "[ERROR] Unsupported distribution for nvme_core.io_timeout fix."
                return 1
                ;;
        esac

        if { [ -n "$_rhel7_efi_config" ] && nvme_timeout_entries_ready "$_rhel7_efi_config"; } ||
           { [ -z "$_rhel7_efi_config" ] && grep -q "nvme_core.io_timeout=240" $_grub_check_files 2>/dev/null; }; then
            echo "[INFO] nvme_core.io_timeout set successfully."
        elif [ -z "$_rhel7_efi_config" ] && command -v grubby &>/dev/null && grubby --info=ALL 2>/dev/null | grep -q "nvme_core.io_timeout=240"; then
            echo "[INFO] nvme_core.io_timeout set successfully (BLS entries)."
        else
            echo "[ERROR] Failed to set nvme_core.io_timeout."
            if [ -n "$_rhel7_efi_config" ]; then return 1; fi
        fi
    fi
}

# Function to check /etc/fstab for deprecated device names
check_fstab() {
    echo "[INFO] Checking /etc/fstab for deprecated device names..."
    # NOTE: /dev/mapper/* (LVM) and PARTUUID= paths survive NVMe conversion
    # because they use UUID-based addressing underneath. Only /dev/sd* and
    # /dev/disk/azure/scsi* paths break when disks move from SCSI to NVMe.
    if grep -Eq '/dev/sd[a-z][0-9]*|/dev/disk/azure/scsi[0-9]*/lun[0-9]*' /etc/fstab; then
        echo "[WARNING] /etc/fstab contains deprecated device names."
        echo "[INFO] Replacing deprecated device names in /etc/fstab with UUIDs..."

        # Create a backup of the fstab file
        cp /etc/fstab /etc/fstab.bak

        # Ensure fstab.new starts fresh (avoid stale leftovers from interrupted runs)
        rm -f /etc/fstab.new

        # Use sed to replace device names with UUIDs
        while read -r line; do
            if [[ "$line" =~ ^[^#] ]]; then
                device=$(echo "$line" | awk '{print $1}')
                if [[ "$device" =~ ^/dev/sd[a-z][0-9]*$ ]]; then
                    uuid=$(guest_blkid "$device" | awk -F\" '/UUID=/ {print $2}')
                    if [ -n "$uuid" ]; then
                        newline=$(echo "$line" | sed "s|$device|UUID=$uuid|g")
                        echo "[INFO] Replaced $device with UUID=$uuid"
                        echo "$newline" >> /etc/fstab.new
                    else
                        echo "[WARNING] Could not find UUID for $device.  Skipping."
                        echo "$line" >> /etc/fstab.new
                    fi
                elif [[ "$device" =~ ^/dev/disk/azure/scsi[0-9]*/lun[0-9]* ]]; then
                    uuid=$(guest_blkid "$device" | awk -F\" '/UUID=/ {print $2}')
                    if [ -n "$uuid" ]; then
                        newline=$(echo "$line" | sed "s|$device|UUID=$uuid|g")
                        echo "[INFO] Replaced $device with UUID=$uuid"
                        echo "$newline" >> /etc/fstab.new
                    else
                        echo "[WARNING] Could not find UUID for $device.  Skipping."
                        echo "$line" >> /etc/fstab.new
                    fi
                else
                    echo "$line" >> /etc/fstab.new
                fi
            else
                echo "$line" >> /etc/fstab.new
            fi
        done < /etc/fstab

        # Replace the old fstab with the new fstab
        mv /etc/fstab.new /etc/fstab

        echo "[INFO] /etc/fstab updated with UUIDs.  Original fstab backed up to /etc/fstab.bak"
    else
        echo "[INFO] /etc/fstab does not contain deprecated device names."
    fi
}

# Run the checks
check_azure_vm_utils
for guest_kernel in "${guest_kernels[@]}"; do
    check_nvme_driver
done
check_nvme_timeout
check_fstab

exit 0
