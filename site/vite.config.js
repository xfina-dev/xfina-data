// data.xfina.dev: a Vue app, rendered to static HTML at build time by
// vite-ssg. Every page (the index, one per published dataset, and the 404)
// is a complete HTML file, so search engines and link previews read each
// dataset's title, summary and facts without running a script; Vue then
// takes over in the browser for the chart, the calendar and the picker.
//
// The pages come from site-data.json, which `xfina-data site data` writes
// from the catalog and metadata.json. SITE_DATA points at another copy, such
// as the recorded fixture CI builds from.

import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import xfina from "xfina-ui/vite";

const here = (path) => fileURLToPath(new URL(path, import.meta.url));
// SITE_DATA and DATA_DIR are paths from site/, where npm runs.
const fromSite = (path) => resolve(here("."), path);
export const siteDataPath = fromSite(process.env.SITE_DATA || "src/site-data.json");

function published() {
  const site = JSON.parse(readFileSync(siteDataPath, "utf8"));
  return site.groups.flatMap((g) => g.datasets).filter((d) => d.published);
}

// `npm run dev` serves /v1/ from DATA_DIR, or from the fixtures, so the
// explorer has CSVs to draw without a deploy.
function devData() {
  const root = fromSite(process.env.DATA_DIR || "fixtures/data");
  return {
    name: "xfina-data-dev-data",
    configureServer(server) {
      server.middlewares.use("/v1", (req, res, next) => {
        const file = join(root, "v1", decodeURIComponent(req.url.split("?")[0]));
        if (!existsSync(file)) return next();
        res.setHeader("Content-Type", file.endsWith(".csv") ? "text/csv; charset=utf-8" : "application/json");
        res.end(readFileSync(file));
      });
    },
  };
}

export default defineConfig({
  plugins: [vue(), xfina(), devData()],
  resolve: { alias: { "@site-data": siteDataPath } },
  ssgOptions: {
    // /datasets/bis-usd-inr/ is written as datasets/bis-usd-inr/index.html,
    // the paths the site has always served.
    dirStyle: "nested",
    includedRoutes: () => ["/", ...published().map((d) => `/datasets/${d.id}/`), "/404"],
    formatting: "none",
  },
  server: { port: 4311 },
  test: { environment: "jsdom", include: ["test/**/*.test.js"] },
});
