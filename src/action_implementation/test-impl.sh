#!/bin/bash

# This is just a simple demo in order to print out the environment seen by the script
# The calling process is preparing the environment accordingly
printenv
lsblk -f
echo -n "chroot for distro: "; cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2

