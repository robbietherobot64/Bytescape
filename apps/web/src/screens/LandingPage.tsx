import React from 'react';
import demoDesktopShot from '../assets/landing-demo-1280.png';
import demoMobileShot from '../assets/landing-demo-390.png';

// Public landing (landing spec): ONE honest screen for logged-out visitors.
// No analytics, no email capture, no popups. Every claim below is the wiki's
// own (bytescape.social/wiki) — the only network-size figure on the page is
// the wiki's "Bluesky's 40+ million", attributed inline.
//
// Demo screenshots are real captures of the app taken on the test instance
// against a synthetic demo account (qa-shots/landing-*-demo.png are the
// verification captures of this very page). Never reuse qa-shots captures of
// real accounts here.

interface Props {
  /** Enter the existing auth flow (signup with invite field / sign in). */
  onJoin: () => void;
}

const WIKI_HOME = 'https://bytescape.social/wiki/';
// Self-host door: the wiki's project guide ("where everything lives", local
// setup) — the working docs page today.
const WIKI_SELF_HOST = 'https://bytescape.social/wiki/#/docs/README';
// Contribute door: the wiki's Contributing page (the wiki SPA routes deep
// links as #/path — a path-style URL always lands on the front page). The
// public repo (https://github.com/robbietherobot64/Bytescape) exists but has
// no content pushed yet.
// SWAP POINT: once README/CONTRIBUTING land in the repo, change this href to
// https://github.com/robbietherobot64/Bytescape
const WIKI_DEVELOPERS = 'https://bytescape.social/wiki/#/developers/contributing';

const PITCH = [
  {
    title: 'IMPORT YOUR HISTORY',
    body: 'Upload a Facebook, Instagram, or Threads GDPR archive: your posts and photos come with you, and your contacts are matched against the network — your friend graph, rebuilt, with one-click follows.',
  },
  {
    title: 'ONE ACCOUNT, TWO PROTOCOLS',
    body: "Your presence is native AT Protocol and carries an ActivityPub identity too — one account that plugs into Bluesky's 40+ million–person network (source: the project wiki) and, as federation rolls out, the fediverse.",
  },
  {
    title: 'YOUR DATA IS YOURS',
    body: 'Your content lives in a portable, exportable AT Protocol repository — take it and leave whenever you like. The whole stack is open source and self-hostable: one server binary, one frontend, one database.',
  },
];

const DOORS = [
  {
    label: 'JOIN',
    blurb: 'Create your account with an invite code — email, Bluesky, or fediverse.',
    action: 'open-flow' as const,
  },
  {
    label: 'SELF-HOST',
    blurb: 'Run the whole platform yourself — the wiki covers the stack and the steps.',
    href: WIKI_SELF_HOST,
  },
  {
    label: 'CONTRIBUTE',
    blurb: 'See how the project is built and where the code lives.',
    href: WIKI_DEVELOPERS,
  },
];

export default function LandingPage({ onJoin }: Props) {
  return (
    <div className="fullscreen landing app-theme-default">
      <div className="landing-inner">
        <header className="landing-top">
          <span className="landing-wordmark">BYTESCAPE</span>
          <button type="button" className="btn landing-signin" onClick={onJoin}>
            Sign in
          </button>
        </header>

        <section className="landing-hero">
          <h1 className="landing-headline">
            Import your history.
            <br />
            Find your friends.
            <br />
            Own your presence.
          </h1>
          <p className="landing-subline">
            Bytescape is an open social platform on AT Protocol for people leaving
            Facebook and Instagram, with ActivityPub federation to the fediverse —
            the deep dive is at <a href={WIKI_HOME}>bytescape.social/wiki</a>.
          </p>
        </section>

        <section className="landing-pitch">
          {PITCH.map((block) => (
            <div key={block.title} className="landing-pitch-block">
              <h2>{block.title}</h2>
              <p>{block.body}</p>
            </div>
          ))}
        </section>

        <section className="landing-demo">
          <h2 className="landing-section-title">THIS IS THE APP</h2>
          <div className="landing-demo-shots">
            <img
              src={demoDesktopShot}
              alt="Bytescape profile screen on desktop: a demo account's public posts in the terminal-styled app"
              className="landing-shot landing-shot-desktop"
            />
            <img
              src={demoMobileShot}
              alt="Bytescape profile screen on a phone: the same demo account's public posts"
              className="landing-shot landing-shot-mobile"
            />
          </div>
          <p className="landing-demo-caption">
            Real screenshots, no stock art — the demo account on the live app.
          </p>
        </section>

        <section className="landing-doors">
          {DOORS.map((door) =>
            door.action === 'open-flow' ? (
              <button
                key={door.label}
                type="button"
                className="btn landing-door landing-door-primary"
                onClick={onJoin}
              >
                <span className="landing-door-label">{door.label}</span>
                <span className="landing-door-blurb">{door.blurb}</span>
              </button>
            ) : (
              <a key={door.label} className="landing-door" href={door.href}>
                <span className="landing-door-label">{door.label}</span>
                <span className="landing-door-blurb">{door.blurb}</span>
              </a>
            ),
          )}
        </section>

        <footer className="landing-foot">
          Invite-gated while the platform grows · No tracking, no ads, no
          engagement algorithm · AGPL-3.0 ·{' '}
          <a href={WIKI_HOME}>bytescape.social/wiki</a>
        </footer>
      </div>
    </div>
  );
}