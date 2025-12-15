#!/usr/bin/bash
# -----------------------------------------------------------------------------
# Version: 1.2.0
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

# this will be infinitely useful
source /etc/os-release

# Do OS detection for use in other functions
function detect_osfam() {
  # Normalize to lowercase
  local id like_str
  id="$(printf '%s' "${ID:-}" | tr '[:upper:]' '[:lower:]')"
  like_str="$(printf '%s' "${ID_LIKE:-}" | tr '[:upper:]' '[:lower:]')"

  # Default
  OSFAM="unknown"

  # Helper: prefer 'fedora' over other RHEL derivatives if both appear
  _prefer_fedora_like() {
    # Takes a space-separated list; echoes the chosen token if matched.
    local tokens="$1"
    # First, explicit fedora
    for t in $tokens; do
      [[ "$t" == "fedora" ]] && { echo "fedora"; return; }
    done
    # Then other RHEL-family tokens
    for t in $tokens; do
      case "$t" in
        rhel|centos|rocky|almalinux|ol|amzn)
          echo "fedora"; return
          ;;
      esac
    done
    # No match
    echo ""
  }

  # Pass 1: exact ID
  case "$id" in
    # Treat all these as fedora lineage
    rhel|fedora|centos|rocky|almalinux|ol|amzn)
      OSFAM="fedora"
      ;;
    ubuntu|debian)
      OSFAM="debian"
      ;;
    sles|suse|opensuse*)
      OSFAM="suse"
      ;;
    *)
      ;;
  esac

  # Pass 2: scan ID_LIKE with fedora preference (handles multi-token)
  if [[ "$OSFAM" == "unknown" && -n "$like_str" ]]; then
    local chosen
    chosen="$(_prefer_fedora_like "$like_str")"
    if [[ -n "$chosen" ]]; then
      OSFAM="$chosen"
    else
      # Fall back to other families
      for like in $like_str; do
        case "$like" in
          debian|ubuntu) OSFAM="debian"; break ;;
          suse|sles|opensuse*) OSFAM="suse"; break ;;
        esac
      done
    fi
  fi

  export $OSFAM
}
# call the function above to set the OSFAM variable for use elsewhere
detect_osfam()

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

## TODO: Test these 4 functions standalone and in the sudo script

# -------------------------------------------
# CHECK: validate current perms vs desired
# Return codes:
#   0 -> match
#   1 -> mismatch (fixable)
#   2 -> check error (do not fix)
# -------------------------------------------
function checkPerm() {
  # Args: $1 = file path, $2 = desired octal (3 or 4 digits; may include special bits)
  local file="$1"
  local desired="$2"
  local actual newperm

  if [[ -z "$file" || -z "$desired" ]]; then
    echo "Usage: checkPerm <file> <desired-octal-perms>"
    return 2
  fi

  if [[ ! -e "$file" ]]; then
    echo "Error: File does not exist: $file"
    return 2
  fi

  # Basic octal validation (3–4 digits)
  if [[ ! "$desired" =~ ^[0-7]{3,4}$ ]]; then
    echo "Error: Desired permissions must be 3 or 4 octal digits (e.g., 644, 0755, 4755)."
    return 2
  fi

  # Get current permissions in full 4-digit octal form (includes special bits)
  actual=$(stat -c "%a" "$file")

  # Pad to 4 digits for consistent comparison (e.g., 755 -> 0755)
  # There is an edge case not being handled in 'actual' output of stat - if the
  # 3-digit perms of the actual file start with 0 from 'stat' it will break the
  # check because it looks like 0x to bash. This is sufficiently edge to not
  # handle it, and also 0XX perms would be basically broken for most real-world
  # uses, so let's keep the implied force change in that scenario.
  actual=$(printf "%04d" "$actual")
  desired=$(printf "%04s" "$desired")

  if [[ "$actual" == "$desired" ]]; then
    echo "OK: $file already has permissions $actual"
    return 0
  else
    echo "MISMATCH: $file has permissions $actual; desired $desired"
    return 1
  fi
}

# -------------------------------------------
# FIX: apply chmod if check reports mismatch
# Behavior:
#   - Calls checkPerm and relays its status.
#   - If checkPerm returns 1 (mismatch), attempts chmod.
#   - On success, echoes FIXED and returns 0.
#   - On failure, echoes ERR and returns 1.
#   - If checkPerm returns 2 (error), propagates 2 (no fix attempt).
# -------------------------------------------
fixPerm() {
  local file="$1"
  local desired="$2"

  # Run check and capture status
  checkPerm "$file" "$desired"
  local rc=$?

  case "$rc" in
    0)
      echo "NOOP: Permissions already correct; no change applied."
      return 0
      ;;
    1)
      # Re-normalize desired to 4 digits before chmod
      desired=$(printf "%04s" "$desired")
      echo "FIX: Applying chmod $desired to $file ..."
      if chmod "$desired" "$file"; then
        local newperm
        newperm=$(stat -c "%a" "$file")
        printf -v newperm "%04d" "$newperm"
        echo "FIXED: $file now has permissions $newperm"
        return 0
      else
        echo "ERR: Unable to fix permissions on $file"
        return 1
      fi
      ;;
    2)
      # Usage / file-not-found / invalid desired perms
      echo "ABORT: Check failed; fix not attempted."
      return 2
      ;;
    *)
      echo "ERR: Unexpected checkPerm status: $rc"
      return 1
      ;;
  esac


# -------------------------------------------
# CHECK: validate current owner/group vs desired
# Return codes:
#   0 -> match
#   1 -> mismatch (fixable)
#   2 -> check error (do not fix)
# -------------------------------------------
checkOwner() {
  local file="$1"
  local owner_group="$2"
  local opt_group="$3"
  local desired_owner desired_group actual_owner actual_group

  # Validate args
  if [[ -z "$file" || -z "$owner_group" ]]; then
    echo "Usage: checkOwner <file> <owner[:group]> [group]"
    return 2
  fi

  # File must exist
  if [[ ! -e "$file" ]]; then
    echo "WARN: File not found: $file"
    return 2
  fi

  # Parse owner[:group]
  if [[ "$owner_group" == *:* ]]; then
    desired_owner="${owner_group%%:*}"
    desired_group="${owner_group##*:}"
  else
    desired_owner="$owner_group"
    desired_group="$opt_group"
  fi

  # Validate parsed values
  if [[ -z "$desired_owner" || -z "$desired_group" ]]; then
    echo "Usage: checkOwner <file> <owner[:group]> [group]"
    return 2
  fi

  # Get actual ownership
  actual_owner=$(stat -c "%U" "$file")
  actual_group=$(stat -c "%G" "$file")

  # Compare
  if [[ "$actual_owner" == "$desired_owner" && "$actual_group" == "$desired_group" ]]; then
    echo "OK: $file owner:group OK ($actual_owner:$actual_group)"
    return 0
  else
    echo "MISMATCH: $file has $actual_owner:$actual_group, desired $desired_owner:$desired_group"
    return 1
  fi
}

# -------------------------------------------
# FIX: apply chown if checkOwner reports mismatch
#   - If checkOwner returns 1 → try fix
#   - If checkOwner returns 0 → do nothing
#   - If checkOwner returns 2 → abort
# -------------------------------------------
fixOwner() {
  local file="$1"
  local owner_group="$2"
  local opt_group="$3"

  # Run check
  checkOwner "$file" "$owner_group" "$opt_group"
  local rc=$?

  case "$rc" in
    0)
      echo "NOOP: Ownership already correct; no change applied."
      return 0
      ;;
    1)
      # Determine owner:group again (same parsing logic)
      local desired_owner desired_group
      if [[ "$owner_group" == *:* ]]; then
        desired_owner="${owner_group%%:*}"
        desired_group="${owner_group##*:}"
      else
        desired_owner="$owner_group"
        desired_group="$opt_group"
      fi

      echo "FIX: Applying chown $desired_owner:$desired_group to $file ..."
      if chown "$desired_owner:$desired_group" "$file"; then
        echo "FIXED: $file now owned by $(stat -c "%U:%G" "$file")"
        return 0
      else
        echo "ERR: Failed to change ownership of $file"
        return 1
      fi
      ;;
    2)
      echo "ABORT: Check failed; fix not attempted."
      return 2
      ;;
    *)
      echo "ERR: Unexpected checkOwner status: $rc"
      return 1
      ;;
  esac
