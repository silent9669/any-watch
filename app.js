/* Hallmark · status behavior: explicit, bounded, same-origin only */
(() => {
  "use strict";

  const defaults = Object.freeze({
    mode: "maintenance",
    headline: "OFFLINE MAI UPDATE",
    message:
      "any-watch is temporarily offline for maintenance. Your watch history, account, and library remain stored safely on my home server.",
    statusLabel: "Maintenance in progress",
    expectedReturn: "Shortly",
    detectedAtIso: null,
    privacy: "Account data stays on home server."
  });

  function detectionTime(value) {
    if (typeof value !== "string" || !value.trim()) return null;
    const date = new Date(value);
    if (!Number.isFinite(date.getTime())) return null;
    const parts = new Intl.DateTimeFormat("en-GB", {
      day: "numeric",
      month: "short",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      timeZoneName: "short"
    }).formatToParts(date);
    const part = (type) => parts.find((item) => item.type === type)?.value ?? "";
    return {
      iso: date.toISOString(),
      display: `${part("day")} ${part("month")} ${part("year")} · ${part("hour")}:${part("minute")}:${part("second")} ${part("timeZoneName")}`
    };
  }

  const elements = {
    title: document.querySelector("#maintenance-title"),
    message: document.querySelector("#maintenance-message"),
    current: document.querySelector("#current-status"),
    expected: document.querySelector("#expected-return"),
    detected: document.querySelector("#down-detected"),
    privacy: document.querySelector("#privacy-note"),
    button: document.querySelector("#check-again"),
    live: document.querySelector("#status-live")
  };

  let status = defaults;
  let resetTimer;

  const safeText = (value, fallback, maxLength = 220) =>
    typeof value === "string" && value.trim() ? value.trim().slice(0, maxLength) : fallback;

  function normalize(data) {
    const mode = data?.mode === "online" ? "online" : "maintenance";
    return {
      mode,
      headline: safeText(data?.headline, defaults.headline, 90),
      message: safeText(data?.message, defaults.message, 320),
      statusLabel: safeText(
        data?.statusLabel,
        mode === "online" ? "The theatre is online" : defaults.statusLabel,
        60
      ),
      expectedReturn: safeText(data?.expectedReturn, defaults.expectedReturn, 60),
      detected: detectionTime(data?.detectedAtIso),
      privacy: safeText(data?.privacy, defaults.privacy, 120)
    };
  }

  function render(next) {
    status = next;
    document.documentElement.dataset.mode = next.mode;
    elements.title.textContent = next.headline;
    elements.message.textContent = next.message;
    elements.current.lastChild.textContent = ` ${next.statusLabel}`;
    elements.expected.textContent = next.expectedReturn;
    elements.detected.textContent = next.detected?.display ?? "Detection time unavailable";
    if (next.detected) elements.detected.dateTime = next.detected.iso;
    else elements.detected.removeAttribute("datetime");
    elements.privacy.lastChild.textContent = ` ${next.privacy}`;
  }

  function setButtonState(state, announcement) {
    window.clearTimeout(resetTimer);
    const labels = {
      idle: "Check again",
      checking: "Checking…",
      online: "Theatre online",
      offline: "Still offline",
      failed: "Try again"
    };
    elements.button.dataset.state = state;
    elements.button.disabled = state === "checking";
    elements.button.querySelector(".button-label").textContent = labels[state];
    elements.live.textContent = announcement || "";

    if (!["idle", "checking"].includes(state)) {
      resetTimer = window.setTimeout(() => setButtonState("idle", ""), 4200);
    }
  }

  async function fetchStatus() {
    const response = await fetch(`status.json?now=${Date.now()}`, {
      cache: "no-store",
      credentials: "same-origin",
      headers: { Accept: "application/json" }
    });
    if (!response.ok) throw new Error("status-unavailable");
    return normalize(await response.json());
  }

  async function refresh({ announce = false } = {}) {
    if (announce) setButtonState("checking", "Checking theatre status.");
    try {
      const next = await fetchStatus();
      render(next);
      if (announce) {
        setButtonState(
          next.mode === "online" ? "online" : "offline",
          next.mode === "online"
            ? "The theatre is back online."
            : "The theatre is still offline for maintenance."
        );
      }
    } catch {
      if (announce) {
        setButtonState(
          "failed",
          "The status request failed. The maintenance page is still available; please try again shortly."
        );
      }
    }
  }

  elements.button.addEventListener("click", () => refresh({ announce: true }));
  render(normalize(defaults));
  refresh();
})();
