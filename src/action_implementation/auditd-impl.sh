#!/usr/bin/bash
# Load helper library
IMPL_DIR=`dirname $0`
. $IMPL_DIR/helpers.sh

function check_space_for_mount() {
  # Since we must be prepared to work in a rescue environment, we cannot assume the disk is mounted.
  # Pull the log location from the auditd config

  # parse the fstab file to locate where this directory is rooted
  #DIRTEST=$LOGDIR
  DIRTEST=$1
  DIRTESTIN=$DIRTEST
  echo "Testing $DIRTEST"

  until ( grep -q -w $DIRTEST /etc/fstab ); do
    echo "$DIRTEST not in fstab"
    # since we are iterating, strip the deepest subdirectory and store it back to the test variable
    DIRTEST=`dirname $DIRTEST`
    # Set these vars any time we check them, so that we know we don't ever have to look again
    if [ $DIRTEST == "/" ]; then NEEDROOT=0; fi
    if [ $DIRTEST == "/var" ]; then NEEDVAR=0; fi
    if [ $DIRTEST == "/var/log" ]; then NEEDVARLOG=0; fi
  done
  echo "found mounted directory for $DIRTESTIN is $DIRTEST"

  FSTABLINE=`grep -w $DIRTEST /etc/fstab`

  LOGDEV=`echo $FSTABLINE | cut -d' ' -f 1`
  # Sanity check - most that use LVM do not do UUIDs, so that should be a good indication, but derive
  #  the underlying volume anyway
  if ( echo $LOGDEV | grep -q -w "UUID" ); then
    echo "UUID Present"
    # UUID in fstab, we need to derive the volume
    LOGDEV=`blkid | grep $LOGDEV | cut -d: -f 1`
    echo found $LOGDEV behind UUID
  fi

  # Verify what type of device the logfile is sitting on and act appropriately
  DEVTYPE=`lsblk -n -o TYPE $LOGDEV`
  if [ $DEVTYPE == "lvm" ]; then
    echo "mountpoint device confirmed as LVM"; 
    # check if the log filesystem is mounted, so we can check for free space
    if ! ( grep -q -w $LOGDEV /proc/mounts ); then
      echo "device not mounted for $DIRTEST, will mount for testing"
      mkdir $AUDITTEMP
      mount $LOGDEV $AUDITTEMP
      # change our target dir to match the new temp mount
      LOGDIR=$AUDITTEMP
    fi
    
    LOGLVUSEDPCENT=`df $DIRTEST --output=pcent | tail -n 1 | tr -d [:space:] | tr -d '%'`
    # We are setting the threshold at 95%, since auditd should ideally take 100% util to 
    #  shut the system down.
    if [ $LOGLVUSEDPCENT -gt 95 ]; then
      # find VG for LV
      LOGLV=`lvs $LOGDEV --no-headings --o lv_name | tr -d [:space:]`
      LOGVG=`lvs $LOGDEV --no-headings --o vg_name | tr -d [:space:]`

      echo "$LOGLV is nearly full:$LOGLVUSEDPCENT%  Will try to add 10% if available in the VG"
      # Try to expand by a bit
      
      LOGLVSIZEKB=`lvs -v $LOGDEV --units k --o lv_size --no-headings | tr -d [:space:] | cut -d. -f 1`
      LOGLVADDKB=$(($LOGLVSIZEKB/10))
      LOGVGFREEKB=`vgs -v $LOGVG -o vg_free --no-headings --units k | tr -d [:space:] | cut -d. -f 1`
      if [ $LOGVGFREEKB -gt $LOGLVADDKB ]; then
        echo "$LOGVG has more than enough space left ( $LOGVGFREEKB > $LOGLVADDKB ).  Will attempt to grow $LOGLV"
        lvextend /dev/$LOGVG/$LOGLV -L +$(($LOGLVADDKB))k --resizefs
        # need to check if --resizefs actually worked
      else
        echo "Not enough free space to grow $LOGLV, wanted $LOGLVADDKB, $LOGVGFREEKB available"
      fi

    else
      echo "Used space in $DIRTEST under 95% - $LOGLVUSEDPCENT% utilized"
    fi
  else 
    echo "audit log volume is not LVM, will not attempt to alter size"; 
  fi
}

# This script will attempt two fixes to non-booting systems who are suspected to be 
#  out of space and shutting down due to auditd rules
# 1. Disable any rules in the auditd config which shut down the system on failure to log
# 2. Should the logical volume (LV) holding the configured log location be over 95%, try
#    to expand it, but will attempt to grow the LV should there be room in the containing 
#    volume group.  Will not touch a RAW partition

AUDACT="HALT"
# TEST condition
# AUDACT="SUSPEND"
NEWAUDACT="SYSLOG"

CONFIGFILE="/etc/audit/auditd.conf"
AUDITTEMP="/tmp/auditlog"


# start editing auditd.conf file
if ( grep -i $AUDACT -q  $CONFIGFILE ); then
  echo "$AUDACT directives found in $CONFIGFILE"
  echo "Creating backup of $CONFIGFILE before altering"
  backup $CONFIGFILE
  echo "Changing $AUDACT in $CONFIGFILE to $NEWAUDACT"
  sed -i s/$AUDACT/$NEWAUDACT/ig $CONFIGFILE
else
  echo "No $AUDACT directives found in config files"
fi

# end auditd.conf

# start file system space checks
# IF any mountpoint is full-ish AND (is LVM && free extents in the VG), we will try to expand 
#  the LV 5% of its current size from the VG's free extents.
# -- will not attempt to work on RAW partitions
echo "Checking for full filesystems"
# vars to check / and /var also, because we should process those as well as audit log location
NEEDROOT=1
NEEDVAR=1
NEEDVARLOG=1
# audit log location first
LOGFILEPATH=`egrep '^log_file' $CONFIGFILE | tr -d '[:space:]' | cut -d '=' -f 2`
LOGDIR=`dirname $LOGFILEPATH`
echo "- Checking the log location from $CONFIGFILE"
check_space_for_mount $LOGDIR

echo "- Checking important locations"
if [ $NEEDROOT -eq 1 ] ; then
  echo "-- Checking /"
  check_space_for_mount "/"
else
  echo "-- Already processed /"
fi
if [ $NEEDVAR -eq 1 ] ; then
  echo "-- Checking /var"
  check_space_for_mount "/var"
else
  echo "-- Already processed /var"
fi
if [ $NEEDVARLOG -eq 1 ] ; then
  echo "-- Checking /var/log"
  check_space_for_mount "/var/log"
else
  echo "-- Already processed /var/log"
fi
# end filesystem modification
