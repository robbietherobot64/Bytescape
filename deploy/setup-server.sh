#!/usr/bin/env bash
# Idempotent Bytescape server bootstrap for a fresh Debian/Ubuntu VPS.
#
# Installs: Caddy (TLS + durable static OAuth metadata + reverse proxy),
# PostgreSQL (local), the bytescape system user, and the app systemd unit.
#
# Run from the repo's deploy/ directory (or pass its path as $1):
#   sudo ./setup-server.sh
#
# What this script does NOT do: deploy the app binary. Drop the release
# binary at /opt/bytescape/bytescape-server (e.g. via /opt/deploy.sh from
# GitLab CI, or scp), then: sudo systemctl restart bytescape

set -euo pipefail

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR=/opt/bytescape
STATIC_DIR=/srv/bytescape/static
DOMAIN=bytescape.social

[ "$(id -u)" -eq 0 ] || { echo "run as root"; exit 1; }

echo "==> 0/6 Swap (small instances OOM without it)"
if ! swapon --show | grep -q .; then
  fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  echo "     2G swapfile active"
fi

echo "==> 1/6 PostgreSQL"
if ! command -v psql >/dev/null; then
  apt-get update -qq
  apt-get install -y -qq postgresql
fi
systemctl enable --now postgresql
# App role/db (idempotent). Password matches the .env you will place at
# $APP_DIR/.env — change both together if you change it.
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='postgres'" >/dev/null
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname='bytescape'" | grep -q 1 \
  || sudo -u postgres createdb -O postgres bytescape

echo "==> 2/6 Caddy (TLS termination + reverse proxy)"
if ! command -v caddy >/dev/null; then
  apt-get update -qq
  # Distro package first (works on all Debian/Ubuntu releases); fall back to
  # Caddy's official repo only if the distro doesn't ship caddy.
  apt-get install -y -qq caddy || {
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https curl
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
      | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
      > /etc/apt/sources.list.d/caddy-stable.list
    apt-get update -qq
    apt-get install -y -qq caddy
  }
fi
systemctl enable --now caddy

echo "==> 3/6 Static OAuth client metadata (durable; independent of app)"
mkdir -p "$STATIC_DIR"
cp "$DEPLOY_DIR/client-metadata.json" "$STATIC_DIR/client-metadata.json"
chmod 644 "$STATIC_DIR/client-metadata.json"

echo "==> 4/6 Caddyfile"
cp "$DEPLOY_DIR/Caddyfile" /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy

echo "==> 5/6 App user + directory"
id -u bytescape >/dev/null 2>&1 || useradd --system --home "$APP_DIR" --shell /usr/sbin/nologin bytescape
mkdir -p "$APP_DIR/uploads"
chown -R bytescape:bytescape "$APP_DIR"

echo "==> 6/6 App systemd unit"
cp "$DEPLOY_DIR/bytescape.service" /etc/systemd/system/bytescape.service
systemctl daemon-reload
systemctl enable bytescape

cat <<EOF

Bootstrap complete.

Remaining manual steps:
  1. Place the app environment at $APP_DIR/.env  (DATABASE_URL, JWT_SECRET,
     BLUESKY_CLIENT_ID=https://$DOMAIN/oauth/client-metadata.json,
     BLUESKY_REDIRECT_URI, FRONTEND_URL, DOMAIN=$DOMAIN).
  2. Place the release binary at $APP_DIR/bytescape-server (chown bytescape).
  3. sudo systemctl start bytescape
  4. Verify the durable fix:
       curl -i https://$DOMAIN/oauth/client-metadata.json
     (must return 200 even with the app STOPPED — that is the point)
EOF
