# Provider inventory and migration policy

## Purpose

This is the migration inventory for source adapters already present in the
legacy codebase. It records implementation status, not legal authorization,
regional availability, or a promise that an upstream will work.

No new provider is enabled based only on a repository, a search result, or an
HTTP 200 page. Each integration must use a documented, unauthenticated web-safe
integration method and pass the certification requirements below. Private
any-watch accounts protect access to this application; they do not authorize an
upstream provider or permit transferring browser cookies into the service.

## Legacy adapter inventory

| Provider | Language focus | Legacy status | any-watch disposition |
| --- | --- | --- | --- |
| MovieBox | English | Default registered adapter | Port after re-certification |
| KKPhim | Vietnamese | Default registered adapter | Port after re-certification |
| OPhim | Vietnamese | Default registered adapter | Port after re-certification |
| Niniyo | Vietnamese | Default registered adapter | Port after re-certification |
| AllAnime | English | Implemented, default disabled, verification-sensitive | Keep as disabled candidate pending review and certification |
| AnimeGG | English | Implemented, default disabled | Keep as disabled candidate pending review and certification |
| AnimeVietSub | Vietnamese | Legacy adapter currently duplicates OPhim's public API | Disabled candidate; replace with a distinct documented web-safe integration before enabling |
| HiAnime | English | Stub, not registered | Do not port without a supported integration basis |
| AnimeTVN | Vietnamese | Configuration only, no adapter | Do not port without an implementation and supported integration basis |
| Prowlarr | N/A | Configuration only | Not a playback provider |

AniList remains discovery and metadata only. It does not prove playback
availability and must not be represented as a media provider.

## Required adapter contract

Each port implements or explicitly disables:

- Provider identity, display name, language/region scope, and capabilities.
- Search, details, episodes, and opaque source identifiers.
- Playback preparation or explicit external handoff.
- Subtitles, audio, quality, and stream type metadata where available.
- Health check, error classification, retry policy, and explicit availability state.
- Per-provider concurrency, timeout, cache, and circuit-breaker configuration.

Provider search results remain distinct. Matching a canonical title is a
confidence-scored mapping; it is never permission to merge source episode IDs or
to silently switch playback providers.

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

The certification report records timestamp, environment, region, sample titles,
capabilities, result, and safe error category. A provider that fails is marked
`Limited`, `Verify`, or `Offline`; it is not silently hidden from existing saved
items.

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
