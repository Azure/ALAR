#!/usr/bin/python3
# -----------------------------------------------------------------------------
# Version: 2.0.0
# Initial release: 2026-03-01
# Latest update: 2026-03-01
# Author: Azure Support
#
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.
# -----------------------------------------------------------------------------
# Purpose: ALAR implementation for fixing fstab issues
#
# This script is a python reimplementation of the v1 fstab action, designed to
#  align fstab formation to best practices and allow the system to boot
#  successfully in case where data disks are missing or have problems mounting
# -----------------------------------------------------------------------------

from pprint import pprint
import os
import stat

# import helper functions for backup and other file operations
from helpers import backup, TIMESTAMP

# various constants
PSEUDO_SPECS = {"none", "proc", "sysfs", "tmpfs", "devpts", "cgroup"}


def read_fstab(path="/etc/fstab"):
  # Pull in the contents of fstab into a list of dicts for evaluation, we're going to
  #  re-read fstab later when we modify it so that we preserve comments
  entries = []
  with open(path) as f:
    for line in f:
      line = line.strip()
      if not line or line.startswith("#"):
        continue
      fields = line.split()
      if len(fields) < 2:
        continue
      entries.append({
        "spec": fields[0],
        "mount": fields[1],
        "fstype": fields[2],
        "opts": fields[3],
        "dump": fields[4],
        "pass": fields[5],
      })
  return entries

def find_root_entry(fstab_entries):
  # pull out the entry for "/", sort of silly to make a whole function but it makes for readable code
  for e in fstab_entries:
    if e["mount"] == "/":
      return e
  raise RuntimeError("No root (/) entry found in fstab")

def resolve_fs_spec(spec):
  # turn the spec field from fstab into a real path to the first thing looking like a block device, whether it's a UUID,
  #   label, logical volume, or actual device.  Our usage is to simply verify that the device this entry is for is the
  #   same one as the 'root' device, nothing else, so we will avoid much of the complexity of handling all
  #   potential nuances
  # 1. Ignore pseudo-filesystems
  if spec in PSEUDO_SPECS:
    return None
  # 2. Decode KEY=VALUE forms down to a device path in the /dev/disk/by-* directories, which should be pointers to the real disk if it's here
  if "=" in spec:
    key, val = spec.split("=", 1)
    key = key.lower()
    if key == "uuid":
      path = f"/dev/disk/by-uuid/{val}"
    elif key == "label":
      path = f"/dev/disk/by-label/{val}"
    elif key == "partuuid":
      path = f"/dev/disk/by-partuuid/{val}"
    elif key == "partlabel":
      path = f"/dev/disk/by-partlabel/{val}"
    else:
      # there's an '=' in the spec but it's some other key we don't understand, so won't deal with it
      return None
  # 3. Plain path
  else:
    path = spec

  # Probably referencing a data disk that isn't here, None will signify it to the caller
  if not os.path.exists(path):
    return None

  real = os.path.realpath(path)
  st = os.stat(real)

  if not stat.S_ISBLK(st.st_mode):
    return None

  return real

def _base_disk(name):
  # If the 'disk' being passed is a partition, we need to get the base disk for comparison of the base
  # sda3 -> sda, nvme0n1p2 -> nvme0n1
  while name and name[-1].isdigit():
    name = name[:-1]
  if name.endswith("p"):
    name = name[:-1]
  return name


def physical_disks_for_device(devpath):
    real = os.path.basename(os.path.realpath(devpath))
    disks = set()
    def walk(dev):
        sys_path = f"/sys/block/{dev}"
        # Device‑mapper node
        if dev.startswith("dm-"):
            slaves = os.path.join(sys_path, "slaves")
            if os.path.isdir(slaves):
                for s in os.listdir(slaves):
                    walk(s)
            return
        # Partition → parent disk
        disks.add(_base_disk(dev))
    walk(real)
    return disks

def find_actual_root_device():
  # Find the device backing the currently mounted root filesystem via stat
  dev = os.stat('/').st_dev
  major, minor = os.major(dev), os.minor(dev)
  return os.path.realpath(f"/dev/block/{major}:{minor}")

### End of helper functions ###
### Main logic ###

print(f"Running fstab action at {TIMESTAMP}")

fstab_entries = read_fstab()
root_entry = find_root_entry(fstab_entries)
root_disk = physical_disks_for_device(resolve_fs_spec(root_entry["spec"]))
pprint(f"Root base disk derived from fstab currently: {root_disk}")

# check root_disk against os.environ['RECOVER_DISK_PATH'], this could be a problem if the fstab and the 'detected' os disk
#   are not the same, but this is a really strange situation, maybe the fstab / spec is wrong
if "RECOVER_DISK_PATH" in os.environ:
  recover_disk = os.environ["RECOVER_DISK_PATH"]
  recover_base = _base_disk(os.path.basename(recover_disk))
  print(f"Recovery disk from ALAR environment variable RECOVER_DISK_PATH: {recover_disk}")
  if recover_base not in root_disk:
    print(f"Notice: Detected recovery disk {recover_disk} does not match root disk {root_disk} specified in fstab")
    print("Determining actual root filesystem device to validate...")
    actual_root_dev = find_actual_root_device()
    if actual_root_dev is None:
      raise RuntimeError("Cannot determine actual root device from /proc/mounts, cannot validate fstab/system sanity, exiting...")
    actual_root_disks = physical_disks_for_device(actual_root_dev)
    print(f"Actual root filesystem device: {actual_root_dev} (physical disks: {actual_root_disks})")
    if actual_root_disks != root_disk:
      print(f"Error: Actual root device physical disks {actual_root_disks} do not match root disk {root_disk} from fstab")
      raise RuntimeError("Actual root device does not match root disk in fstab, cannot validate fstab/system sanity, exiting...")
    print("Actual root device matches fstab root disk, proceeding...")

# check if the root_disk is exactly one item, otherwise we'd have a spanned disk.  This may never happen as a spanned / volume
#   should not mount and/or activate in the ALAR scenario, but here for completeness.
if len(root_disk) != 1:
  print(f"Warning: Root disk {root_disk} does not resolve to exactly one physical disk, this is unexpected and may indicate a problem with the fstab entry for /, please investigate manually...")
  raise RuntimeError("Root disk does not resolve to exactly one physical disk, cannot validate fstab/system sanity, exiting...")

# back up fstab
backup_result = backup("/etc/fstab")
if backup_result is None:
  print("Failed to back up fstab, aborting modification to prevent potential data loss")
  raise RuntimeError("Failed to back up fstab, aborting modification to prevent potential data loss")
else:
  print(f"Backed up fstab to {backup_result}")

# Open fstab (again), and read it in line by line, store the new file 'in memory' and then write it into the original file for keeping owner/mode
new_lines = []
with open("/etc/fstab", "r") as f:
  for raw in f:
    line = raw.rstrip("\n")
    stripped = line.strip()

    # If the line is blank or a comment, just keep it as is and add to the array
    if not stripped or stripped.startswith("#"):
      new_lines.append(line)
      continue

    fields = stripped.split()
    if len(fields) < 6:
      # malformed line; comment it out to avoid boot failure
      new_lines.append("# Line below commented because of syntax error (fewer than 6 fields)")
      new_lines.append("# " + line)
      continue

    entry = {
      "spec": fields[0],
      "mount": fields[1],
      "fstype": fields[2],
      "opts": fields[3],
      "dump": fields[4],
      "pass": fields[5],
    }

    fs_dev = resolve_fs_spec(entry["spec"])
    this_disk = physical_disks_for_device(fs_dev) if fs_dev else None
    if this_disk == root_disk:
      # this volume is on the root disk, we can just pass it as-is
      new_lines.append(line)
    else:
      # this volume is not on the root disk, we should be fine to just add nofail to the options
      if 'nofail' not in entry["opts"].split(","):
        entry["opts"] += ",nofail"
        new_line = f"{entry['spec']}\t{entry['mount']}\t{entry['fstype']}\t{entry['opts']}\t{entry['dump']}\t{entry['pass']}"
        new_lines.append("# nofail added to the next entry by ALAR")
        new_lines.append(new_line)
      else:
        new_lines.append(line)

# write out the new fstab
with open("/etc/fstab", "w") as f:
  for l in new_lines:
    f.write(l + "\n")