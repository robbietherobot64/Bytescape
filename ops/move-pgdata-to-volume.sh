#!/bin/bash
# Move the Postgres data directory onto the attached Hetzner volume (ext4).
# Run with DRY_RUN=1 first to confirm the target device. Requires the raw
# volume attached as an unused block device (NOT sda).
set -euo pipefail

DEV=$(lsblk -dpno NAME,TYPE | awk '$2=="disk" && $1 !~ /sda|sr0/ {print $1; exit}')
if [ -z "$DEV" ]; then
  echo "no unused block device found (attach the volume first)"
  exit 1
fi
echo "target volume: $DEV ($(lsblk -dno SIZE "$DEV"))"
if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "dry run — exiting"
  exit 0
fi

# 1) partition + ext4 (skip partitioning if one already exists)
if [ ! -b "${DEV}1" ]; then
  parted -s "$DEV" mklabel gpt mkpart primary 0% 100%
  sleep 1
fi
mkfs.ext4 -q -L pgspace "${DEV}1"

# 2) mount + fstab (mount via the stable path directly)
mkdir -p /mnt/pgsql
mount "${DEV}1" /mnt/pgsql
grep -q pgspace /etc/fstab || echo "LABEL=pgspace /var/lib/postgresql ext4 defaults,nofail 0 2" >> /etc/fstab

# 3) stop postgres, copy data (preserve permissions/links), swap mounts
systemctl stop postgresql@18-main
rsync -aHAX /var/lib/postgresql/ /mnt/pgsql/
mv /var/lib/postgresql /var/lib/postgresql.old
mkdir -p /var/lib/postgresql
umount /mnt/pgsql
mount /var/lib/postgresql
chown -R postgres:postgres /var/lib/postgresql
systemctl start postgresql@18-main
sleep 3
systemctl is-active postgresql@18-main
sudo -u postgres psql -c "SELECT pg_size_pretty(pg_database_size('bytescape'));"
df -h / /var/lib/postgresql
echo "OK — /var/lib/postgresql.old kept; remove manually after verification"
