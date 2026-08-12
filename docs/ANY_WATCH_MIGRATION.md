# any-watch migration plan

## Goal

Replace the legacy `ani-web` application with `any-watch`: a private,
account-only, web-first viewing application for family members on desktop,
mobile, and TV browsers. The target stack is Nuxt 4, Go, PostgreSQL, Valkey,
Caddy, and Cloudflare.

This plan does not authorize a provider. Existing source adapters are preserved
as migration inventory and must pass the provider admission policy before they
are enabled in the replacement. Private app accounts are independent from
provider access: ports must not require upstream account credentials, imported
browser cookies, or manual challenge completion.

## Fixed product decisions

- Login is the only public route; administrators create all accounts.
- There is no guest mode and no guest local-library migration.
- Account history, My List, progress, and preferences are server-side.
- English and Vietnamese are first-class UI, metadata, subtitle, and source
  preference dimensions.
- The app is an installable PWA and supports remote navigation in TV browsers.
- Administrators manage accounts from a protected Settings section, not a public
  dashboard.
- The existing `web.db` users, roles, password hashes, favorites, and history
  are migrated. Existing sessions are revoked during cutover.

## Provider migration

The Go provider framework will use a source-scoped adapter contract. Legacy
providers are ported one at a time, preserving source and episode IDs, search
semantics, language labels, subtitles, quality metadata, and safe failures.

For each candidate:

1. Document a public, web-safe API, embed, or handoff and its operating constraints.
2. Port the adapter without upstream credentials, imported cookies, or exposing
   signed URLs or headers.
3. Add deterministic parser/contract tests.
4. Certify search, details, episodes, playback preparation, subtitles, and
   browser playback independently.
5. Record the result, date, region, and failure mode in the provider registry.
6. Enable only after the certification passes; otherwise expose `Limited`,
   `Verify`, or `Offline` without deleting user library data.

See [PROVIDER_INVENTORY.md](PROVIDER_INVENTORY.md) for the current inventory.

## Phases

### 1. Protect the current service

- Snapshot the production SQLite databases, active image, Caddy state, and
  current detached-worktree changes.
- Test restoration before any schema work.
- Keep the current Compose stack untouched and serving traffic.

### 2. Build the replacement foundation

- Create the Nuxt PWA and Go modules in new directories without deleting legacy
  code.
- Define versioned OpenAPI contracts and typed client generation.
- Add PostgreSQL migrations, Valkey namespaces, structured logs, correlation
  IDs, health, readiness, metrics, and audit events.
- Implement the `any-watch` design system before product screens.

### 3. Implement identity and account data

- Add admin-created accounts, Argon2id verification, revocable secure sessions,
  CSRF protection, and Valkey-backed login rate limits.
- Build protected Settings account management.
- Write an idempotent SQLite-to-PostgreSQL importer.
- Preserve user IDs where possible; otherwise create an explicit old-to-new ID
  map. Validate counts and content hashes before cutover.

### 4. Port catalog and provider contracts

- Add canonical title, external-ID, offering, and episode models.
- Port discovery and cache behavior with upstream rate-limit protection.
- Port legacy providers according to the admission workflow above.
- Add approved embeds, handoffs, or operator-authorized media as separate
  provider modes rather than pretending they are direct media sources.

### 5. Deliver the viewing experience

- Build Login, Home, Search, Detail, Player, Continue Watching, My List, and
  Settings routes.
- Add HLS/DASH/native playback only for authorized media and official iframe
  player integration where supported.
- Add source health, explicit fallback choices, quality/subtitle controls,
  AniSkip-compatible ranges, and TV focus navigation.

### 6. Validate and deploy side by side

- Run API, migration, provider-contract, accessibility, browser, and TV tests.
- Run load tests against the 100-concurrent-user browsing target.
- Deploy replacement services on private ports and a temporary hostname.
- Run a final SQLite snapshot/import and acceptance validation.
- Switch Caddy only after explicit acceptance. Retain the old image and database
  snapshots for rollback.

## Acceptance gates

- No account data crosses user boundaries.
- Every imported account can log in with its existing password or has a planned
  administrator reset path.
- History and My List import row counts match the signed migration report.
- Each enabled provider passes current certification in the intended region.
- Login, account library, and settings work when metadata or one provider fails.
- Provider authorization material is never required or accepted, and provider
  protocol details never reach browser responses or logs.
- PWA works at 320, 375, 414, 768, 1100, 1440, 1728, and TV viewport widths.
- Rollback restores the prior application and verified data snapshot.

## Non-goals for this phase

- A public anonymous catalog or guest accounts.
- Rewriting the current application in place.
- Claiming availability or playback support before source certification.
- Treating Cloudflare as a substitute for a media CDN or authorization from a
  source owner.
