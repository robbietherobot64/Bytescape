#!/usr/bin/env python3
"""Build a publishable wiki staging tree from the repo's wiki/ folder.

Outputs (under --out):
  <out>/             copy of wiki/ minus editor junk and denylisted files,
                     plus the repository-root documents the wiki links to
  <out>/pages.json   navigation manifest: [{path, title, section}, ...]

The manifest is the single source of truth for the sidebar in
wiki/index.html — adding a markdown page and republishing is all it takes
for it to appear in navigation.

Usage:
  python3 deploy/wiki-build.py --out /tmp/wiki-staging [--repo-root DIR]
"""

import argparse
import json
import re
import shutil
import sys
from pathlib import Path

# Files under wiki/ (relative, posix) that must never be published.
# - code-audit.md is a live vulnerability inventory (see add-hosted-wiki).
# - prd.md is an archived reference superseded by VISION/mvp-scope.
# - design/ holds internal v2 target-state screen specs (working docs).
# Entries ending in "/" match a directory prefix; others match exactly.
DENYLIST = ("product/code-audit.md", "product/prd.md", "design/")

# Preferred sidebar section order; sections not listed sort after, alphabetically.
SECTION_ORDER = ("Home", "Users", "Developers", "Docs", "Architecture", "Product")

# Repository-root documents the wiki's own links reference; published at
# these paths inside the staging tree so clamped links resolve.
ROOT_DOCS = ("VISION.md", "STATUS.md", "DECISIONS.md", "docs/README.md")

HOME_SECTION = "Home"
HEADING_RE = re.compile(r"^#\s+(.+?)\s*$")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
SWAP_SUFFIXES = (".swp", ".swo", ".kate-swp", ".tmp", "~")


def is_junk(rel: Path) -> bool:
    """Editor/scratch files: any dotfile path component, or swap suffixes."""
    if any(part.startswith(".") for part in rel.parts):
        return True
    return rel.name.endswith(SWAP_SUFFIXES)


def is_denied(rel: Path) -> bool:
    """True for denylisted publish exclusions (exact paths + directory prefixes)."""
    path = rel.as_posix()
    return any(
        path.startswith(entry) if entry.endswith("/") else path == entry
        for entry in DENYLIST
    )


def title_of(text: str):
    """First top-level markdown heading, skipping fenced code blocks."""
    in_fence = False
    for line in text.splitlines():
        if FENCE_RE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        m = HEADING_RE.match(line)
        if m:
            title = m.group(1).strip()
            title = title.replace("**", "").replace("__", "").replace("`", "")
            title = title.rstrip("#").strip()
            if title:
                return title
    return None


def section_of(rel: Path) -> str:
    """Top-level directory as display section; root-level pages -> Home."""
    if len(rel.parts) == 1:
        return HOME_SECTION
    return rel.parts[0].replace("-", " ").title()


def sort_key(entry):
    """index.md first, then preferred section order, then alphabetical."""
    path = entry["path"]
    section = entry["section"]
    rank = (
        SECTION_ORDER.index(section) if section in SECTION_ORDER else len(SECTION_ORDER)
    )
    return (
        0 if path == "index.md" else 1,
        rank,
        section.lower(),
        entry["title"].lower(),
        path,
    )


def build(repo_root: Path, out: Path) -> None:
    wiki = repo_root / "wiki"
    if not wiki.is_dir():
        sys.exit(f"error: wiki directory not found: {wiki}")
    if out == repo_root or out in wiki.parents or wiki in out.parents:
        sys.exit(f"error: refusing to build into {out}")

    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    # 1. Copy wiki/, excluding junk and denylisted files.
    for src in sorted(wiki.rglob("*")):
        if not src.is_file():
            continue
        rel = src.relative_to(wiki)
        if is_junk(rel) or is_denied(rel):
            continue
        dst = out / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)

    # 2. Publish the referenced repository-root documents.
    for doc in ROOT_DOCS:
        src = repo_root / doc
        if not src.is_file():
            sys.exit(f"error: required root document missing: {doc}")
        dst = out / doc
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)

    # 3. Navigation manifest over every published .md file.
    entries = []
    for md in sorted(out.rglob("*.md")):
        rel = md.relative_to(out)
        text = md.read_text(encoding="utf-8", errors="replace")
        entries.append(
            {
                "path": rel.as_posix(),
                "title": title_of(text) or md.stem,
                "section": section_of(rel),
            }
        )
    entries.sort(key=sort_key)
    (out / "pages.json").write_text(
        json.dumps(entries, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    # 3b. Normalize permissions — this is a public site; a source file with
    #     restrictive modes (e.g. 600) must not 403 once published.
    for f in out.rglob("*"):
        f.chmod(0o755 if f.is_dir() else 0o644)

    # 4. Guard: never ship denylisted or junk files (protects against a
    #    future script edit or an accidental git mv into wiki/).
    problems = []
    for f in out.rglob("*"):
        if not f.is_file():
            continue
        rel = f.relative_to(out)
        if is_denied(rel):
            problems.append(f"denylisted file staged: {rel}")
        if is_junk(rel):
            problems.append(f"junk file staged: {rel}")
    if problems:
        sys.exit("error: " + "; ".join(problems))

    md_count = len(entries)
    file_count = sum(1 for f in out.rglob("*") if f.is_file())
    print(
        f"built {out}: {md_count} markdown pages in pages.json, "
        f"{file_count} files total"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, help="staging directory to write")
    parser.add_argument(
        "--repo-root",
        default=str(Path(__file__).resolve().parent.parent),
        help="repository root (default: parent of deploy/)",
    )
    args = parser.parse_args()
    build(Path(args.repo_root).resolve(), Path(args.out).resolve())


if __name__ == "__main__":
    main()
