#!/bin/bash
# A helper functions to prevent an accidently exit of the chroot environment
# This script is used in together with the chroot-cli action
function exit-chroot() {
    unset exit
    exit
}
export -f exit-chroot

function exit() {
    echo "If you want to exit please use 'exit-chroot' instead"
}
export -f exit
