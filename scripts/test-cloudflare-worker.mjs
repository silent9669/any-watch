import assert from "node:assert/strict";
import worker from "../deploy/cloudflare/failover-worker.js";

const originalFetch = globalThis.fetch;

try {
  globalThis.fetch = async (_request, init) => {
    assert.equal(init.cache, "no-store");
    return new Response("app", { status: 200 });
  };
  const online = await worker.fetch(new Request("https://ani.dangphuc.me/"));
  assert.equal(online.status, 200);
  assert.equal(online.headers.get("x-ani-desk-mode"), "app");
  assert.equal(online.headers.get("cache-control"), "no-store");
  assert.equal(await online.text(), "app");

  const requestedUrls = [];
  globalThis.fetch = async (request) => {
    const url = request instanceof Request ? request.url : String(request);
    requestedUrls.push(url);
    if (url.startsWith("https://ani.dangphuc.me")) throw new TypeError("origin offline");
    return new Response("maintenance", { status: 200, headers: { "content-type": "text/html" } });
  };
  const fallback = await worker.fetch(new Request("https://ani.dangphuc.me/watch/21", {
    headers: { accept: "text/html" },
  }));
  assert.equal(fallback.status, 200);
  assert.equal(fallback.headers.get("x-ani-desk-mode"), "maintenance");
  assert.equal(fallback.headers.get("cache-control"), "no-store");
  assert.equal(requestedUrls[1], "https://silent9669.github.io/any-watch/");

  const bareRootFallback = await worker.fetch(new Request("https://ani.dangphuc.me/"));
  assert.equal(bareRootFallback.headers.get("x-ani-desk-mode"), "maintenance");
  assert.equal(bareRootFallback.headers.get("cache-control"), "no-store");

  globalThis.fetch = async () => { throw new TypeError("origin offline"); };
  const apiFallback = await worker.fetch(new Request("https://ani.dangphuc.me/api/health"));
  assert.equal(apiFallback.status, 503);
  assert.equal(apiFallback.headers.get("x-ani-desk-mode"), "maintenance");
  assert.equal((await apiFallback.json()).code, "SERVICE_UNAVAILABLE");
} finally {
  globalThis.fetch = originalFetch;
}

console.log("Cloudflare failover worker behavior valid.");
