import { access, readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const artifact = join(root, "maintenance");
const requiredFiles = [
  "index.html",
  "styles.css",
  "app.js",
  "status.json",
  "assets/logo.png",
  "assets/art/cinema-brick-wall.webp",
  "assets/fonts/barlow-condensed-700.woff2",
  "assets/fonts/barlow-condensed-latin-700.woff2",
  "assets/fonts/barlow-condensed-vietnamese-700.woff2",
  "assets/fonts/manrope-400.woff2",
  "assets/fonts/manrope-600.woff2",
  "assets/fonts/manrope-latin-400.woff2",
  "assets/fonts/manrope-latin-600.woff2",
  "assets/fonts/manrope-vietnamese-400.woff2",
  "assets/fonts/manrope-vietnamese-600.woff2",
  "assets/fonts/ibm-plex-mono-500.woff2",
  "assets/fonts/ibm-plex-mono-latin-ext-500.woff2",
  "assets/fonts/ibm-plex-mono-vietnamese-500.woff2"
];

await Promise.all(requiredFiles.map((file) => access(join(artifact, file))));

const [html, css, script, statusSource] = await Promise.all([
  readFile(join(artifact, "index.html"), "utf8"),
  readFile(join(artifact, "styles.css"), "utf8"),
  readFile(join(artifact, "app.js"), "utf8"),
  readFile(join(artifact, "status.json"), "utf8")
]);

const status = JSON.parse(statusSource);
const requiredFields = [
  "mode",
  "headline",
  "message",
  "statusLabel",
  "expectedReturn",
  "privacy"
];

for (const field of requiredFields) {
  if (typeof status[field] !== "string" || !status[field].trim()) {
    throw new Error(`status.json requires a non-empty ${field} string`);
  }
}

if (status.detectedAtIso !== null || status.checkedAtIso !== null) {
  throw new Error("static status.json timestamps must be null; the Worker owns live outage timing");
}

if (!new Set(["maintenance", "online"]).has(status.mode)) {
  throw new Error("status.json mode must be maintenance or online");
}

const forbidden = [
  /https?:\/\//i,
  /\b(?:\d{1,3}\.){3}\d{1,3}\b/,
  /authorization/i,
  /bearer\s+/i,
  /cookie/i,
  /stack\s*trace/i
];

for (const [name, source] of [["index.html", html], ["status.json", statusSource], ["app.js", script]]) {
  for (const pattern of forbidden) {
    if (pattern.test(source)) throw new Error(`${name} contains forbidden public detail: ${pattern}`);
  }
}

for (const [name, source] of [["index.html", html], ["status.json", statusSource], ["app.js", script]]) {
  if (/ani-desk|ani_desk/i.test(source)) throw new Error(`${name} contains former product branding`);
}

const checks = [
  [html.includes('aria-live="polite"'), "aria-live status region"],
  [html.includes('id="check-again"'), "Check again action"],
  [html.includes('id="down-detected"'), "outage detection time"],
  [html.includes("any-watch"), "any-watch branding"],
  [html.includes('class="announcement__track" tabindex="0"'), "focusable announcement strip"],
  [css.includes("overflow: clip"), "viewport overflow guard"],
  [css.includes("prefers-reduced-motion"), "reduced-motion treatment"],
  [css.includes("pre-emit critique:"), "Hallmark critique stamp"],
  [script.includes('cache: "no-store"'), "no-store status fetch"],
  [script.includes("detectedAtIso"), "Worker outage timestamp rendering"],
  [!html.includes("<script type=\"module\""), "dependency-free fallback rendering"]
];

for (const [condition, label] of checks) {
  if (!condition) throw new Error(`Maintenance validation failed: ${label}`);
}

console.log(`Maintenance artifact valid (${requiredFiles.length} files, ${requiredFields.length} status fields).`);
