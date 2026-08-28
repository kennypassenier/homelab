#!/bin/bash
DATE=$(date +%Y%m%d-%H%M)
LOGFILE="/var/log/zfs_appdata_backup.log"
MAILTO="mendax1@gmail.com"
SUBJECT="ZFS Appdata Backup Report $(date '+%Y-%m-%d %H:%M')"

# Log rotation: max 10MB, keep previous as .old
MAXSIZE=10485760
if [ -f "$LOGFILE" ] && [ $(stat -c%s "$LOGFILE") -ge $MAXSIZE ]; then
  mv "$LOGFILE" "$LOGFILE.old"
fi

# Log all output
exec > >(tee -a "$LOGFILE") 2>&1

echo "=== ZFS Appdata Backup & Check === $DATE"

# Retention function: keep all for 7 days, then 1 per week for 3 months
prune_retention() {
  local DS="$1"
  local PREFIX="$2"
  local NOW=$(date +%s)
  local DAY_SEC=86400
  declare -A KEEP
  declare -A WEEKLY

  mapfile -t SNAPS < <(zfs list -H -t snapshot -o name,creation -s creation | grep "^$DS@$PREFIX" | awk '{print $1 "," $2}')
  for entry in "${SNAPS[@]}"; do
    SNAP="${entry%%,*}"
    DATESNAP="${entry##*,}"
    AGE=$(( (NOW - DATESNAP) / DAY_SEC ))
    if (( AGE <= 7 )); then
      KEEP["$SNAP"]=1
    elif (( AGE <= 90 )); then
      WEEKKEY=$(date -d @"$DATESNAP" +%G-%V)
      WEEKLY["$WEEKKEY"]="$SNAP"
    fi
  done
  for snap in "${WEEKLY[@]}"; do
    KEEP["$snap"]=1
  done
  for entry in "${SNAPS[@]}"; do
    SNAP="${entry%%,*}"
    if [[ -z "${KEEP[$SNAP]:-}" ]]; then
      echo "Pruning old snapshot (retention): $SNAP"
      zfs destroy "$SNAP"
    fi
  done
}

# Main backup/replication loop
for DS in $(zfs list -H -o name | grep '^HDD2TB/appdata_'); do
  echo "Snapshot & replicate: $DS"
  zfs snapshot -r ${DS}@backup-$DATE

  TGT_DS="HDD18TB/REPLICA_2TB/$(basename $DS)"
  # Find latest common snapshot
  SRC_SNAPS=($(zfs list -H -t snapshot -o name -s creation | grep "^${DS}@backup-" | awk -F@ '{print $2}'))
  TGT_SNAPS=($(zfs list -H -t snapshot -o name -s creation | grep "^${TGT_DS}@backup-" | awk -F@ '{print $2}'))
  COMMON=""
  for (( idx=${#SRC_SNAPS[@]}-1 ; idx>=0 ; idx-- )); do
    if [[ " ${TGT_SNAPS[*]} " =~ " ${SRC_SNAPS[$idx]} " ]]; then
      COMMON=${SRC_SNAPS[$idx]}
      break
    fi
  done

  if [[ -n "$COMMON" ]]; then
    echo "Incremental send from $COMMON"
    zfs send -RI ${DS}@$COMMON ${DS}@backup-$DATE | zfs receive -F $TGT_DS
  else
    echo "No common snapshot, destroying all target snapshots for $TGT_DS and children"
    zfs list -H -t snapshot -o name | grep "^${TGT_DS}" | xargs -r -n 1 zfs destroy
    echo "Doing full send"
    zfs send -R ${DS}@backup-$DATE | zfs receive -F $TGT_DS
  fi

  # Source: keep only last 7 snapshots
  zfs list -t snapshot -o name | grep "${DS}@backup-" | head -n -7 | xargs -n 1 zfs destroy 2>/dev/null

  # Replica: retention policy
  prune_retention "$TGT_DS" "backup-"
done

# Pools themselves (optional)
for POOL in HDD2TB HDD4TB; do
  zfs snapshot -r ${POOL}@backup-$DATE
  SRC_DS="${POOL}"
  TGT_DS="HDD18TB/REPLICA_${POOL:3}"
  SRC_SNAPS=($(zfs list -H -t snapshot -o name -s creation | grep "^${SRC_DS}@backup-" | awk -F@ '{print $2}'))
  TGT_SNAPS=($(zfs list -H -t snapshot -o name -s creation | grep "^${TGT_DS}@backup-" | awk -F@ '{print $2}'))
  COMMON=""
  for (( idx=${#SRC_SNAPS[@]}-1 ; idx>=0 ; idx-- )); do
    if [[ " ${TGT_SNAPS[*]} " =~ " ${SRC_SNAPS[$idx]} " ]]; then
      COMMON=${SRC_SNAPS[$idx]}
      break
    fi
  done

  if [[ -n "$COMMON" ]]; then
    echo "Incremental send from $COMMON"
    zfs send -RI ${SRC_DS}@$COMMON ${SRC_DS}@backup-$DATE | zfs receive -F $TGT_DS
  else
    echo "No common snapshot, destroying all target snapshots for $TGT_DS and children"
    zfs list -H -t snapshot -o name | grep "^${TGT_DS}" | xargs -r -n 1 zfs destroy
    echo "Doing full send"
    zfs send -R ${SRC_DS}@backup-$DATE | zfs receive -F $TGT_DS
  fi

  zfs list -t snapshot -o name | grep "${SRC_DS}@backup-" | head -n -7 | xargs -n 1 zfs destroy 2>/dev/null
  prune_retention "$TGT_DS" "backup-"
done

echo ""
echo "Latest snapshots per appdata dataset (source):"
for DS in $(zfs list -H -o name | grep '^HDD2TB/appdata_'); do
  echo -n "$DS: "
  zfs list -t snapshot -o name -s creation | grep "${DS}@backup-" | tail -n 1
done

echo ""
echo "Latest snapshots per appdata dataset (target):"
for DS in $(zfs list -H -o name | grep '^HDD18TB/REPLICA_2TB/appdata_'); do
  echo -n "$DS: "
  zfs list -t snapshot -o name -s creation | grep "${DS}@backup-" | tail -n 1
done

echo ""
echo "ZFS appdata backup, replication and check completed at $DATE"

# --- HTML Email Report ---
SUMMARY=$(awk '
/cannot receive new filesystem stream/ {fail=1; print "<b style=\"color:red;\">&#10060; ERROR:</b> " $0 "<br/>"}
/ZFS appdata backup, replication and check completed/ {ok=1}
END {
  if (fail) print "<br/><b style=\"color:red;\">Attention: Errors occurred during replication!</b><br/>"
  if (ok && !fail) print "<b style=\"color:green;\">&#9989; Backup and replication completed successfully.</b><br/>"
}
' "$LOGFILE")

TAILLOG=$(tail -n 50 "$LOGFILE" | sed 's/$/<br\/>/')

{
  echo "Content-Type: text/html"
  echo "Subject: $SUBJECT"
  echo "To: $MAILTO"
  echo ""
  echo "<html><body>"
  echo "<h2>ZFS Appdata Backup & Replication</h2>"
  echo "<b>Date:</b> $(date '+%Y-%m-%d %H:%M')<br/><br/>"
  echo "$SUMMARY"
  echo "<hr>"
  echo "<b>Last 50 log lines:</b><br/>"
  echo "<div style=\"font-family:monospace; font-size:12px; background:#f4f4f4; border:1px solid #ccc; padding:8px;\">"
  echo "$TAILLOG"
  echo "</div>"
  echo "</body></html>"
} | /usr/sbin/sendmail -t