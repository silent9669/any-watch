# any-watch product brief

## Audience

`any-watch` is a private family viewing app for administrator-created accounts.
It is web-first and must work on desktop, mobile, and TV browsers. The public
hostname exposes only a login screen; there is no self-registration or guest
mode.

## Required journeys

- Sign in with an administrator-created account.
- Resume a title, browse discovery, or search for a canonical title.
- Inspect explicit source-specific viewing options, choose one, select an
  episode, and start playback or a declared handoff.
- Keep a query while changing source/language filters.
- Manage server-side My List and Continue Watching across signed-in browsers.
- Change subtitles, quality, playback, language, accessibility, and TV settings.
- Let an administrator manage family accounts within protected Settings.

## Product rules

- The title page is canonical; providers and episodes remain source-scoped.
- AniList is metadata/discovery only, never proof of playback.
- English and Vietnamese remain independent language dimensions and test paths.
- Source state is explicit: Healthy, Limited, Verify, or Offline.
- Provider failure never blocks login, settings, My List, or history.
- An enabled provider must have current certification; a title never implies a
  source can play it.

## Quality priorities

1. Account privacy and safe server-side media authorization.
2. Reliable resume and clear source failure behavior.
3. Original, premium, accessible browser and TV experience.
4. Reversible deployment, backup, monitoring, and low operator load.
5. A scalable path based on measured browsing and media demand.

## Non-goals

- Public signup, social features, billing, or advertising.
- A public anonymous catalog or local guest library.
- In-place replacement of the live legacy service.
- Treating an arbitrary third-party webpage as a certified provider.
- Transcoding or persistent mirroring unless the operator is authorized and has
  separately sized the storage and bandwidth.
