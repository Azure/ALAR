#!/usr/bin/bash
TIMESTAMP=`date +%Y%m%dT%H%M%S`

function backup() {
  ORIGFILE=$1
  BACKUP=$1.alar.$TIMESTAMP
  cp -v -p $ORIGFILE $BACKUP
}
