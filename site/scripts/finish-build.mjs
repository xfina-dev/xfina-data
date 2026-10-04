// After vite-ssg: move the 404 page to where Cloudflare looks for it, check
// that every page the site data promises was written, and, for a deploy, copy
// the published data in beside the pages.
//
//   DATA_DIR=../data npm run build                     # a deploy: dist/ holds v1/ too
//   SITE_DATA=fixtures/site-data.json npm run build    # pages only, as CI builds them

import { cpSync, existsSync, readFileSync, renameSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { siteDataPath } from "../vite.config.js";

const dist = fileURLToPath(new URL("../dist", import.meta.url));

// vite-ssg's manifest for its own server render; nothing to serve.
rmSync(join(dist, ".vite"), { recursive: true, force: true });

// vite-ssg writes /404 as 404/index.html; Cloudflare serves 404.html for any
// path that is not a file.
renameSync(join(dist, "404/index.html"), join(dist, "404.html"));
rmSync(join(dist, "404"), { recursive: true });

// A page that was not written would be a 404 on the live site, so a missing
// one fails the build rather than the reader.
const site = JSON.parse(readFileSync(siteDataPath, "utf8"));
const expected = [
  "index.html",
  "404.html",
  ...site.groups.flatMap((g) => g.datasets).filter((d) => d.published).map((d) => `datasets/${d.id}/index.html`),
];
const missing = expected.filter((page) => !existsSync(join(dist, page)));
if (missing.length) {
  console.error(`finish-build: pages not written: ${missing.join(", ")}`);
  process.exit(1);
}

// The point of building the pages ahead is that each one, as served, already
// names its dataset: a page that only gets its title once scripts run is the
// single-page app this replaced.
const escapeHtml = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
for (const d of site.groups.flatMap((g) => g.datasets).filter((d) => d.published)) {
  const html = readFileSync(join(dist, `datasets/${d.id}/index.html`), "utf8");
  for (const needle of [`<title>${escapeHtml(d.title)} · Xfina Data</title>`, escapeHtml(d.summary), d.published.first]) {
    if (!html.includes(needle)) {
      console.error(`finish-build: datasets/${d.id}/index.html does not contain ${needle.slice(0, 60)}`);
      process.exit(1);
    }
  }
}

if (process.env.DATA_DIR) {
  const v1 = join(resolve(fileURLToPath(new URL("..", import.meta.url)), process.env.DATA_DIR), "v1");
  if (!existsSync(join(v1, "metadata.json"))) {
    console.error(`finish-build: ${v1} has no metadata.json; is DATA_DIR the data branch?`);
    process.exit(1);
  }
  cpSync(v1, join(dist, "v1"), { recursive: true });
}
console.log(`finish-build: ${expected.length} pages${process.env.DATA_DIR ? ", data copied" : ""}`);
