#!/bin/bash

cd /tmp
# Get version of ALAR and fetch it
VERSION=$(curl -s -L https://raw.githubusercontent.com/Azure/ALAR/main/Cargo.toml | grep  -i VERSION | cut -f3 -d' ' | cut -c2-6)

# What architecture we are running on?
ARCH=$(uname -m)

if [[ ${ARCH} == "aarch64" ]]; then
        curl -s -o alar2 -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar2-aarch64
        chmod 700 alar2

        # Start the recovery
        ./alar2 $1
        exit $?
else
        curl -s -o alar2 -L https://github.com/Azure/ALAR/releases/download/v$VERSION/alar2
        chmod 700 alar2

        # Start the recovery
        ./alar2 $1
        exit $?
fi