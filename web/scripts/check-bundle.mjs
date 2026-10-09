// Fails when the JavaScript a first visit must download (the entry script and the
// chunks index.html preloads) grows past the budget, gzipped. Run after `vite build`.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const BUDGET_KB = Number(process.env.BUNDLE_BUDGET_KB ?? 180);
const dist = new URL("../dist/", import.meta.url).pathname;
const html = readFileSync(join(dist, "index.html"), "utf8");
const files = [
  ...html.matchAll(/<script[^>]+type="module"[^>]+src="\/([^"]+\.js)"/g),
  ...html.matchAll(/<link[^>]+rel="modulepreload"[^>]+href="\/([^"]+\.js)"/g),
].map((m) => m[1]);

let total = 0;
for (const file of new Set(files)) {
  const size = gzipSync(readFileSync(join(dist, file)), { level: 9 }).length;
  total += size;
  console.log(`${(size / 1024).toFixed(1).padStart(7)} kB  ${file}`);
}
const kb = total / 1024;
console.log(`${kb.toFixed(1).padStart(7)} kB  initial JavaScript (gzip), budget ${BUDGET_KB} kB`);
if (files.length === 0) {
  console.error("no scripts found in dist/index.html");
  process.exit(1);
}
if (kb > BUDGET_KB) {
  console.error(`initial JavaScript is over budget: lazy-load something (routes, dialogs).`);
  process.exit(1);
}
