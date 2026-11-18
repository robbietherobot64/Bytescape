#!/usr/bin/env python3
"""Backfill 11.3 — bounded listRecords pull of calendar events from an
allowlist of prolific authors (spec: events/ingestion historical catch-up).

Follows the firehose_census.py pattern: small, stdlib-only, printed output.
SQL goes to STDOUT (pipe into psql); progress and caps go to STDERR so the
run is auditable:

    python3 ops/backfill-calendar-authors.py \
        | psql "$DATABASE_URL"

Bounded by design: --limit caps records per author (default 200) and pages
stop at the cap. No network-wide scans — allowlist only, grow it deliberately.
"""

import argparse
import datetime
import json
import sys
import urllib.parse
import urllib.request

# Known prolific community.lexicon.calendar publishers. Grow deliberately:
# calendar.city alone has published 2k+ events (see atmo.rsvp/p/calendar.city).
DEFAULT_HANDLES = [
    "calendar.city",
    "atmo.rsvp",
    "atmosphere.community",
]

NSIDS = (
    "community.lexicon.calendar.event",
    "events.smokesignal.Event",
    "events.smokesignal.calendar.event",
)

# Mirror of calendar::parse_event's required-field rule: a non-empty name.
# Canonical records additionally require createdAt; legacy may omit it.
LOCATION_TEXT_KEYS = ("name", "street", "locality", "region", "postalCode", "description")


def log(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def http_json(url: str) -> dict:
    req = urllib.request.Request(url, headers={"User-Agent": "bytescape-backfill/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)


def resolve_did(handle: str) -> str:
    qs = urllib.parse.urlencode({"handle": handle})
    return http_json(f"https://bsky.social/xrpc/com.atproto.identity.resolveHandle?{qs}")["did"]


def resolve_pds(did: str) -> str:
    doc = http_json(f"https://plc.directory/{did}")
    for svc in doc.get("service", []):
        # PLC docs use both the older "#atproto_pds" and the newer
        # "AtprotoPersonalDataServer" type forms.
        if svc.get("type") in ("#atproto_pds", "AtprotoPersonalDataServer"):
            endpoint = svc.get("serviceEndpoint") or svc.get("endpoint") or ""
            if isinstance(endpoint, dict):
                endpoint = endpoint.get("endpoint", "")
            if endpoint:
                return endpoint.rstrip("/")
    raise RuntimeError(f"no atproto PDS in DID doc for {did}")


def rfc3339_to_tsql(value) -> str | None:
    if not value or not isinstance(value, str):
        return None
    try:
        dt = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return "'" + dt.astimezone(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S+00") + "'"


def sql_str(value) -> str:
    if value is None:
        return "NULL"
    if not isinstance(value, str):
        value = json.dumps(value, ensure_ascii=False)
    return "'" + value.replace("\x00", "").replace("'", "''") + "'"


def location_text(locations) -> str | None:
    if not isinstance(locations, list):
        return None
    parts: list[str] = []
    for item in locations:
        if not isinstance(item, dict):
            continue
        for key in LOCATION_TEXT_KEYS:
            s = item.get(key)
            if isinstance(s, str) and s.strip() and s not in parts:
                parts.append(s)
    return ", ".join(parts) if parts else None


def backfill_author(handle: str, limit: int) -> tuple[int, int]:
    """Pull up to `limit` events for one author. Returns (fetched, emitted)."""
    did = resolve_did(handle)
    pds = resolve_pds(did)
    log(f"{handle} → {did} (pds: {pds})")

    emitted = fetched = 0
    cursor: str | None = None
    while fetched < limit:
        qs = urllib.parse.urlencode({
            "repo": did,
            "collection": "community.lexicon.calendar.event",
            "limit": min(100, limit - fetched),
            **({"cursor": cursor} if cursor else {}),
        })
        page = http_json(f"{pds}/xrpc/com.atproto.repo.listRecords?{qs}")
        records = page.get("records", [])
        if not records:
            break
        for rec in records:
            fetched += 1
            uri = rec.get("uri", "")
            cid = rec.get("cid", "")
            value = rec.get("value") or {}
            name = (value.get("name") or "").strip()
            if not uri or not cid or not name:
                continue  # mirror: missing/empty name never reaches the index
            locations = value.get("locations") if isinstance(value.get("locations"), list) else []
            uris = value.get("uris") if isinstance(value.get("uris"), list) else []
            ltext = location_text(locations)
            print(
                "INSERT INTO calendar_events ("
                "uri, cid, author_did, name, description, starts_at, ends_at, "
                "mode, status, location_text, locations, uris, record_created_at, source"
                ") VALUES ("
                f"{sql_str(uri)}, {sql_str(cid)}, {sql_str(did)}, {sql_str(name)}, "
                f"{sql_str(value.get('description'))}, "
                f"{rfc3339_to_tsql(value.get('startsAt')) or 'NULL'}, "
                f"{rfc3339_to_tsql(value.get('endsAt')) or 'NULL'}, "
                f"{sql_str(value.get('mode'))}, "
                f"{sql_str(value.get('status') or 'community.lexicon.calendar.event#scheduled')}, "
                f"{sql_str(ltext)}, {sql_str(json.dumps(locations))}, "
                f"{sql_str(json.dumps(uris))}, "
                f"{rfc3339_to_tsql(value.get('createdAt')) or 'NULL'}, "
                "'atproto'"
                ") ON CONFLICT (uri) DO NOTHING;"
            )
            emitted += 1
        cursor = page.get("cursor")
        if not cursor:
            break
    log(f"  fetched={fetched} emitted={emitted} (cap {limit})")
    return fetched, emitted


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handles", nargs="*", default=DEFAULT_HANDLES)
    parser.add_argument("--limit", type=int, default=200, help="max records per author")
    args = parser.parse_args()

    print("BEGIN;")
    total_fetched = total_emitted = 0
    for handle in args.handles:
        try:
            fetched, emitted = backfill_author(handle, args.limit)
            total_fetched += fetched
            total_emitted += emitted
        except Exception as exc:  # one bad author must not kill the run
            log(f"  ERROR {handle}: {exc}")
    print("COMMIT;")
    log(f"done: authors={len(args.handles)} fetched={total_fetched} emitted={total_emitted} (bounded by --limit)")


if __name__ == "__main__":
    main()
