# any-watch migration acceptance

No deployment is authorized by this document. It defines the evidence required
before an owner-approved cutover.

## Build gates

1. Nuxt PWA, Go API, PostgreSQL migrations, Valkey, and Compose build together.
2. Authentication and user isolation pass integration tests.
3. SQLite import dry runs preserve expected users, library rows, and progress.
4. Every enabled provider has a current certification report.
5. Browser, accessibility, mobile, and TV navigation tests pass.
6. Backup/restore and Caddy rollback are rehearsed on staging.
7. Load testing documents the 100-concurrent-user browsing envelope and a safe
   media concurrency limit.

## Cutover gates

- A final verified legacy database snapshot exists.
- The importer report has expected counts/checksums and no unresolved conflicts.
- Owner verifies login, discovery, English/Vietnamese selection, source choice,
  episode selection, playback, progress, My List, Settings, and TV controls.
- Readiness verifies PostgreSQL, Valkey, and writable application storage.
- The old image, old Compose configuration, and data snapshot remain available.
- Caddy can switch back without requiring a destructive database operation.

## Definition of done

- No public registration, guest data, or cross-user data access.
- No raw source authorization data in browser payloads or logs.
- No silent provider merge or source fallback.
- Source and metadata outages degrade independently.
- Operational documentation, tested backups, monitoring, and rollback evidence
  match the deployed version.
