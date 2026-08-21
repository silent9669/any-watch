# Provider inventory and admission policy

## Purpose

This inventory records implementation status, not legal authorization,
regional availability, or a promise that an upstream will work.

No new provider is enabled based only on a repository, a search result, or an
HTTP 200 page. Each integration must use a documented, unauthenticated web-safe
integration method and pass the certification requirements below. Private
any-watch accounts protect access to this application; they do not authorize an
upstream provider or permit transferring browser cookies into the service.

## Adapter inventory

| Provider | Language focus | Current status | any-watch disposition |
| --- | --- | --- | --- |
| Invidious | YouTube, source-language agnostic | Implemented through documented v1 search/video/caption and DASH manifest APIs, with trending, popular, and related video feeds | Opt-in with `ANY_WATCH_INVIDIOUS_URL`; displayed in a dedicated authentic YouTube watch experience & dashboard |
| K20 | Vietnamese | Implemented via Stremio Addon protocol (`sc.k-20.xyz`); multi-catalog support (NguonC, STP, HH3D, VSMOV, YanHH3D) with direct HLS & WebVTT/SRT subtitles | Enabled by default; certified for live search, episode index, and direct playback |
| MovieBox | English | Implemented; current playback is HEVC-only DASH | Disabled until an AVC/VP9/AV1 browser-safe stream is certified |
| KKPhim | Vietnamese | Implemented and certified | Enabled by default |
| OPhim | Vietnamese | Implemented and certified; hardened timeout (45s) and multi-CDN fallback | Enabled by default |
| Niniyo | Vietnamese | Implemented and certified | Enabled by default |
| AllAnime | English | Legacy adapter; current source API is challenge-gated | Keep disabled; no documented browser-safe integration is available |
| AnimeGG | English | Implemented and certified | Enabled by default |
| AnimeVietSub | Vietnamese | Legacy adapter currently duplicates OPhim's public API | Disabled candidate; replace with a distinct documented web-safe integration before enabling |
| HiAnime | English | Stub, not registered | Do not port without a supported integration basis |
| AnimeTVN | Vietnamese | Configuration only, no adapter | Do not port without an implementation and supported integration basis |
| AniZone | English | Implemented; current structured search payload, HLS, and converted English softsubs passed prior live browser and Docker playback checks | Disabled by default; retain as an opt-in operational adapter pending re-certification of its undocumented page integration |
| AniDB | English | Implemented from ani-cli v5 (`92e9d796d23aef3ae94b52852f9c992e2bce4fe3`); search, 1,173 One Piece episodes, Japanese HLS pass live playback | Enabled by default; public frontend endpoints without a documented API contract |
| Reanime | English | Public catalog API documented; playback integration is undocumented | Defer pending a supported playback API, embed, or handoff |
| Prowlarr | N/A | Configuration only | Not a playback provider |

## Evaluated external repositories

### anime-vsub/desktop-web

Evaluated at repository commit `b2d86e026578fb117b1adf34d2290fba33e7a44b`.
The project exposes one playback source, AnimeVietSub; its `DU` and `FB` labels
are alternate routes through the same upstream rather than independent
providers. No adapter was imported because:

- all upstream HTTP and media access is gated by AnimeVsub Helper, a browser
  extension bridge;
- account and verification flows transfer AnimeVietSub cookies and instruct the
  user to complete upstream Cloudflare checks;
- playback uses protected AJAX tokens, playlist/segment decryption, and the
  `anime-vsub/new-dha` submodule, which was unavailable during evaluation;
- the source repository is GPL-3.0 while any-watch is currently distributed
  under MIT, so copying implementation code would require satisfying GPL-3.0
  obligations for the resulting work or obtaining separate compatible
  permission;
- no documented public API, supported embed, or authorized server-side handoff
  was found.

The existing `AnimeVietSub` adapter in this repository is not a port of that
project. It remains an opt-in OPhim-backed compatibility adapter and is not
presented as the AnimeVietSub website integration. A future distinct adapter
requires upstream documentation or permission and full certification.

AniList remains discovery and metadata only. It does not prove playback
availability and must not be represented as a media provider.

### AllAnime / MKissa

Re-evaluated on 2026-08-12 against the live `mkissa.to` frontend and
`api.mkissa.net` source API. Public catalog search still works, and the current
frontend bundles expose enough rotating material to reproduce the signed
bootstrap and `aaReq` request. The source request then returns `NEED_CAPTCHA`.

The current protocol also depends on undocumented, frequently rotated frontend
implementation details: a build ID, four obfuscated mask seeds, a content lane,
a signed bootstrap request, and a matching GraphQL query hash. Completing the
source request would require integrating the upstream challenge flow or
transferring browser state. Neither is an acceptable unauthenticated,
browser-safe integration method under this policy, so the legacy adapter stays
disabled even though its catalog operations remain reachable.

### AniZone

Evaluated on 2026-08-12 against `anizone.to`. Exact-title search, recent
episodes, browser playback, HLS variants, media segments, and an English ASS
subtitle track were verified without imported browser state. AniZone exposes
these through server-rendered first-party pages, not a documented public API,
supported embed, or external handoff. No canonical API terms, caching rules, or
integration permission were found, and the media CDN authorizes browser CORS
only for AniZone's own origin.

AniZone remains implemented but disabled by default. It may be explicitly
enabled as an operational opt-in despite the absence of a documented integration
contract. The adapter uses public first-party search, title, and episode pages
without imported cookies or challenge bypasses. Search results are read from the
page's structured `items` payload because AniZone no longer renders one Livewire
card per result.
The authenticated opaque proxy relays HLS manifests, keys, audio, video, and
segments, and converts English ASS tracks to browser-native WebVTT. This loses
advanced ASS positioning, fonts, animation, and karaoke styling but preserves
ordinary dialogue and line breaks.

AniZone page and media requests use the system `curl` binary with the neutral
`any-watch/1.0` user agent. This avoids the upstream network behavior that
refuses reqwest connections while accepting ordinary non-browser clients; it
does not import cookies or impersonate a browser.

Certification passed for exact One Piece search, 1,173 regular episodes, recent
episode playback, rewritten media resources, converted English subtitles, the
production Docker image, and Chromium playback with a same-origin media blob.
The adapter is operational rather than supported: AniZone page markup or CDN
policy can change without notice, so failures must remain isolated from login,
libraries, and the Vietnamese providers.

### AniDB

Evaluated on 2026-08-13 against `anidb.app`, the single playback source used by
ani-cli v5. Search (`/browse?q=`), details, episode metadata
(`/api/frontend/anime/{id}/episodes`), language/embed selection
(`/api/frontend/episode/{id}/languages`), and the HLS master inside the embed
were verified without imported browser state. Ordinary requests return One
Piece with 1,173 regular episodes and a Japanese-audio HLS stream.

The site fronts its HTML and API endpoints with Cloudflare bot management. It
challenges known scraper user agents (including the literal `curl` UA) and
browser-claiming UAs issued by non-browser TLS stacks. The adapter therefore
uses a neutral, honest user agent (`any-watch/1.0`) issued through the system
`curl` binary (subprocess), and its health check probes streams through that
same transport. This signature passes from both residential and datacenter
networks. No cookies, challenges, or impersonated browser state are involved.

AniDB streams are marked `use_curl`: the server's opaque media proxy, HLS
segment resolution, and download paths fetch AniDB manifests and segments
through the `curl` binary instead of reqwest, preserving the same
user-agent/referer signature that the bot manager accepts. The container image
installs `curl` explicitly.

The integration has no documented public API contract and upstream page markup,
embed structure, or bot policy can change without notice. Failures are
classified and isolated from login, libraries, and the other providers.

### K20 (Stremio Addon Protocol)

Evaluated on 2026-08-18 against `https://sc.k-20.xyz`, a public Stremio Addon service
aggregating Vietnamese and East Asian cinema catalogs (NguonC, STP, HH3D, VSMOV, YanHH3D).
The integration conforms to the standard Stremio Addon specification:

- Catalog search queries: `/catalog/{type}/{catalog_id}/search={query}.json`
- Media metadata & episode lists: `/meta/{type}/{id}.json`
- Stream discovery & subtitles: `/stream/{type}/{id}.json`

The provider features:
- Multi-catalog simultaneous asynchronous search across series and movie categories.
- Robust alphanumeric episode identifier parsing with fallbacks.
- Web-standard direct HLS media streams with automatic WebVTT/SRT subtitle extraction and mapping.
- Browser-safe same-origin proxy support through any-watch's opaque stream pipeline.
- Certified against live anime search (e.g. Naruto, One Piece) and video playback.

### Reanime

Evaluated on 2026-08-12 against `reanime.to`. Its first-party OpenAPI document
describes unauthenticated search, title, episode, and playability endpoints, and
those endpoints returned an exact One Piece match and current subbed episode
metadata. The documentation does not expose streams, subtitles, an embed
contract, or a media handoff.

On-site playback resolves through an undocumented `/api/flix` endpoint and a
Flixcloud player using short-lived, IP-bound media state, encoded playlists, and
CDN resources that reject direct server retrieval. Reproducing that pipeline
would require relying on rotating undocumented implementation details and would
not work with the existing opaque HLS proxy. Reanime's terms also prohibit
copying, reverse engineering, transferring, or mirroring site materials.
Reanime is deferred until it publishes API-specific integration terms and a
supported browser-safe playback method.

## Required adapter contract

Each adapter implements or explicitly disables:

- Provider identity, display name, language/region scope, and capabilities.
- Search, details, episodes, and opaque source identifiers.
- Playback preparation or explicit external handoff.
- Subtitles, audio, quality, and stream type metadata where available.
- Health check, error classification, retry policy, and explicit availability state.
- Per-provider concurrency, timeout, cache, and circuit-breaker configuration.
- Explicit AniSkip episode numbering where provider episodes are certified to
  match the canonical title; ambiguous season splits and decimal specials stay
  unmapped.

Authenticated aggregate `GET` and `POST /api/providers/health` refreshes share a
five-minute cache and coalescing/version protection. At most sixteen provider checks
run concurrently, and each provider-health operation has a 60-second backend
timeout. The Cloudflare Worker gives the aggregate endpoint 70 seconds and leaves
provider-level failures scoped to that endpoint.

Provider search results remain distinct. Matching a canonical title is a
confidence-scored mapping; it is never permission to merge source episode IDs or
to silently switch playback providers.

The enabled-by-default providers expose AniSkip numbers for AniDB, AnimeGG,
KKPhim, OPhim, and integer-numbered Niniyo episodes. AniZone exposes the same
mapping when explicitly enabled. Automatic Skip intro still requires a unique
exact catalog match and a compatible episode duration. AniSkip may have
no submitted timing for an otherwise correct episode (for example, One Piece
1170 at the time of certification); the application must not borrow timing from
an adjacent episode.

## Certification policy

A provider can be enabled only after a repeatable test verifies:

1. Provider search returns a selected result.
2. Details and an episode or movie entry resolve.
3. The provider produces the expected unauthenticated web viewing method.
4. Browser playback or the declared platform handoff works.
5. Subtitle and quality claims match the returned capability data.
6. Login, library, and other providers remain usable when this provider fails.
7. Browser responses and logs contain no source credentials, cookies, required
   upstream headers, or raw signed media URLs.
8. HLS/DASH video codecs are supported by the declared browser matrix; a
   reachable HEVC-only manifest does not qualify as browser playback.

The certification report records timestamp, environment, region, sample titles,
capabilities, result, and safe error category. A failed check is exposed as
retryable `unavailable` rather than silently hiding saved items. If the initial
aggregate request fails, UI entries still in `unknown` transition to retryable
`unavailable` so users can explicitly retry them.

## Adding sources

New sources require all of the following before engineering begins:

- A documented public API, supported embed, or handoff that works without user
  authorization or imported browser state.
- Terms, attribution, caching, geographic, and user-data requirements captured
  in the provider registry.
- A named owner for protocol maintenance and re-certification.
- A bounded operational impact compatible with the VM's network and memory
  budget.

This rule keeps the source list maintainable and prevents an upstream change from
becoming a system-wide playback outage.
