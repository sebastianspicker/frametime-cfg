import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("./", import.meta.url);
const [html, css, javascript, readme] = await Promise.all([
  readFile(new URL("index.html", root), "utf8"),
  readFile(new URL("styles.css", root), "utf8"),
  readFile(new URL("app.js", root), "utf8"),
  readFile(new URL("README.md", root), "utf8")
]);

const expectedPanels = ["overview", "assess", "setup", "benchmark", "network", "video", "recovery"];

test("ships one complete dependency-free static entrypoint", () => {
  assert.match(html, /<link rel="icon" href="favicon\.svg" type="image\/svg\+xml">/);
  assert.match(html, /<link rel="stylesheet" href="styles\.css">/);
  assert.match(html, /<script src="app\.js" defer><\/script>/);
  assert.doesNotMatch(html, /(?:src|href)="https?:\/\//i);
  assert.doesNotMatch(css, /@import\s|url\(\s*["']?https?:\/\//i);
  assert.doesNotMatch(javascript, /\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource/);
});

test("exposes all seven desktop workflow views and navigation targets", () => {
  for (const panel of expectedPanels) {
    assert.equal(html.split(`data-panel="${panel}"`).length - 1, 1, panel);
    assert.equal(html.split(`data-panel-view="${panel}"`).length - 1, 1, panel);
  }
  assert.match(javascript, /querySelectorAll\("\[data-panel\]"\)/);
  assert.match(javascript, /aria-selected/);
});

test("keeps every cross-panel action resolvable", () => {
  const destinations = [...html.matchAll(/data-go="([^"]+)"/g)].map((match) => match[1]);
  assert.ok(destinations.length >= 3);
  for (const destination of destinations) {
    assert.ok(expectedPanels.includes(destination), `unknown data-go target: ${destination}`);
  }
});

test("states the browser-only trust boundary in visible copy and code", () => {
  assert.match(html, /Nothing here runs PowerShell or writes system state\./);
  assert.match(html, /Browser simulation only/);
  assert.match(html, /not a full system snapshot/i);
  assert.match(javascript, /No PowerShell command ran\./);
  assert.match(javascript, /No suite state was written\./);
});

test("blocks browser network connections and remote embedding", () => {
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /object-src 'none'/);
  assert.match(html, /base-uri 'none'/);
  assert.match(html, /form-action 'none'/);
});

test("retains keyboard, reduced-motion, forced-color, and responsive contracts", () => {
  assert.match(html, /class="skip-link"/);
  assert.match(html, /role="tablist"/);
  assert.match(html, /aria-live="polite"/);
  assert.match(css, /:focus-visible/);
  assert.match(css, /prefers-reduced-motion: reduce/);
  assert.match(css, /forced-colors: active/);
  assert.match(css, /@media \(max-width: 620px\)/);
  assert.match(javascript, /ArrowDown/);
  assert.match(javascript, /event\.key === "Escape"/);
});

test("documents the entrypoint and exact local checks", () => {
  assert.match(readme, /Open `index\.html` directly in a browser/);
  assert.match(readme, /node --check demo\/app\.js/);
  assert.match(readme, /node --test demo\/demo\.test\.mjs/);
});

test("has valid JavaScript syntax without executing the DOM entrypoint", () => {
  assert.doesNotThrow(() => new Function(javascript));
});
