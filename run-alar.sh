#!/bin/bash

cd /tmp
# Get version of ALAR and fetch it
VERSION=$(curl -s -L https://raw.githubusercontent.com/Azure/ALAR/refs/heads/ALAR-redesign/Cargo.toml | grep  -i VERSION | cut -f3 -d' ' | cut -c2-6)
#VERSION=$(curl -s -L https://raw.githubusercontent.com/Azure/ALAR/main/Cargo.toml | grep  -i VERSION | cut -f3 -d' ' | cut -c2-6)

# What architecture we are running on?
ARCH=$(uname -m)

if [[ ${ARCH} == "aarch64" ]]; then
        #curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar-aarch64
        curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/test-0.9.0/alar-aarch64
        chmod 700 alar

        # Start the recovery
        RUST_LOG=info ./alar $@
        exit $?
else
        #curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar
        curl -s -o alar -L https://github.com/Azure/ALAR/releases/download/test-0.9.0/alar
        chmod 700 alar

        # Start the recovery
        RUST_LOG=info ./alar $@
        exit $?
fi
