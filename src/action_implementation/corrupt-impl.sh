#!/usr/bin/bash
# -----------------------------------------------------------------------------
# Version: 1.0.0
# Initial release: 2026-03-01
# Latest update: 2026-03-01
# Author: Azure Support
#
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.
# -----------------------------------------------------------------------------
# Purpose: ALAR implementation for fixing basic filesystem corruption issues
#
# This script is a vehicle for running the ALAR assembly process, which will by
#   nature do filesystem checks and repairs.  This script will not do much in
#   the initial implementation but report some filesystem basic information
# - display all block device and filesystem info
# - if LVs are present, display LV info
# -----------------------------------------------------------------------------
#
# Load helper library
IMPL_DIR=`dirname $0`
. $IMPL_DIR/helpers.sh

echo "Displaying all block device and filesystem info"
lsblk -f

echo "Displaying LV info if LVs are present"
lvs -o lv_name,vg_name,segtype,devices