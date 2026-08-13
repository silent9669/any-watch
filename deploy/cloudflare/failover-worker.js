const MAINTENANCE_ORIGIN = "https://silent9669.github.io";
const MAINTENANCE_PREFIX = "/any-watch";
const ORIGIN_TIMEOUT_MS = 4_000;

export default {
  async fetch(request) {
    try {
      const originResponse = await fetch(request, {
        cache: "no-store",
        redirect: "manual",
        signal: AbortSignal.timeout(ORIGIN_TIMEOUT_MS),
      });

      if (originResponse.status < 500) {
        return withMode(originResponse, "app", true);
      }
    } catch {
      // Network failure and timeout both select the independent fallback.
    }

    return maintenanceResponse(request);
  },
};

async function maintenanceResponse(request) {
  const url = new URL(request.url);
  if ((request.method !== "GET" && request.method !== "HEAD") || url.pathname.startsWith("/api/")) {
    return new Response(JSON.stringify({
      code: "SERVICE_UNAVAILABLE",
      message: "any-watch is temporarily offline for maintenance.",
      retryable: true,
    }), {
      status: 503,
      headers: {
        "cache-control": "no-store",
        "content-type": "application/json; charset=utf-8",
        "retry-after": "60",
        "x-any-watch-mode": "maintenance",
      },
    });
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
    cf: { cacheTtl: url.pathname === "/status.json" ? 0 : 300 },
  });

  if (!fallbackResponse.ok && isNavigation) {
    return new Response("Maintenance page is temporarily unavailable.", {
      status: 503,
      headers: {
        "cache-control": "no-store",
        "content-type": "text/plain; charset=utf-8",
        "retry-after": "60",
        "x-any-watch-mode": "maintenance",
      },
    });
  }

  return withMode(
    fallbackResponse,
    "maintenance",
    url.pathname === "/" || url.pathname === "/status.json" || isNavigation,
  );
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
