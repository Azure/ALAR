#!/bin/bash


cd /tmp
mkdir alar2

// Get the ghrel tool in order to download the latest ALAR bin
wget -O ghrel.tgz https://github.com/jreisinger/ghrel/releases/download/v0.5.2/ghrel_0.5.2_linux_amd64.tar.gz
tar xzf ghrel.tgz ghrel
chmod 700 ghrel

// Get alar2 binary
./ghrel Azure/ALAR
chmod 700 alar2

mkdir action_implementation
pushd action_implementation
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/fstab-impl.sh
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/grub-awk.sh
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/initrd-impl.sh
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/kernel-impl.sh
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/serialconsole-impl.sh
wget https://raw.githubusercontent.com/Azure/ALAR/main/src/action_implementation/test-impl.sh
popd

alar2 $1
exit $?