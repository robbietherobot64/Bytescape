# What Bytescape is

Bytescape is an open social home for people leaving Facebook and Instagram —
and for Bluesky users who want richer profiles and more control over their
feed. It runs on **AT Protocol**: your content lives in your own PDS
repository, portable and federated with the wider network (Bluesky included).
ActivityPub integration — sign in with Mastodon, see Pixelfed and Mastodon
content — is rolling out now.

Three principles shape everything: **chronological by default** (trust over
tuning), **transparency about why content appears** (no engagement-maximizing
algorithm), and **utility over habit** (built to be useful, not addictive).

## What works today

- **Sign in with Bluesky** — bring your existing identity; OAuth means your
  password never touches our servers. Fediverse sign-in is arriving with the
  ActivityPub integration — see the [FAQ](faq.md).
- **Home feed** — posts from people you follow, your topic feeds, hashtags you
  subscribed to, and discovery, in chronological order.
- **Topic feeds done right** — about 113 real AT Protocol feeds (punk, hockey,
  photography, cats, …) that anyone on any Bluesky app can look up and follow.
  Combine topics into your own custom feed and share it.
- **Interactions** — likes, emoji reactions, reposts/shares, comments, and
  follow/unfollow, with a hover card on shared posts.
- **Import your history** — upload a Facebook, Instagram, or Threads GDPR
  export: posts, photos, and videos come in with a "📦 archive" origin badge,
  and imported contacts are matched against the network so you can follow
  friends in one click. Instagram is fully verified against real exports;
  Facebook and Threads are still being verified ([FAQ](faq.md)).
- **Customizable profiles** — drag-and-drop widget grid, preset themes (1999,
  XP, Terminal), per-widget styling, music embeds, photo galleries.
- **Stories** — media-first ephemeral stories: rail, full-screen viewer,
  composer, and a Vault archive. Works across Bytescape instances.
- **Chat** — direct messages and group chats over Bluesky's chat protocol,
  with live updates, unread badges, and groups that keep their own identity.
- **Events** — browse, create, and RSVP to events, plus a
  [standalone events site](events.md) whose records live on the AT Protocol
  network — interoperable with other event apps like atmo.rsvp and OpenMeet.
- **Vault** — a media browser of the images from your own posts.
- **NSFW hidden by default** — opt-in toggle in the profile toolbar.

## What's next

The **ActivityPub integration** is the active build: sign in with your
fediverse account, see Mastodon and Pixelfed content in your feed, and follow
fediverse accounts — all feature-flagged and additive, so the shipped app
never regresses. After that come the shelf-gated sections: **Classifieds**
and **Groups**.

- Current phase and next actions: [STATUS.md](../STATUS.md)
- Ship gates for everything above: [MVP scope](../product/mvp-scope.md)
- Active sprints right now: [Current work](../developers/current-work.md)

## Design principles

Chronological default = trust. Transparency about *why* content appears.
Legibility over cleverness. User-defined algorithms, not corporate ones.
Utility over habit. Community over scale. The full vision and success ladder:
[VISION.md](../VISION.md).
