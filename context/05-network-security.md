# any-watch network and security baseline

## Trust boundaries

1. The browser is untrusted, even for family members.
2. Caddy is the only public listener.
3. The application network is private to Compose.
4. AniList and every playback provider are untrusted outbound services.
5. The Docker host, persistent data directory, configuration file, and backups are privileged assets.

## Minimum controls

- Expose only TCP 80/443 and UDP 443 when public HTTPS is used. Restrict SSH to the management LAN or overlay network.
- Keep application port 3000 unpublished.
- Use a unique administrator password of at least 16 characters and separate named user accounts.
- Store production environment values in an owner-readable `any-watch` config
  directory with product-neutral file names.
- Run all application containers as non-root users and apply Compose resource,
  PID, and log-retention limits.
- Enable automatic OS security updates and keep Docker/Caddy patched.
- Keep HTTPS-only cookies and HSTS only when the selected hostname is always reachable over HTTPS.
- Rate-limit login at Caddy as well as the application if the service is public; application memory limits reset on restart.
- Add server-side Origin/Host validation for unsafe requests.
- Set request-body, header, connection, and upstream timeouts. Media routes need streaming-specific timeouts rather than ordinary API limits.
- Never proxy arbitrary caller-supplied URLs. Media resource URLs must be created from an authenticated server-side session and restricted to expected schemes/hosts.

## DNS and TLS

For public exposure, point a dedicated hostname to the home public IP. Use the existing Namecheap DDNS timer if applicable. Forward ports 80 and 443 to the VM; Caddy obtains and renews certificates. Do not expose the hypervisor management UI or Docker socket.

For overlay-only exposure, use split DNS or the overlay hostname and ensure certificate behavior matches the chosen DNS model. Remove public router forwards if they are not required.

## Secrets

Secrets include admin password, DDNS token, provider verification cookies, session cookies, and any tunnel credentials. They must not enter Git, images, CI artifacts, shell history, screenshots, or logs.

Recommended permissions:

```sh
sudo install -o root -g docker -m 0640 any-watch.env /srv/any-watch/config/any-watch.env
sudo install -o root -g root -m 0600 any-watch-ddns.env /etc/any-watch-ddns.env
```

If a secret is ever committed or pasted into a public system, rotate it; deleting the file from the latest commit is not sufficient.

## Backups

During coexistence, back up stopped or transactionally snapshotted SQLite data.
For any-watch, use PostgreSQL `pg_dump` custom-format backups, encrypted off-host
copies, retention, automated verification, and restore drills. Keep at least one
backup outside the VM.

Downloads are replaceable and large; back them up only if the family values offline retention more than storage cost. Databases and configuration are the primary recovery set.

## Logging and privacy

Log timestamp, level, route template, status, latency, provider name, safe error code, and correlation ID. Avoid usernames unless operationally necessary. Never log passwords, cookies, authorization headers, provider headers, signed URLs, media query strings, or complete request/response bodies.

Retain application and access logs for a small bounded period. Provider breakage is common; logs should distinguish upstream failure from local authentication, database, DNS, TLS, and disk failures.

## Security review before public access

- Verify there is no public registration route.
- Verify disabled users lose session access.
- Verify users cannot read or mutate another user's history/favorites/downloads.
- Verify admin self-lockout/protected-admin safeguards.
- Verify CSRF/Origin checks and cookie flags through the real domain.
- Verify media signatures expire and cannot fetch arbitrary hosts.
- Verify login throttling at both application and edge.
- Scan the Git history and built image for secrets.
- Test backup restore on a clean VM.
