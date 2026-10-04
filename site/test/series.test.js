// @vitest-environment node
// The explorer's arithmetic, on the fixture CSVs the Rust tests publish.

import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import {
  YEAR,
  cellText,
  customPeriod,
  defaultPreset,
  derive,
  divergingBound,
  modes,
  parse,
  percent,
  periodRows,
  presetPeriod,
  ranges,
  readQuery,
  summary,
  toTime,
  writeQuery,
} from "../src/explorer/series.js";

const fixture = (path) => readFileSync(new URL(`../fixtures/data/${path}`, import.meta.url), "utf8");
const cpiConfig = { lines: [{ column: "index", label: "CPI" }], views: ["year-on-year"] };

describe("the published CSV", () => {
  test("parses into rows and times, a month counting from its first day", () => {
    const rows = parse(fixture("v1/inflation/in-cpi.csv"));
    expect(rows.columns).toEqual(["month", "index"]);
    expect(rows.keyed[0]).toEqual(["2024-01", "97.70"]);
    expect(rows.time[0]).toBe(Date.UTC(2024, 0, 1));
    expect(toTime("2026-09-29")).toBe(Date.UTC(2026, 8, 29));
  });

  test("derives the change on the previous row and on a year earlier", () => {
    const rows = parse(fixture("v1/inflation/in-cpi.csv"));
    const { lines, change, yoy } = derive(rows, cpiConfig);
    expect(lines[0].index).toBe(1);
    expect(change[0][0]).toBeNull();
    expect(change[0][1]).toBeCloseTo((97.9 / 97.7 - 1) * 100, 10);
    // 2024 has no year before it in the fixture; 2025-01 does.
    expect(yoy[0][0]).toBeNull();
    const jan25 = rows.keyed.findIndex((r) => r[0] === "2025-01");
    expect(yoy[0][jan25]).toBeCloseTo((Number(rows.keyed[jan25][1]) / 97.7 - 1) * 100, 10);
  });

  test("a configured column missing from the CSV is an error, not an empty line", () => {
    const rows = parse(fixture("v1/fx/bis-usd-inr.csv"));
    expect(() => derive(rows, { lines: [{ column: "value", label: "USD/INR" }] })).toThrow(/column value is not in the CSV/);
  });
});

describe("periods", () => {
  const first = Date.UTC(2020, 0, 6);
  const last = Date.UTC(2026, 9, 3);

  test("presets are offered only when the series is longer than them", () => {
    expect(ranges(first, last).map(([, label]) => label)).toEqual(["1Y", "5Y", "All"]);
    expect(ranges(first, first + 0.5 * YEAR).map(([, label]) => label)).toEqual(["All"]);
  });

  test("a long daily series opens on five years; a monthly one opens whole", () => {
    expect(defaultPreset(first, last, false)).toBe(5);
    expect(defaultPreset(first, last, true)).toBe(0);
    expect(presetPeriod(5, first, last)).toEqual({ preset: 5, from: Date.UTC(2021, 9, 3), to: last });
    expect(presetPeriod(0, first, last)).toEqual({ preset: 0, from: first, to: last });
  });

  test("a custom period is ordered, kept inside the series, and counts whole months", () => {
    expect(customPeriod(last + YEAR, first - YEAR, first, last, false)).toEqual({ preset: null, from: first, to: last });
    expect(customPeriod(Date.UTC(2024, 2, 15), Date.UTC(2024, 5, 1), first, last, true).from).toBe(Date.UTC(2024, 2, 1));
  });

  test("the period's rows, and an empty period", () => {
    const time = [1, 2, 3, 4, 5];
    expect(periodRows(time, 2, 4)).toEqual([1, 3]);
    const [lo, hi] = periodRows(time, 6, 9);
    expect(lo > hi).toBe(true);
  });
});

describe("the summary", () => {
  test("first and last published values, the change, and the CAGR, pro rata", () => {
    const rows = parse(fixture("v1/inflation/in-cpi.csv"));
    const derived = derive(rows, cpiConfig);
    const s = summary(rows, derived, 0, rows.keyed.length - 1);
    expect(s.text).toBe("2024-01 → 2025-12 · 1.9 years · 24 rows");
    const [cpi] = s.stats;
    expect([cpi.from, cpi.to]).toEqual(["97.70", "104.10"]);
    expect(cpi.change).toBe(percent((104.1 / 97.7 - 1) * 100));
    const years = (Date.UTC(2025, 11, 1) - Date.UTC(2024, 0, 1)) / YEAR;
    expect(cpi.cagr).toBe(`${percent((Math.pow(104.1 / 97.7, 1 / years) - 1) * 100)} a year`);
  });

  test("a single row has a change but no rate", () => {
    const rows = parse(fixture("v1/inflation/in-cpi.csv"));
    expect(summary(rows, derive(rows, cpiConfig), 3, 3).stats[0].cagr).toBe("—");
    expect(summary(rows, derive(rows, cpiConfig), 4, 3)).toBeNull();
  });
});

describe("the address", () => {
  const first = Date.UTC(1973, 0, 2);
  const last = Date.UTC(2026, 8, 29);
  const ctx = { modes: modes([]), ranges: ranges(first, last), first, last, monthly: false };

  test("reads a shared view and period, and ignores what it does not know", () => {
    expect(readQuery("?view=calendar&mode=change&period=1y", ctx)).toMatchObject({ view: "calendar", mode: "change", preset: 1 });
    expect(readQuery("?mode=yoy&view=table", ctx)).toEqual({});
    expect(readQuery("?from=2024-01-01&to=2024-12-31", ctx)).toEqual({
      preset: null,
      from: Date.UTC(2024, 0, 1),
      to: Date.UTC(2024, 11, 31),
    });
  });

  test("writes only what differs from the page's defaults", () => {
    expect(writeQuery({ view: "chart", mode: "level", preset: 5 }, 5)).toBe("");
    expect(writeQuery({ view: "calendar", mode: "change", preset: 0 }, 5)).toBe("?view=calendar&mode=change&period=all");
    expect(writeQuery({ view: "chart", mode: "level", preset: null, from: Date.UTC(2024, 0, 1), to: Date.UTC(2024, 11, 31) }, 5)).toBe(
      "?from=2024-01-01&to=2024-12-31",
    );
  });
});

test("cells keep the published digits, cut rather than rounded", () => {
  expect(cellText("95.985029")).toBe("95.98…");
  expect(cellText("90.50")).toBe("90.50");
  expect(divergingBound([null, 1, -2, 3, -4])).toBe(4);
  expect(percent(0)).toBe("0.00%");
  expect(percent(1.234)).toBe("+1.23%");
});
