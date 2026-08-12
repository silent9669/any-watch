# Capacity, cost, and evolution

## Principle

Keep the first deployment inexpensive, but never make cheapness depend on fragile shortcuts. Reliability comes from bounded work, caching, observable limits, restore practice, and a clear path to the next tier. The number of registered accounts is secondary to concurrent playback, bitrate, request fan-out, and storage retention.

## Workload model

Track these separately:

- signed-in users and active sessions;
- searches per minute by provider;
- AniList refreshes per hour and cache-hit ratio;
- simultaneous playback sessions by average and peak bitrate;
- media bytes proxied versus redirected/directly delivered;
- concurrent downloads and retained download bytes;
- database writes per second and write-lock time;
- provider error, timeout, and challenge rates.

As a bandwidth example, five simultaneous 8 Mbit/s streams require roughly 40 Mbit/s of sustained upstream capacity before protocol overhead. Ten 12 Mbit/s streams require roughly 120 Mbit/s. Measure real manifests and segments; do not size from labels such as 1080p.

## Service objectives

Starting objectives for a private service:

- authenticated API p95 below 500 ms when no provider call is required;
- cached discovery p95 below 300 ms;
- login/library availability at least 99.5% per month on powered-on homelab time;
- no cross-user data leakage and no stream URL/header leakage;
- readiness fails before traffic is accepted when the database is unavailable;
- provider failures are isolated and never trigger an unbounded retry storm;
- restore point objective of 24 hours and restore time objective of 60 minutes, verified quarterly.

Provider response time is an external dependency and must be graphed separately from local API latency.

## Repeatable capacity gate

Run three profiles against staging with production-like TLS and storage:

1. **Control plane:** sign-in, discovery cache hits, favorites, and progress updates without playback.
2. **Provider burst:** provider-first searches at expected peak plus a 2x safety factor, confirming request coalescing and per-provider limits.
3. **Media plane:** 1, 2, 4, 8, then 12 concurrent representative streams/downloads until a stop condition is reached.

Stop a stage when any of these persists for five minutes:

- playback buffering or segment error rate exceeds 1%;
- host CPU exceeds 80% or memory pressure causes swapping;
- disk latency p95 exceeds 25 ms for the state volume;
- outbound link exceeds 75% of measured stable throughput;
- API p95 doubles from the previous stage;
- provider challenge/429/5xx rate rises above the normal baseline.

Keep the production alert threshold one stage below the first failing stage. Save the test command, commit SHA, hardware, link speed, media bitrates, and results with every report.

## Cheapest safe growth path

### Tier 0 — any-watch single node

- One Go application, PostgreSQL, Valkey, Caddy, local persistent storage, and
  encrypted off-host backups.
- Cache metadata and coalesce provider lookups.
- Avoid transcoding. Use a bounded media path and do not make the VM a general
  third-party media relay.

### Tier 1 — larger single node and cleaner media path

- Increase bandwidth or move ingress closer to the better uplink before adding application replicas.
- Add storage quotas, download retention, connection limits, and per-user/per-provider rate limits.
- Put downloads in S3-compatible object storage only for content the operator is authorized to retain; use short-lived signed links.

### Tier 2 — larger or external durable state

- Increase PostgreSQL capacity or move it to a managed/private host only after
  backup, connection, and recovery needs justify it.
- Keep Valkey limited to expiring sessions, request coalescing, and rate counters.
- Keep provider adapters in one worker boundary until independent failure or
  scaling is demonstrated.

### Tier 3 — multiple application replicas

- Use a shared PostgreSQL database, shared ephemeral state, stateless application replicas, and health-aware routing.
- Move long provider resolutions/download preparation to a bounded job queue if request timeouts become operationally harmful.
- Keep media traffic out of the main API process where a dedicated proxy/object store is both authorized and cheaper.

### Tier 4 — service split

Split identity/library, provider resolution, and media delivery only when at least one is true:

- they need materially different scaling;
- failure isolation cannot be achieved inside one process;
- deployments are blocked by independent ownership/release needs;
- profiling shows a stable boundary with a measurable operational benefit.

## Replacement decision

The Nuxt/Go/PostgreSQL/Valkey replacement is approved. The remaining decisions
are evidence-based: provider ports, media limits, replica count, and the timing
of retiring legacy assets. Each requires migration, rollback, performance, and
operational evidence rather than framework preference.

## Cost ledger

Record monthly:

- electricity attributable to the node;
- domain, DNS, tunnel/VPN, and off-site backup fees;
- ISP/uplink changes;
- object storage and egress, if used;
- storage replacement and backup media amortization;
- operator time spent on provider repairs, upgrades, and incidents.

Alert on unexpected egress and storage growth. Provider maintenance time is likely to exceed compute cost, so adapter diagnostics and fixtures are a first-class cost control.
