// The site's datasets, from site-data.json (see vite.config.js). Read at build
// time for the static pages, and bundled for the browser so the same values
// render there.

import site from "@site-data";

export const groups = site.groups;
export const datasets = groups.flatMap((g) => g.datasets);
export const published = datasets.filter((d) => d.published);

export const datasetById = (id) => datasets.find((d) => d.id === id);
export const pageOf = (dataset) => `/datasets/${dataset.id}/`;

export const ORIGIN = "https://data.xfina.dev";
export const csvUrl = (dataset) => `${ORIGIN}/${dataset.published.path}`;

export const frequencyName = (frequency) => ({ daily: "Daily", monthly: "Monthly" })[frequency];
