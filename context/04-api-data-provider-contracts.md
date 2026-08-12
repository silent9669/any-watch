# API, data, and provider contracts

## API contract

The browser calls same-origin `/api`. Keep all external DTOs versioned and
camelCase. Errors provide a stable `code`, safe `message`, `operation`,
`retryable`, and `correlationId`.

Route groups:

| Group | Access |
| --- | --- |
| Health and readiness | Public, minimal, no provider secrets |
| Session | Login public; session inspection/logout authenticated |
| Admin accounts | Administrator only |
| Discovery, titles, providers | Authenticated |
| Playback grants and media routes | Authenticated, short-lived, source-scoped |
| History, My List, preferences | Authenticated and per-user |

Unsafe requests require origin validation plus the application request header.
Session tokens never enter local storage.

## Migration data contract

Import legacy users, roles, Argon2-compatible password hashes, favorites, and
watch history into PostgreSQL. Preserve `provider:id` library identity and
opaque episode IDs. Sessions do not migrate and are revoked at cutover.

Every importer is idempotent, reports counts/checksums, has an explicit rollback
consequence, and is rehearsed against a snapshot before production use.

## Provider contract

Each adapter implements or explicitly disables identity, region/language,
health, search, details, episodes, playback preparation/handoff, subtitles,
quality, required server-side headers, and safe failures.

Never assume metadata IDs, titles, and provider IDs are interchangeable. A
mapping is a confidence-scored suggestion, not permission to merge episode IDs
or silently change source.

Provider requests are bounded by timeout, concurrency, retry, circuit breaker,
and cache policy. Certification requires search-to-browser-viewing verification,
failure-path coverage, and redaction checks.
