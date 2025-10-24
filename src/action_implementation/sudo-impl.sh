#!/usr/bin/bash
#
# ALAR implementation for fixing common issues with the sudo configurations
#
# This script is intended to fix the following conditions
# - sudoers files do not have the required 440 permissions bits
# - sudoers files are not owned by root:root
# - duplicate username definitions exist in the waagent file
# -- common byproduct of running vmaccess (reset password blade)
# -- only the sudoers.d/waagent file is moved, all other issues are
#    reported only
# - sudoers contains the 'targetpw' flag, which is common in (older?) SUSE
#   images
#
# Load helper library
IMPL_DIR=`dirname $0`
. $IMPL_DIR/helpers.sh

# Detect users directly granted sudo rights in more than one sudoers file
# Works across /etc/sudoers and /etc/sudoers.d/*

sudoers_files=$(find /etc/sudoers /etc/sudoers.d -type f 2>/dev/null)

declare -A user_files
declare -A duplicates

for file in $sudoers_files; do
  while IFS= read -r line; do
    # Skip comments and blank lines
    [[ "$line" =~ ^# ]] && continue
    [[ -z "$line" ]] && continue

    # Match lines like "azureadmin ALL=(ALL) ALL"
    if [[ "$line" =~ ^([A-Za-z0-9._%-]+)[[:space:]]+ALL[[:space:]]*=\( ]]; then
      user="${BASH_REMATCH[1]}"

      # Skip non-user keywords
      case "$user" in
        User_Alias|Runas_Alias|Host_Alias|Cmnd_Alias|Defaults)
          continue
          ;;
      esac

      # Add file only once per user
      if [[ ! " ${user_files[$user]} " =~ " $file " ]]; then
        user_files[$user]+=" $file"
      fi
    fi
  done < "$file"
done

# Now check which users appear in >1 unique file
for user in "${!user_files[@]}"; do
  file_count=$(wc -w <<<"${user_files[$user]}")
  if (( file_count > 1 )); then
    duplicates["$user"]="${user_files[$user]}"
  fi
done

if (( ${#duplicates[@]} > 0 )); then
  echo "WARN: Users with sudo privileges in multiple files:"

  for u in "${!duplicates[@]}"; do
    echo " - $u:${duplicates[$u]}"
    for f in ${duplicates[$u]}; do
      # If /etc/sudoers.d/waagent is in any duplicate list, back it up to /root
      # this is the most common failure mode, and the one specific to Azure
      # activities, so we will fix it.  This issue is where vmaccess usage has
      # overridden  cloud-init defined behavior
      if [[ "$f" == "/etc/sudoers.d/waagent" ]]; then
        timestamp=$(date +"%Y%m%d%H%M%S")
        dest="/root"

        echo "WARN: /etc/sudoers.d/waagent has duplicate entries, moving to $dest"
        backup "$f" "$dest"
      fi
    done
  done

else
  echo "OK: No users defined in more than one sudoers file."
fi

# regenerate the list, before more tests, since we might have moved a file or
# two above
sudoers_files=$(find /etc/sudoers /etc/sudoers.d -type f 2>/dev/null)

# Iterate through all the sudo config files and check/fix the permissions
# using 'helper-defined' functions
for file in $sudoers_files; do
  ls -alF $file
  checkPerm $file 0440
  checkOwner $file root:root
done

# check for the 'targetpw' setting historically from suse, but would be
# problematic wherever
if grep -q -e '^Defaults targetpw' /etc/sudoers; then
  echo "WARN: targetpw found, commenting";
  backup /etc/sudoers
  sed -i -e "s/^Defaults targetpw/#Defaults targetpw/;s/^ALL/#ALL/" /etc/sudoers
fi
# silently do nothing if it was not found