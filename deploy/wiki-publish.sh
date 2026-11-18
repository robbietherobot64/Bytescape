#!/usr/bin/env bash
# Publish a wiki staging tree to the live document root with a guarded swap.
#
# Usage:
#   deploy/wiki-publish.sh --staging DIR [--host user@host] [--key FILE]
#                          [--remote-base DIR] [--local-root DIR]
#
# Default mode: rsync DIR/ to <host>:/srv/bytescape/wiki.staging, then swap
# over SSH (wiki -> wiki.prev, wiki.staging -> wiki). A swap only happens if
# the staged tree contains index.html; any failure leaves the live tree as-is.
#
# --local-root DIR: perform the same staging + swap under DIR with no network
# access — used for dry runs and failure drills.

set -euo pipefail

STAGING=""
HOST=""
KEY=""
REMOTE_BASE="/srv/bytescape"
LOCAL_ROOT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --staging)     STAGING="${2:?--staging needs a value}"; shift 2 ;;
    --host)        HOST="${2:?--host needs a value}"; shift 2 ;;
    --key)         KEY="${2:?--key needs a value}"; shift 2 ;;
    --remote-base) REMOTE_BASE="${2:?--remote-base needs a value}"; shift 2 ;;
    --local-root)  LOCAL_ROOT="${2:?--local-root needs a value}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$STAGING" ]; then
  echo "error: --staging is required" >&2
  exit 2
fi

# Guard before anything touches the live tree: the staging tree must be a
# plausible wiki (has its entry page).
if [ ! -f "$STAGING/index.html" ]; then
  echo "error: staging tree invalid (missing index.html): $STAGING" >&2
  exit 1
fi

# The swap itself, run either locally or on the server via stdin.
# Checks staged content again server-side, then renames; restores the live
# tree if the second rename somehow fails.
SWAP_SCRIPT=$(cat <<'EOS'
set -euo pipefail
if [ ! -f wiki.staging/index.html ]; then
  echo "error: refusing to swap — wiki.staging/index.html missing" >&2
  exit 1
fi
moved=0
if [ -d wiki ]; then
  rm -rf wiki.prev
  mv wiki wiki.prev
  moved=1
fi
if ! mv wiki.staging wiki; then
  if [ "$moved" = 1 ]; then mv wiki.prev wiki; fi
  echo "error: swap failed — previous wiki restored" >&2
  exit 1
fi
echo "published: $(pwd)/wiki"
EOS
)

if [ -n "$LOCAL_ROOT" ]; then
  mkdir -p "$LOCAL_ROOT"
  rm -rf "$LOCAL_ROOT/wiki.staging"
  cp -a "$STAGING" "$LOCAL_ROOT/wiki.staging"
  (cd "$LOCAL_ROOT" && bash -c "$SWAP_SCRIPT")
  exit 0
fi

if [ -z "$HOST" ]; then
  echo "error: --host is required (or use --local-root for a dry run)" >&2
  exit 2
fi

SSH_ARGS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes)
if [ -n "$KEY" ]; then
  SSH_ARGS+=(-i "$KEY")
fi

rsync -az --delete -e "ssh ${SSH_ARGS[*]}" "$STAGING/" "$HOST:$REMOTE_BASE/wiki.staging/"
printf '%s\n' "$SWAP_SCRIPT" | ssh "${SSH_ARGS[@]}" "$HOST" "cd $REMOTE_BASE && bash -s"
