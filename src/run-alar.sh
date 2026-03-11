#!/bin/bash
# -----------------------------------------------------------------------------
# Author: Azure Support
#
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.
# -----------------------------------------------------------------------------

# Save the arguments
args=("$@")

cd /tmp
# Fetch the actual latest release version from github, we will use this to download the latest release of ALAR
VERSION=$(curl -fsSL https://api.github.com/repos/Azure/ALAR/releases/latest | grep tag_name | cut -d':' -f2 | sed s/[v\",\ ]//g)

# What architecture we are running on?
ARCH=$(uname -m)

if [[ ${ARCH} == "aarch64" ]]; then
        curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar-aarch64
else
        curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar
fi

# Get sure the binary is executable and has the correct permissions
chmod 700 alar

# Start the recovery
if [[ ${args[1]} == "SELFHELP" ]]; then
        unset 'args[1]'
        RUST_LOG=info ./alar "${args[@]}" --selfhelp-initiator
else
        RUST_LOG=info ./alar "${args[@]}"
        exit $?
fi