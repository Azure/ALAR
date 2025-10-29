#!/usr/bin/bash
# -----------------------------------------------------------------------------
# Version: 1.1.0
# Released: 2025-10-31
# Author: Azure Support
#
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.
# -----------------------------------------------------------------------------
# Purpose: ALAR helper script library
#
# these functions are not intended to function independently, the file is a
# library to be included in other ALAR implementations
# -----------------------------------------------------------------------------
# define this once for the script run - so that all backups have the same exact
# timestamp
TIMESTAMP=`date +%Y%m%dT%H%M%S`

function backup() {
  # Create a backup of a file.
  # Args:
  #   $1 = file name to back up
  #   $2 = optional target directory
  #
  # Behavior:
  #   - If $2 provided: move file to $2 with timestamp appended.
  #   - Otherwise: copy file to current directory with timestamp appended.

  local ORIGFILE="$1"
  local TARGETDIR="$2"

  # Validate args
  if [[ -z "$ORIGFILE" ]]; then
    echo "Usage: backup <file> [target_dir]"
    return 1
  fi

  if [[ ! -e "$ORIGFILE" ]]; then
    echo "ERR: File does not exist: $ORIGFILE"
    return 1
  fi

  if [[ -n "$TARGETDIR" ]]; then
    # Create target directory if it doesn't exist
    if [[ ! -d "$TARGETDIR" ]]; then
      echo "INFO: Creating backup directory: $TARGETDIR"
      mkdir -p "$TARGETDIR" || {
        echo "ERR: Failed to create backup directory: $TARGETDIR"
        return 1
      }
    fi

    local BASENAME
    BASENAME=$(basename "$ORIGFILE")
    local DEST="$TARGETDIR/${BASENAME}.${TIMESTAMP}"

    echo "INFO: Moving $ORIGFILE to $DEST"
    mv -v "$ORIGFILE" "$DEST"
  else
    # Copy into PWD
    local BACKUP="${ORIGFILE}.alar.${TIMESTAMP}"
    echo "INFO: backing up $ORIGFILE to $BACKUP"
    cp -v -p "$ORIGFILE" "$BACKUP"
  fi
}

function checkPerm() {
  # Validate permissions, including special bits.
  # Args: $1 = file to validate perms
  #       $2 = octal representation of desired permissions (may include special
  #            bits)

  file="$1"
  desired="$2"

  if [[ -z "$file" || -z "$desired" ]]; then
    echo "Usage: checkPerm <file> <desired-octal-perms>"
    return 1
  fi

  if [[ ! -e "$file" ]]; then
    echo "Error: File does not exist: $file"
    return 1
  fi

  # Get current permissions in full 4-digit octal form (includes special bits)
  actual=$(stat -c "%a" "$file")
  # Pad to 4 digits for consistent comparison (e.g., 755 -> 0755)
  # there is an edge case not being handled in 'actual' output of stat - if the
  # 3-digit perms of the actual file start with 0 from 'stat' it will break the
  # check because it looks like 0x to bash.  This is sufficiently edge to not 
  # handle it, and also 0XX perms would be basically broken for most real-world
  # uses, so lets keep the implied force change in that scenario
  actual=$(printf "%04d" "$actual")
  desired=$(printf "%04s" "$desired")

  if [[ "$actual" == "$desired" ]]; then
    echo "OK: $file already has permissions $actual"
  else
    echo "WARN: $file has permissions $actual, fixing to $desired"
    if chmod "$desired" "$file"; then
      newperm=$(stat -c "%a" "$file")
      printf -v newperm "%04d" "$newperm"
      echo "FIXED: $file now has permissions $newperm"
    else
      echo "ERR: Unable to fix permissions on $file"
      return 1
    fi
  fi
}

function checkOwner() {
  # Validate file ownership
  # Args:
  #   $1 = file to validate
  #   $2 = desired owner or owner:group
  #   $3 = optional group (ignored if $2 includes :)
  #
  # Examples:
  #   checkOwner /etc/sudoers root root
  #   checkOwner /etc/sudoers root:root

  local file="$1"
  local owner_group="$2"
  local desired_owner desired_group

  # Validate args
  if [[ -z "$file" || -z "$owner_group" ]]; then
    echo "Usage: checkOwner <file> <owner[:group]> [group]"
    return 1
  fi

  # Parse user/group input
  if [[ "$owner_group" == *:* ]]; then
    desired_owner="${owner_group%%:*}"
    desired_group="${owner_group##*:}"
  else
    desired_owner="$owner_group"
    desired_group="$3"
  fi

  # Sanity check
  if [[ -z "$desired_owner" || -z "$desired_group" ]]; then
    echo "Usage: checkOwner <file> <owner[:group]> [group]"
    return 1
  fi

  # Verify file exists
  if [[ ! -e "$file" ]]; then
    echo "WARN: File not found: $file"
    return 1
  fi

  # Get current owner and group
  local actual_owner actual_group
  actual_owner=$(stat -c "%U" "$file")
  actual_group=$(stat -c "%G" "$file")

  # Compare and fix if needed
  if [[ "$actual_owner" == "$desired_owner" && "$actual_group" == "$desired_group" ]]; then
    echo "OK: $file owner:group OK ($actual_owner:$actual_group)"
  else
    echo "WARN: $file has $actual_owner:$actual_group, fixing to $desired_owner:$desired_group"
    if chown "$desired_owner:$desired_group" "$file"; then
      echo "FIXED: $file now owned by $(stat -c "%U:%G" "$file")"
    else
      echo "ERR: Failed to change ownership of $file"
      return 1
    fi
  fi
}