// The explorer's arithmetic, apart from any drawing: the published CSV parsed,
// its changes, the period presets, the period's summary, and the address that
// records what is on screen. Pure functions, so they are tested without a
// browser, and the numbers a reader sees are the numbers the tests check.
//
// Every value computed here (a change, a CAGR) is labelled as computed on the
// page and is never written anywhere. Tooltips and calendar cells show the
// CSV's own text, so a value reads exactly as published (90.50 stays 90.50).

export const DAY = 864e5;
export const YEAR = 365.2425 * DAY;
export const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// `YYYY-MM-DD` or `YYYY-MM` as a UTC time; a month counts from its first day.
export function toTime(key) {
  const [y, m, d] = key.split("-").map(Number);
  return Date.UTC(y, m - 1, d || 1);
}

// The published CSVs are written by xfina-data and never quote a field, so
// splitting on commas is the whole parser.
export function parse(text) {
  const [header, ...lines] = text.trim().split("\n");
  const keyed = lines.map((line) => line.split(","));
  return { columns: header.split(","), keyed, time: keyed.map((r) => toTime(r[0])) };
}

// How a change between two values is measured. A price or an index changes
// by a percentage of itself. A series that is itself a percentage, such as a
// policy rate (the catalog's `change: points`), changes by the difference:
// 6.50 to 5.25 is -1.25 points, and calling it -19.23% would read as a
// return, which a rate is not.
function changeBetween(points) {
  return points ? (b, a) => b - a : (b, a) => (b / a - 1) * 100;
}

// Each drawn line's published values, its change on the previous published
// row, and, where configured, its change on the same month a year earlier.
// A configured column missing from the CSV is an error, not an empty line.
export function derive(rows, config) {
  const points = config.change === "points";
  const delta = changeBetween(points);
  const lines = config.lines.map((line) => {
    const index = rows.columns.indexOf(line.column);
    if (index < 0) throw new Error(`column ${line.column} is not in the CSV (${rows.columns.join(", ")})`);
    return { ...line, index };
  });
  const level = lines.map((line) => rows.keyed.map((r) => Number(r[line.index])));
  // The previous trading day for a daily series, the previous month for a
  // monthly one (which has no gaps).
  const change = level.map((series) => series.map((v, i) => (i === 0 ? null : delta(v, series[i - 1]))));
  const yoy = lines.map((line) => {
    const byKey = new Map(rows.keyed.map((r) => [r[0], Number(r[line.index])]));
    return rows.keyed.map((r) => {
      const [y, m] = r[0].split("-");
      const before = byKey.get(String(Number(y) - 1) + "-" + m);
      return before ? delta(Number(r[line.index]), before) : null;
    });
  });
  return { lines, level, change, yoy, points };
}

export function modes(views) {
  return [
    ["level", "Level"],
    ["change", "Change"],
    ...(views.includes("year-on-year") ? [["yoy", "YoY"]] : []),
  ];
}

// A preset is offered only when the series is longer than it: a 10Y button on
// six years of data would just mean All.
export function ranges(first, last) {
  const span = (last - first) / YEAR;
  return [1, 5, 10]
    .filter((n) => span > n)
    .map((n) => [String(n), `${n}Y`])
    .concat([["0", "All"]]);
}

// Five years of a long daily series opens readably; anything else opens whole.
export function defaultPreset(first, last, monthly) {
  return !monthly && (last - first) / YEAR > 5 ? 5 : 0;
}

export function presetPeriod(years, first, last) {
  if (!years) return { preset: 0, from: first, to: last };
  const end = new Date(last);
  const from = Math.max(first, Date.UTC(end.getUTCFullYear() - years, end.getUTCMonth(), end.getUTCDate()));
  return { preset: years, from, to: last };
}

// A custom period from two days, in either order, kept inside the series. A
// monthly series counts a month from its first day, so a start date in
// mid-March still includes March.
export function customPeriod(from, to, first, last, monthly) {
  if (from > to) [from, to] = [to, from];
  if (monthly) {
    const start = new Date(from);
    from = Date.UTC(start.getUTCFullYear(), start.getUTCMonth(), 1);
  }
  const clampedFrom = Math.max(first, Math.min(from, last));
  return { preset: null, from: clampedFrom, to: Math.max(clampedFrom, Math.min(to, last)) };
}

// The index range of rows inside a period; lo > hi when none are.
export function periodRows(time, from, to) {
  let lo = 0;
  while (lo < time.length && time[lo] < from) lo += 1;
  let hi = time.length - 1;
  while (hi >= 0 && time[hi] > to) hi -= 1;
  return [lo, hi];
}

// The period's own summary: each line's first and last published value in it,
// the change between them, and that change as a compound annual rate,
// annualised pro rata over whatever span is chosen, a few weeks included. A
// period with no length (a single row) has no rate, and nor does a change in
// points: compounding a rate's difference means nothing.
export function summary(rows, derived, lo, hi) {
  if (lo > hi) return null;
  const years = (rows.time[hi] - rows.time[lo]) / YEAR;
  const span = years >= 1 ? `${years.toFixed(1)} years` : `${Math.round(years * 365.2425)} days`;
  return {
    text: `${rows.keyed[lo][0]} → ${rows.keyed[hi][0]} · ${span} · ${hi - lo + 1} rows`,
    stats: derived.lines.map((line, k) => {
      const a = derived.level[k][lo];
      const b = derived.level[k][hi];
      return {
        label: line.label,
        from: rows.keyed[lo][line.index],
        to: rows.keyed[hi][line.index],
        change: formatChange(changeBetween(derived.points)(b, a), derived.points),
        cagr: derived.points ? null : years > 0 ? `${percent((Math.pow(b / a, 1 / years) - 1) * 100)} a year` : "—",
      };
    }),
  };
}

// The address carries what is on screen, so a view can be shared and the same
// period opened on another dataset to compare them exactly:
// ?from=2024-01-01&to=2024-12-31, or ?period=1y, plus view and mode.
export function readQuery(search, { modes: modeList, ranges: rangeList, first, last, monthly }) {
  const q = new URLSearchParams(search);
  const out = {};
  if (["chart", "calendar"].includes(q.get("view"))) out.view = q.get("view");
  if (modeList.some(([m]) => m === q.get("mode"))) out.mode = q.get("mode");
  const period = (q.get("period") || "").toLowerCase();
  const preset = period === "all" ? 0 : parseInt(period, 10);
  if (period && rangeList.some(([n]) => Number(n) === preset)) Object.assign(out, presetPeriod(preset, first, last));
  const from = parseDay(q.get("from"));
  const to = parseDay(q.get("to"));
  if (from != null || to != null) {
    Object.assign(out, customPeriod(from ?? out.from ?? first, to ?? out.to ?? last, first, last, monthly));
  }
  return out;
}

export function writeQuery(state, defaultYears) {
  const q = new URLSearchParams();
  if (state.view !== "chart") q.set("view", state.view);
  if (state.mode !== "level") q.set("mode", state.mode);
  if (state.preset == null) {
    q.set("from", ymd(state.from));
    q.set("to", ymd(state.to));
  } else if (state.preset !== defaultYears) {
    q.set("period", state.preset ? `${state.preset}y` : "all");
  }
  const query = q.toString();
  return query ? `?${query}` : "";
}

// `YYYY-MM-DD` as a UTC day, or null if it is not one.
export function parseDay(text) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text || "");
  if (!match) return null;
  const t = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return Number.isNaN(t) ? null : t;
}

export function ymd(t) {
  return new Date(t).toISOString().slice(0, 10);
}

export function stamp(t, monthly) {
  return new Date(t).toISOString().slice(0, monthly ? 7 : 10);
}

export function percent(value) {
  return (value > 0 ? "+" : "") + value.toFixed(2) + "%";
}

// A computed change as the page shows it. Points are rounded to four places,
// which is as many as the BIS prints, so a difference such as 12.06 - 11.83
// reads 0.23 rather than carrying binary noise, and 0.125 stays 0.125.
export function formatChange(value, points) {
  if (!points) return percent(value);
  return (value > 0 ? "+" : "") + String(Number(value.toFixed(4))) + " pp";
}

// A published value short enough for a calendar cell: digits past the second
// decimal are cut, not rounded, and marked with an ellipsis.
export function cellText(text) {
  const [whole, fraction] = text.split(".");
  return fraction && fraction.length > 2 ? `${whole}.${fraction.slice(0, 2)}…` : text;
}

// Diverging scales are symmetric about zero, clipped at the 95th percentile of
// the size of a change, so one outlier does not grey out every other cell; the
// tooltip still has the exact value.
export function divergingBound(series) {
  const sizes = series
    .filter((v) => v != null && Number.isFinite(v))
    .map(Math.abs)
    .sort((a, b) => a - b);
  return sizes.length ? sizes[Math.min(sizes.length - 1, Math.floor(sizes.length * 0.95))] || 1 : 1;
}

export function axisName(mode, unit, monthly, points) {
  if (mode === "level") return unit;
  const what = points ? "Change in percentage points" : "% change";
  if (mode === "yoy") return `${what} on a year earlier`;
  return monthly ? `${what} on the month before` : `${what} on the previous published day`;
}

export function escape(text) {
  return String(text).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}
