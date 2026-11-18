# FAQ

Answers for using Bytescape. For the live state of any feature, the honest
source is [STATUS.md](../STATUS.md).

## Accounts & sign-in

**How do I create an account?**
Sign-up is invite-gated: you need an invite code, then you sign in with
**Bluesky** (OAuth — we never see your password) or with your **fediverse
account** (Mastodon and other ActivityPub instances — rolling out with the
ActivityPub integration). Native email + password sign-up exists behind the
same invite gate. Invite codes are issued per host by the project.

**Can I use my existing Bluesky account?**
Yes — that's the primary path. Your Bytescape account *is* your AT Protocol
identity: sign in with Bluesky OAuth and your existing handle, posts, and
follows come with you.

**What's the difference between a Bluesky-linked and a Bytescape-hosted
account?**
A Bluesky-linked account keeps its identity on Bluesky's PDS (`did:plc`) —
Bytescape reads and writes there through your authorization. A
Bytescape-hosted account lives on our server's repository (`did:web`).
Either way your data is portable AT Protocol data — you can leave with it.

**Do I need an invite code if I already have an account?**
No — invite codes gate *new sign-ups*, not returning sign-ins.

**How do I delete my account?**
Deleting purges your Bytescape records from the AT repository (for
Bytescape-hosted accounts, the whole repository; Bluesky-linked accounts keep
standard Bluesky records) along with your app data.

## Import

**What can I import?**
Facebook, Instagram, and Threads GDPR exports — the archive you can request
from Meta. Posts, photos, and videos stream in with an origin badge on your
profile, and your imported contacts are matched against the network so you
can follow friends who are already here.

**What's verified today?**
Instagram: fully verified against real exports — photos, videos, feed videos,
Reels, and stories. Facebook and Threads: parsers cover the JSON and legacy
HTML formats, but verification against real exports is still in progress
(status updates in [STATUS.md](../STATUS.md)).

**Where does my imported data live?**
In the database of the instance you signed up at, tied to your account.
Storing archive content as portable protocol records is the documented
architecture direction ([Architecture](../architecture/README.md)).

## Your data & privacy

**Who owns my content?**
You do. It lives in an AT Protocol repository — portable, exportable, and
readable by other AT Protocol apps, not just Bytescape.

**Do you sell my data or run an engagement algorithm?**
No ads, no engagement-maximizing ranking. The feed is chronological by
default, and content carries provenance so you can see why it appeared.

**What happens when I delete something or leave?**
Deletes are real: they remove the record from the repository, not just a UI
flag, and cascade to comments and reactions in the index. Account deletion
purges your records as described above.

## Feed, stories & chat

**Why is my feed chronological?**
Design principle: chronological default = trust. Interest picks and topic
feeds give you control without a black-box ranking. Custom/user-defined
algorithms are the direction — see [VISION.md](../VISION.md).

**How do stories work?**
Media-first and ephemeral: a rail under the tab row, full-screen viewing,
reactions/replies, and a Vault archive of your own. Stories work across
Bytescape instances (followers on another server see them within ~10 minutes).

**Is chat private?**
Chat rides Bluesky's chat protocol — direct messages and group chats with
live updates and unread badges.

## Events

**How do events work?**
Events are first-class AT Protocol records (`community.lexicon.calendar.*`)
stored in your repository. Browse and RSVP in the main app, or use the
[standalone events site](events.md). Because the records are network-native,
events created on atmo.rsvp or OpenMeet show up here — and ours show up
there.

## Moderation

**Who moderates?**
Community-run moderation is the decided direction (product fork resolved
2026-10-10). The moderation layer — reports, labels, queues, appeals — is a
planned milestone in the vision sequence and is not shipped yet. Sensitive
content is hidden by default with an opt-in toggle meanwhile.

## What's coming

**ActivityPub integration (active build):** sign in with Mastodon, see
Mastodon/Pixelfed posts and stories in your feed, follow fediverse accounts —
feature-flagged so it ships without regressing the AT app. Then **Classifieds**
and **Groups**, gated by the ship gates in
[MVP scope](../product/mvp-scope.md). See
[Current work](../developers/current-work.md) for the sprint picture.
