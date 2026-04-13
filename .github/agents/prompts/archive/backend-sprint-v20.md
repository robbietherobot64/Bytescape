# Backend Sprint v20 — Profile Final Polish Support & Pre-Web Gate

**Sprint**: v20 (Profile Final Polish Support)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-11  
**Previous sprint**: backend profile helpers already landed; this sprint is for the last backend issues blocking profile sign-off before hosted publishing work  
**HEAD at prompt time**: `8010b45`  
**Supersedes**: the earlier hosted-identity publish-service prompt that jumped ahead of profile acceptance

---

## Sprint Context

The frontend profile surface is substantially built already. Backend should stay narrowly focused on the last issues that can still make the profile feel incomplete before the hosted web phase.

### Current relevant backend state

Already present:

- `save_profile_settings`
- `get_profile_settings`
- `save_profile_page`
- `get_profile_pages`
- `delete_profile_page`
- `delete_vault_item`
- `save_avatar_image`
- `export_profile_data`
- `import_mastodon_posts`

Hosted publish infrastructure is **not** ready yet:

- no `bytescape/server/` crate
- no `publish_identity`
- no `check_publish_status`
- no `set_publish_secret`

Do not spend this sprint building the hosted server or publish pipeline.

---

## Sprint Goal

Close the backend-side blockers that still prevent the profile from being considered locally finished and safely ready for the next hosted-publish phase.

---

## P0: Mastodon Posts Must Appear In Profile History

This is the top backend risk from the latest user feedback.

### Problem statement

The user reports:

- Mastodon connections are now showing
- Mastodon posts still do not appear on the user's profile post history page

Current evidence:

- `import_mastodon_posts` is wired into boot sync
- profile post history is owner-scoped through `list_vault_posts_paged`

### Required outcome

Trace the full path and fix the root cause if present:

1. Mastodon own posts are imported into the vault.
2. They are stamped / stored so the owner's profile queries can see them.
3. Source naming and filtering line up with the frontend filter chips.
4. The fix is verified against the actual profile history path, not just import counts.

### Strong suspicion to check first

The likely fault line is the interaction between:

- `import_mastodon_posts`
- shared post save/import code
- owner-scoped vault listing in `list_vault_posts_paged`

If the imported items are not associated with the same effective owner identity that the profile query uses, they can exist in the vault but still be invisible on the profile.

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/import.rs` | Verify Mastodon post stamping / save path |
| `src-tauri/src/commands/vault.rs` | Verify owner-scoped query behavior and source filtering |
| `src-tauri/src/lib.rs` | Only if command wiring or boot sync registration needs adjustment |

---

## P1: Support Full-Size Profile Media Viewing If Backend Is The Bottleneck

Frontend needs the profile Media section to open full-size images and playable videos.

### Required behavior

Audit whether the current backend commands already provide everything needed for:

- full-size image display
- video playback from the profile media surface
- stable blob/file resolution from the profile context

If an actual backend gap exists, fill it surgically by reusing or extending existing vault/media access patterns.

### Constraint

Do not create a second redundant media-read API if an existing command can be reused from the profile screen.

---

## P2: Export Parity With The Current Profile Model

`export_profile_data` exists, but the profile model has moved beyond the older prompt assumptions.

### Required outcome

Verify that export still reflects the current persisted profile surface well enough for the next hosted-publish sprint:

- modules
- custom pages
- avatar
- background/banner settings
- any current persisted profile-specific fields that matter for rendering

If the export shape is missing newly persisted fields, fix that now while the profile model is still fresh.

### Constraint

Do not invent backend state for features the frontend no longer treats as distinct persisted concepts.

---

## Out of Scope

- `bytescape/server/` crate
- public resolver endpoints
- `publish_identity`
- `check_publish_status`
- `set_publish_secret`
- full hosted identity rollout

Those remain the next phase after this profile gate closes.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape/src-tauri && cargo check
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
```

Also do a behavior-level check that imported Mastodon posts actually appear through the same profile-history path the frontend uses.

---

## Track Boundaries

- Track A owns `src-tauri/src/`.
- Coordinate with Track B on the exact visible profile-history failure, but fix the root cause in backend if the issue is import/ownership/query related.
- Do not jump ahead to hosted publish service work until this sprint is accepted.
