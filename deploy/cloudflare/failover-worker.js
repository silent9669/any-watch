const MAINTENANCE_ORIGIN = "https://silent9669.github.io";
const MAINTENANCE_PREFIX = "/any-watch";
const ORIGIN_TIMEOUT_MS = 4_000;
const PROVIDER_HEALTH_TIMEOUT_MS = 70_000;

export class OutageState {
  constructor(state) {
    this.storage = state.storage;
  }

  async fetch(request) {
    const observation = await request.json();
    const detectedAtIso = await this.storage.transaction(async (transaction) => {
      const current = await transaction.get("detectedAtIso");
      if (observation.mode === "online") {
        if (current) await transaction.delete("detectedAtIso");
        return null;
      }
      if (current) return current;
      await transaction.put("detectedAtIso", observation.observedAtIso);
      return observation.observedAtIso;
    });
    return Response.json({ detectedAtIso });
  }
}

export default {
  async fetch(request, env, context) {
    const url = new URL(request.url);
    if (url.pathname === "/status.json") {
      return statusResponse(request, env);
    }

    const providerHealth = url.pathname === "/api/providers/health";
    try {
      const originResponse = await fetch(request, {
        cache: "no-store",
        redirect: "manual",
        signal: AbortSignal.timeout(providerHealth ? PROVIDER_HEALTH_TIMEOUT_MS : ORIGIN_TIMEOUT_MS),
      });

      // A provider check reports provider failures itself. Never turn that
      // endpoint's response into a whole-site outage.
      if (providerHealth || originResponse.status < 500) {
        context?.waitUntil(recordOutage(env, url.hostname, "online"));
        return withMode(originResponse, "app", true);
      }
    } catch {
      // Network failure and timeout both select the independent fallback.
    }

    const detectedAtIso = await recordOutage(env, url.hostname, "maintenance");
    return maintenanceResponse(request, detectedAtIso);
  },
};

async function statusResponse(request, env) {
  const healthUrl = new URL("/api/health", request.url);
  try {
    const response = await fetch(healthUrl, {
      cache: "no-store",
      redirect: "manual",
      signal: AbortSignal.timeout(ORIGIN_TIMEOUT_MS),
    });
    if (response.ok) {
      await recordOutage(env, healthUrl.hostname, "online");
      return jsonStatus("online", null);
    }
  } catch {
    // The status response below records the first observed outage time.
  }

  const detectedAtIso = await recordOutage(env, healthUrl.hostname, "maintenance");
  return jsonStatus("maintenance", detectedAtIso);
}

function jsonStatus(mode, detectedAtIso) {
  const online = mode === "online";
  const headers = new Headers({
    "cache-control": "no-store",
    "content-type": "application/json; charset=utf-8",
    "x-any-watch-mode": mode,
  });
  return new Response(JSON.stringify({
    service: "any-watch",
    mode,
    headline: online ? "The theatre is online." : "The theatre is taking a short break.",
    message: online
      ? "any-watch is online and ready for your next screening."
      : "any-watch is temporarily offline for maintenance. Your watch history, family accounts, and library remain stored safely on the home server.",
    statusLabel: online ? "The theatre is online" : "Maintenance in progress",
    expectedReturn: online ? "Available now" : "Shortly",
    detectedAtIso,
    checkedAtIso: new Date().toISOString(),
    privacy: "Account data stays on your home server.",
  }), { status: 200, headers });
}

async function maintenanceResponse(request, detectedAtIso) {
  const url = new URL(request.url);
  if ((request.method !== "GET" && request.method !== "HEAD") || url.pathname.startsWith("/api/")) {
    const headers = new Headers({
      "cache-control": "no-store",
      "content-type": "application/json; charset=utf-8",
      "retry-after": "60",
      "x-any-watch-mode": "maintenance",
    });
    return new Response(JSON.stringify({
      code: "SERVICE_UNAVAILABLE",
      message: "any-watch is temporarily offline for maintenance.",
      retryable: true,
      detectedAtIso,
    }), { status: 503, headers });
  }

  const acceptsHtml = request.headers.get("accept")?.includes("text/html");
  const isNavigation = request.headers.get("sec-fetch-mode") === "navigate" || acceptsHtml;
  const fallbackPath = isNavigation
    ? `${MAINTENANCE_PREFIX}/`
    : `${MAINTENANCE_PREFIX}${url.pathname}`;
  const fallbackUrl = new URL(fallbackPath, MAINTENANCE_ORIGIN);
  fallbackUrl.search = url.search;
  const fallbackResponse = await fetch(fallbackUrl, {
    method: request.method,
    headers: { accept: request.headers.get("accept") || "*/*" },
    redirect: "follow",
    cf: { cacheTtl: 300 },
  });

  if (!fallbackResponse.ok && isNavigation) {
    const headers = new Headers({
      "cache-control": "no-store",
      "content-type": "text/plain; charset=utf-8",
      "retry-after": "60",
      "x-any-watch-mode": "maintenance",
    });
    return new Response("Maintenance page is temporarily unavailable.", { status: 503, headers });
  }

  const response = withMode(
    fallbackResponse,
    "maintenance",
    url.pathname === "/" || isNavigation,
  );
  return response;
}

async function recordOutage(env, hostname, mode) {
  const observedAtIso = new Date().toISOString();
  if (!env?.OUTAGE_STATE) return mode === "online" ? null : observedAtIso;
  const id = env.OUTAGE_STATE.idFromName(hostname);
  const response = await env.OUTAGE_STATE.get(id).fetch("https://outage-state.internal/", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ mode, observedAtIso }),
  });
  const result = await response.json();
  return result.detectedAtIso ?? null;
}

function withMode(response, mode, noStore = false) {
  const headers = new Headers(response.headers);
  headers.set("x-any-watch-mode", mode);
  if (noStore) headers.set("cache-control", "no-store");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}
