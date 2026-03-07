#!/usr/bin/env python3
# -----------------------------------------------------------------------------
# Version: 1.2.0
# Released: 2025-10-31
# Latest update: 2025-12-16
# Author: Azure Support
#
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the terms found in the LICENSE file in the root of this source tree.
# -----------------------------------------------------------------------------
# Purpose: ALAR helper library (Python)
# -----------------------------------------------------------------------------

import os
import shutil
from datetime import datetime
from pathlib import Path

# Define once for the script run so all backups share the same timestamp
TIMESTAMP = datetime.now().strftime("%Y%m%dT%H%M%S")


def _copy_preserve(src: str, dst: str) -> None:
    """Copy a file preserving permissions, timestamps, and ownership."""
    shutil.copy2(src, dst)
    st = os.stat(src)
    os.chown(dst, st.st_uid, st.st_gid)


def backup(origfile: str, targetdir: str | None = None) -> int:
    """Create a backup of a file.

    Args:
        origfile:   Path to the file to back up.
        targetdir:  Optional target directory.

    Behavior:
        - If targetdir is provided: move file to targetdir with timestamp appended.
        - Otherwise: copy file into the current directory with timestamp appended.

    Returns:
        filename on success, None on failure.
    """
    if not origfile:
        print("Usage: backup(<file>, [target_dir])")
        return None

    orig = Path(origfile)

    if not orig.exists():
        print(f"ERR: File does not exist: {origfile}")
        return None

    if targetdir:
        target = Path(targetdir)
        if not target.is_dir():
            print(f"INFO: Creating backup directory: {targetdir}")
            try:
                target.mkdir(parents=True, exist_ok=True)
            except OSError as exc:
                print(f"ERR: Failed to create backup directory: {targetdir} ({exc})")
                return None

        dest = target / f"{orig.name}.alar.{TIMESTAMP}"
        print(f"INFO: Copying {origfile} to {dest}")
        _copy_preserve(str(orig), str(dest))
        return str(dest)
    else:
        backup_path = Path(f"{origfile}.alar.{TIMESTAMP}")
        print(f"INFO: backing up {origfile} to {backup_path}")
        _copy_preserve(str(orig), str(backup_path))

    return str(backup_path)
