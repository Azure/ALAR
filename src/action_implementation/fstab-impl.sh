#!/bin/bash

mv -f /etc/fstab{,.copy}

# For Debian we need to instal gawk first. It comes only with mawk
if [[ -f /usr/bin/apt ]]; then
    apt-get install -qq -y gawk
fi

awk '/[[:space:]]+\/[[:space:]]+/ {print}' /etc/fstab.copy >>/etc/fstab
awk '/[[:space:]]+\/boot[[:space:]]+/ {print}' /etc/fstab.copy >>/etc/fstab
# For Suse
awk '/[[:space:]]+\/boot\/efi[[:space:]]+/ {print}' /etc/fstab.copy >>/etc/fstab
# In case we have a LVM system
awk '/rootvg-homelv/ {print}' /etc/fstab.copy >>/etc/fstab
awk '/rootvg-optlv/ {print}' /etc/fstab.copy >>/etc/fstab
awk '/rootvg-tmplv/ {print}' /etc/fstab.copy >>/etc/fstab
awk '/rootvg-usrlv/ {print}' /etc/fstab.copy >>/etc/fstab
awk '/rootvg-varlv/ {print}' /etc/fstab.copy >>/etc/fstab

echo "Content of fstab after running the script -->"
cat /etc/fstab

exit 0