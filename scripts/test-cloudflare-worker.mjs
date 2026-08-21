import assert from "node:assert/strict";
import worker, { OutageState, originTimeoutForPath } from "../deploy/cloudflare/failover-worker.js";

const originalFetch = globalThis.fetch;

function outageEnvironment() {
  const values = new Map();
  const storage = {
    transaction: async (callback) => callback({
      get: async (key) => values.get(key),
      put: async (key, value) => values.set(key, value),
      delete: async (key) => values.delete(key),
    }),
  };
  return {
    OUTAGE_STATE: {
      idFromName: (name) => name,
      get: () => ({ fetch: (url, init) => new OutageState({ storage }).fetch(new Request(url, init)) }),
    },
  };
}

try {
  const env = outageEnvironment();
  const context = { waitUntil: (promise) => promise };
  assert.equal(originTimeoutForPath("/api/health"), 4_000);
  assert.equal(originTimeoutForPath("/api/youtube/trending"), 65_000);
  assert.equal(originTimeoutForPath("/api/media/session-1"), 65_000);
  assert.equal(originTimeoutForPath("/api/providers/health"), 70_000);
  globalThis.fetch = async (_request, init) => {
    assert.equal(init.cache, "no-store");
    return new Response("app", { status: 200 });
  };
  const online = await worker.fetch(new Request("https://ani.dangphuc.me/"), env, context);
  assert.equal(online.status, 200);
  assert.equal(online.headers.get("x-any-watch-mode"), "app");
  assert.equal(online.headers.get("cache-control"), "no-store");
  assert.equal(await online.text(), "app");

  // Provider and media APIs report typed failures themselves. A received 5xx
  // must never become a whole-site outage or generic maintenance response.
  globalThis.fetch = async () => new Response(JSON.stringify({ code: "PROVIDER_UNAVAILABLE" }), {
    status: 502,
    headers: { "content-type": "application/json" },
  });
  const providerHealth = await worker.fetch(new Request("https://ani.dangphuc.me/api/providers/health"), env, context);
  assert.equal(providerHealth.status, 502);
  assert.equal(providerHealth.headers.get("x-any-watch-mode"), "app");
  const youtubeFailure = await worker.fetch(new Request("https://ani.dangphuc.me/api/youtube/trending"), env, context);
  assert.equal(youtubeFailure.status, 502);
  assert.equal(youtubeFailure.headers.get("x-any-watch-mode"), "app");
  assert.equal((await youtubeFailure.json()).code, "PROVIDER_UNAVAILABLE");

  globalThis.fetch = async () => new Response("upstream unavailable", {
    status: 502,
    headers: { "content-type": "text/plain" },
  });
  const proxyFailure = await worker.fetch(new Request("https://ani.dangphuc.me/api/youtube/trending"), env, context);
  assert.equal(proxyFailure.status, 503);
  assert.equal(proxyFailure.headers.get("x-any-watch-mode"), "maintenance");

  const requestedUrls = [];
  globalThis.fetch = async (request) => {
    const url = request instanceof Request ? request.url : String(request);
    requestedUrls.push(url);
    if (url.startsWith("https://ani.dangphuc.me")) throw new TypeError("origin offline");
    return new Response("maintenance", { status: 200, headers: { "content-type": "text/html" } });
  };
  const fallback = await worker.fetch(new Request("https://ani.dangphuc.me/watch/21", {
    headers: { accept: "text/html" },
  }), env, context);
  assert.equal(fallback.status, 200);
  assert.equal(fallback.headers.get("x-any-watch-mode"), "maintenance");
  assert.equal(fallback.headers.get("cache-control"), "no-store");
  assert.equal(requestedUrls[1], "https://silent9669.github.io/any-watch/");
  assert.doesNotMatch(await fallback.text(), /ani-desk|ani_desk/i);

  globalThis.fetch = async () => { throw new TypeError("origin offline"); };
  const unavailableFallback = await worker.fetch(new Request("https://ani.dangphuc.me/watch/21", {
    headers: { accept: "text/html" },
  }), env, context);
  assert.equal(unavailableFallback.status, 503);
  assert.equal(unavailableFallback.headers.get("x-any-watch-mode"), "maintenance");
  assert.equal(await unavailableFallback.text(), "Maintenance page is temporarily unavailable.");

  const apiFallback = await worker.fetch(new Request("https://ani.dangphuc.me/api/health"), env, context);
  assert.equal(apiFallback.status, 503);
  assert.equal(apiFallback.headers.get("x-any-watch-mode"), "maintenance");
  const apiBody = await apiFallback.json();
  assert.equal(apiBody.code, "SERVICE_UNAVAILABLE");
  assert.equal(apiBody.message, "any-watch is temporarily offline for maintenance.");

  const status = await worker.fetch(new Request("https://ani.dangphuc.me/status.json"), env, context);
  const statusBody = await status.json();
  assert.equal(status.status, 200);
  assert.equal(status.headers.get("cache-control"), "no-store");
  assert.equal(statusBody.service, "any-watch");
  assert.equal(statusBody.mode, "maintenance");
  assert.equal(statusBody.detectedAtIso, apiBody.detectedAtIso);
  assert.ok(Number.isFinite(new Date(statusBody.detectedAtIso).getTime()));

  globalThis.fetch = async () => new Response(JSON.stringify({ service: "any-watch", status: "ok" }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
  const recovered = await worker.fetch(new Request("https://ani.dangphuc.me/status.json"), env, context);
  const recoveredBody = await recovered.json();
  assert.equal(recoveredBody.mode, "online");
  assert.equal(recoveredBody.detectedAtIso, null);
} finally {
  globalThis.fetch = originalFetch;
}

console.log("Cloudflare failover worker behavior valid.");
