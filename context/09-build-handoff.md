# any-watch build handoff

## Implementation order

1. Create the isolated Nuxt/Go/PostgreSQL/Valkey foundation beside legacy code.
2. Establish design tokens, application shell, login, and browser/TV accessibility
   harnesses.
3. Implement identity, administrator account management, and durable library
   routes.
4. Build and rehearse the SQLite-to-PostgreSQL importer.
5. Implement canonical titles and the versioned provider framework.
6. Port legacy providers one at a time with certification reports.
7. Build discovery, search, detail, player, progress, My List, and Settings.
8. Add observability, backups, staging, load tests, and Caddy cutover tooling.

## Required evidence per change

- Scope and affected product invariant.
- Tests run and their output.
- Provider certification or explicit non-enable decision when a source changes.
- Migration schema version, dry-run report, and rollback consequence for durable
  data changes.
- Responsive and remote-navigation screenshots or test results for UI work.
- Secrets/redaction review for network, playback, or provider changes.

## Handoff boundaries

- The current legacy code is an inventory and a behavioral reference, not the
  target architecture.
- No provider is enabled by default merely because legacy code exists.
- No migration deletes legacy code, data, or runbooks before side-by-side
  acceptance and rollback evidence exist.
- The owner approves production cutover separately from code completion.
