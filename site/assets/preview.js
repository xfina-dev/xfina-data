// A dataset's page: its published CSV as a chart or a calendar.
//
// The two views are twins. They share one set of controls — what to show
// (the level, or its change), which period, chosen by preset or by dragging
// the slider under either view — and one summary of that period, CAGR
// included. Switching view keeps everything else as it was.
//
// Configured per dataset from the `preview` block in datasets.yaml, embedded
// in the page as JSON. Tooltips and calendar cells show the CSV's own text,
// so a value reads exactly as published (90.50 stays 90.50). Every number
// computed here — a change, a CAGR — is labelled as computed and is never
// written anywhere.

(async () => {
  const config = JSON.parse(document.getElementById("config").textContent);
  const status = document.getElementById("status");
  const monthly = config.frequency === "monthly";
  const DAY = 864e5;
  const YEAR = 365.2425 * DAY;
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  copyButton();

  let rows;
  try {
    const response = await fetch("/" + config.path);
    if (!response.ok) throw new Error("HTTP " + response.status);
    rows = parse(await response.text());
  } catch (error) {
    status.textContent = "Could not load " + config.path + ": " + error.message;
    return;
  }

  // ---- Data ----------------------------------------------------------------

  // The published CSVs are written by this project and never quote a field,
  // so splitting on commas is the whole parser.
  function parse(text) {
    const [header, ...lines] = text.trim().split("\n");
    const keyed = lines.map((line) => line.split(","));
    return { columns: header.split(","), keyed, time: keyed.map((r) => toTime(r[0])) };
  }

  function toTime(key) {
    const [y, m, d] = key.split("-").map(Number);
    return Date.UTC(y, m - 1, d || 1);
  }

  const lines = config.lines.map((line) => ({ ...line, index: rows.columns.indexOf(line.column) }));
  const level = lines.map((line) => rows.keyed.map((r) => Number(r[line.index])));
  // Change on the previous published row: the previous trading day for a
  // daily series, the previous month for a monthly one (which has no gaps).
  const change = level.map((series) => series.map((v, i) => (i === 0 ? null : (v / series[i - 1] - 1) * 100)));
  // This month over the same month a year earlier, where both are published.
  const yoy = lines.map((line) => {
    const byKey = new Map(rows.keyed.map((r) => [r[0], Number(r[line.index])]));
    return rows.keyed.map((r) => {
      const [y, m] = r[0].split("-");
      const before = byKey.get(String(Number(y) - 1) + "-" + m);
      return before ? (Number(r[line.index]) / before - 1) * 100 : null;
    });
  });

  const first = rows.time[0];
  const last = rows.time[rows.time.length - 1];
  const yearOf = (t) => new Date(t).getUTCFullYear();
  const firstYear = yearOf(first);

  const MODES = [["level", "Level"], ["change", "Change"]]
    .concat(config.views.includes("year-on-year") ? [["yoy", "YoY"]] : []);
  // A preset is offered only when the series is longer than it: a 10Y
  // button on six years of data would just mean All.
  const span = (last - first) / YEAR;
  const RANGES = [1, 5, 10].filter((n) => span > n).map((n) => [String(n), n + "Y"]).concat([["0", "All"]]);
  const defaultPreset = !monthly && span > 5 ? 5 : 0;

  const state = { view: "chart", mode: "level", preset: defaultPreset, from: first, to: last };
  setPreset(state.preset);
  readUrl();

  // The page's address carries what is on screen, so a view can be shared
  // and the same period opened on another dataset to compare them exactly:
  // ?from=2024-01-01&to=2024-12-31, or ?period=1y, plus view and mode.
  function readUrl() {
    const q = new URLSearchParams(location.search);
    if (["chart", "calendar"].includes(q.get("view"))) state.view = q.get("view");
    if (MODES.some(([m]) => m === q.get("mode"))) state.mode = q.get("mode");
    const period = (q.get("period") || "").toLowerCase();
    const preset = period === "all" ? 0 : parseInt(period, 10);
    if (period && RANGES.some(([n]) => Number(n) === preset)) setPreset(preset);
    const from = parseDay(q.get("from")), to = parseDay(q.get("to"));
    if (from != null || to != null) setPeriod(from ?? state.from, to ?? state.to);
  }

  function writeUrl() {
    const q = new URLSearchParams();
    if (state.view !== "chart") q.set("view", state.view);
    if (state.mode !== "level") q.set("mode", state.mode);
    if (state.preset == null) {
      q.set("from", ymd(state.from));
      q.set("to", ymd(state.to));
    } else if (state.preset !== defaultPreset) {
      q.set("period", state.preset ? state.preset + "y" : "all");
    }
    const query = q.toString();
    history.replaceState(null, "", location.pathname + (query ? "?" + query : ""));
  }

  // `YYYY-MM-DD` as a UTC day, or null if it is not one.
  function parseDay(text) {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text || "");
    if (!match) return null;
    const t = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
    return Number.isNaN(t) ? null : t;
  }

  function ymd(t) {
    return new Date(t).toISOString().slice(0, 10);
  }

  // A custom period from two days, in either order, kept inside the series.
  // A monthly series counts a month from its first day, so a start date in
  // mid-March still includes March.
  function setPeriod(from, to) {
    if (from > to) [from, to] = [to, from];
    if (monthly) {
      const start = new Date(from);
      from = Date.UTC(start.getUTCFullYear(), start.getUTCMonth(), 1);
    }
    state.from = Math.max(first, Math.min(from, last));
    state.to = Math.max(state.from, Math.min(to, last));
    state.preset = null;
  }

  function setPreset(years) {
    state.preset = years;
    state.to = last;
    if (!years) {
      state.from = first;
    } else {
      const end = new Date(last);
      state.from = Math.max(first, Date.UTC(end.getUTCFullYear() - years, end.getUTCMonth(), end.getUTCDate()));
    }
  }

  function valuesFor(mode) {
    return mode === "change" ? change : mode === "yoy" ? yoy : level;
  }

  // The index range of rows inside the chosen period.
  function periodRows() {
    let lo = 0;
    while (lo < rows.time.length && rows.time[lo] < state.from) lo += 1;
    let hi = rows.time.length - 1;
    while (hi >= 0 && rows.time[hi] > state.to) hi -= 1;
    return [lo, hi];
  }

  // ---- Shared helpers ------------------------------------------------------

  function css(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function isDark() {
    const theme = document.documentElement.getAttribute("data-theme");
    return theme ? theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  }

  function percent(value) {
    return (value > 0 ? "+" : "") + value.toFixed(2) + "%";
  }

  function stamp(t) {
    return new Date(t).toISOString().slice(0, monthly ? 7 : 10);
  }

  function escape(text) {
    return String(text).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  }

  // A published value short enough for a calendar cell: digits past the
  // second decimal are cut, not rounded, and marked with an ellipsis.
  function cellText(text) {
    const [whole, fraction] = text.split(".");
    return fraction && fraction.length > 2 ? whole + "." + fraction.slice(0, 2) + "…" : text;
  }

  function axisName() {
    if (state.mode === "level") return config.unit;
    if (state.mode === "yoy") return "% change on a year earlier";
    return monthly ? "% change on the month before" : "% change on the previous published day";
  }

  // Diverging scales are symmetric about zero, clipped at the 95th percentile
  // of the size of a change, so one outlier does not grey out every other
  // cell; the tooltip still has the exact value.
  function divergingBound(series) {
    const sizes = series.filter((v) => v != null && Number.isFinite(v)).map(Math.abs).sort((a, b) => a - b);
    return sizes.length ? sizes[Math.min(sizes.length - 1, Math.floor(sizes.length * 0.95))] || 1 : 1;
  }

  const ramp = (name) => css(name).split(",").map((c) => c.trim());

  // ---- Chart ---------------------------------------------------------------

  const chart = echarts.init(document.getElementById("chart"));

  function renderChart() {
    const [lo, hi] = periodRows();
    const ink = css("--foreground"), muted = css("--muted-foreground"), line = css("--border");
    const colours = [css("--series-1"), css("--series-2")];
    // Only the period's rows are drawn, so the y-axis fits the period rather
    // than the whole history.
    const slice = (series) => rows.time.slice(lo, hi + 1).map((t, j) => [t, series[lo + j]]);

    let series;
    if (state.mode === "change") {
      // Change as bars in the calendar's colours — red for a rise, blue for
      // a fall — so the two views read the same way.
      const div = ramp("--div");
      series = [{
        name: lines[0].label + " change",
        type: "bar",
        barMaxWidth: 8,
        data: slice(change[0]).map(([t, v]) => ({
          value: [t, v],
          itemStyle: { color: v == null ? line : v >= 0 ? div[4] : div[0] },
        })),
      }];
    } else {
      series = lines.map((l, k) => ({
        name: l.label,
        type: "line",
        showSymbol: false,
        connectNulls: false,
        lineStyle: { width: 2 },
        itemStyle: { color: colours[k] },
        emphasis: { focus: "series" },
        data: slice(valuesFor(state.mode)[k]),
      }));
    }

    chart.setOption({
      animation: false,
      textStyle: { color: ink, fontFamily: "inherit" },
      grid: { left: 8, right: 16, top: 40, bottom: 8, containLabel: true },
      legend: series.length > 1 ? { top: 0, right: 0, textStyle: { color: ink }, icon: "roundRect" } : { show: false },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "line", lineStyle: { color: muted } },
        backgroundColor: css("--card"),
        borderColor: line,
        textStyle: { color: ink },
        formatter: (points) => (points.length ? tooltip(lo + points[0].dataIndex) : ""),
      },
      xAxis: {
        type: "time",
        min: state.from,
        max: state.to,
        axisLine: { lineStyle: { color: line } },
        axisLabel: { color: muted, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: state.mode === "level",
        name: axisName(),
        nameLocation: "end",
        nameTextStyle: { color: muted, align: "left" },
        axisLabel: { color: muted, formatter: state.mode === "level" ? undefined : "{value}%" },
        splitLine: { lineStyle: { color: line } },
      },
      series,
    }, true);
  }

  // ---- Calendar ------------------------------------------------------------

  const calendar = echarts.init(document.getElementById("calendar"));

  function renderCalendar() {
    const coloured = valuesFor(state.mode)[0];
    const diverging = state.mode !== "level";
    const [lo, hi] = periodRows();
    const inPeriod = (i) => i >= lo && i <= hi;
    // Level scales to the period, so movement within it shows; a change
    // scales to the whole series, so a calm stretch looks calm.
    const shown = coloured.slice(lo, hi + 1).filter((v) => v != null);
    const bound = divergingBound(coloured);
    const range = diverging ? [-bound, bound]
      : shown.length ? [Math.min(...shown), Math.max(...shown)] : [0, 1];
    const muted = css("--muted-foreground"), line = css("--border"), background = css("--background");

    const base = {
      animation: false,
      textStyle: { color: css("--foreground"), fontFamily: "inherit" },
      tooltip: {
        backgroundColor: css("--card"),
        borderColor: line,
        textStyle: { color: css("--foreground") },
        formatter: (p) => {
          const data = Array.isArray(p.data) ? p.data : p.data.value;
          return tooltip(data[data.length - 1]);
        },
      },
      visualMap: {
        type: "continuous",
        seriesIndex: [0, 1],
        dimension: monthly ? 2 : 1,
        min: range[0],
        max: range[1],
        calculable: false,
        orient: "horizontal",
        left: monthly ? 52 : 56,
        bottom: 0,
        itemHeight: 160,
        inRange: { color: ramp(diverging ? "--div" : "--seq") },
        text: diverging ? ["rose", "fell"] : ["high", "low"],
        textStyle: { color: muted },
        formatter: (v) => (diverging ? percent(v) : v.toFixed(2)),
      },
    };

    if (monthly) {
      renderMonthGrid(base, coloured, inPeriod, range, diverging, { muted, line, background });
    } else {
      renderDecade(base, coloured, inPeriod, { muted, line, background });
    }
  }

  // A daily series as GitHub draws contributions: one strip per year the
  // period touches, newest on top, at most a decade of them, ending where the
  // period ends. Cells are sized to fill the width, as the chart does. Days
  // in those years but outside the period are faded rather than hidden, so
  // the period reads in context.
  function renderDecade(base, coloured, inPeriod, { muted, line, background }) {
    const endYear = yearOf(state.to);
    const startYear = Math.max(yearOf(state.from), endYear - 9, firstYear);
    const years = [];
    for (let y = endYear; y >= startYear; y -= 1) years.push(y);

    const left = 56, gap = 10, monthRow = 24;
    const width = Math.max(calendar.getDom().clientWidth, 640);
    const cell = Math.max(8, Math.min(20, Math.floor((width - left - 8) / 54)));
    const strip = 7 * cell;

    const calendars = years.map((year, k) => ({
      // No `right`: given both edges, ECharts stretches cells into bars.
      top: monthRow + k * (strip + gap),
      left,
      range: String(year),
      cellSize: [cell, cell],
      splitLine: { lineStyle: { color: muted, width: 1, opacity: 0.4 } },
      // A day with no value is an outline on the page background, so it can
      // never be read as the grey of "no change".
      itemStyle: { color: background, borderColor: line, borderWidth: 1 },
      yearLabel: { show: true, position: "left", margin: 28, color: muted, fontSize: 12 },
      monthLabel: { show: k === 0, color: muted, nameMap: "en" },
      dayLabel: { color: muted, firstDay: 1, fontSize: 9, nameMap: ["", "M", "", "W", "", "F", ""] },
    }));

    // Per year, three series: coloured in the period, coloured but faded
    // outside it, and grey for the first day, which has nothing to change from.
    const series = [];
    years.forEach((year, k) => {
      const buckets = [[], [], []];
      rows.keyed.forEach((r, i) => {
        if (Number(r[0].slice(0, 4)) !== year) return;
        const v = coloured[i];
        buckets[v == null ? 2 : inPeriod(i) ? 0 : 1].push([r[0], v == null ? 0 : v, i]);
      });
      buckets.forEach((data, b) => series.push({
        type: "heatmap",
        coordinateSystem: "calendar",
        calendarIndex: k,
        data,
        itemStyle: b === 2
          ? { color: line, borderColor: background, borderWidth: 1 }
          : { borderColor: background, borderWidth: 1, opacity: b === 1 ? 0.25 : 1 },
      }));
    });
    const visualMap = visualMapFor(base, 1, series.map((_, i) => i).filter((i) => i % 3 !== 2));

    calendar.getDom().style.height = monthRow + years.length * strip + (years.length - 1) * gap + 56 + "px";
    calendar.resize();
    calendar.setOption({ ...base, visualMap, calendar: calendars, series }, true);
  }

  function visualMapFor(base, dimension, seriesIndex) {
    return { ...base.visualMap, dimension, seriesIndex };
  }

  // A monthly series: a row per year in the period, newest on top, twelve
  // months across, each cell its published value.
  function renderMonthGrid(base, coloured, inPeriod, range, diverging, { muted, line, background }) {
    const years = [];
    for (let y = yearOf(state.to); y >= yearOf(state.from); y -= 1) years.push(String(y));
    const compact = years.length > 20;
    const labelInk = (v) => {
      const t = Math.max(0, Math.min(1, (v - range[0]) / (range[1] - range[0] || 1)));
      const strong = (diverging ? Math.abs(2 * t - 1) : t) > 0.6;
      return strong !== isDark() ? "#fafafa" : "#09090b";
    };
    const buckets = [[], [], []];
    rows.keyed.forEach((r, i) => {
      const [y, m] = r[0].split("-");
      const row = years.indexOf(y);
      if (row < 0) return;
      const v = coloured[i];
      buckets[v == null ? 2 : inPeriod(i) ? 0 : 1].push({
        value: [Number(m) - 1, row, v == null ? 0 : v, i],
        label: { color: v == null ? css("--foreground") : labelInk(v) },
      });
    });
    const label = {
      show: true,
      fontSize: compact ? 10 : 11,
      formatter: (p) => cellText(rows.keyed[p.data.value[3]][lines[0].index]),
    };
    const series = buckets.map((data, b) => ({
      type: "heatmap",
      data,
      label,
      itemStyle: b === 2
        ? { color: line, borderColor: background, borderWidth: 2 }
        : { borderColor: background, borderWidth: 2, opacity: b === 1 ? 0.25 : 1 },
    }));

    calendar.getDom().style.height = (compact ? 22 : 30) * years.length + 96 + "px";
    calendar.resize();
    calendar.setOption({
      ...base,
      visualMap: visualMapFor(base, 2, [0, 1]),
      grid: { top: 24, left: 52, right: 8, bottom: 64 },
      xAxis: { type: "category", data: MONTHS, position: "top", axisLine: { show: false },
        axisTick: { show: false }, axisLabel: { color: muted } },
      // Category axes run bottom-up; inverted, the newest year is on top.
      yAxis: { type: "category", data: years, inverse: true, axisLine: { show: false },
        axisTick: { show: false }, axisLabel: { color: muted } },
      series,
    }, true);
  }

  // Every published value of the row, as published, then the computed one.
  function tooltip(index) {
    const row = rows.keyed[index];
    const parts = lines.map((l) => escape(l.label) + ": <b>" + escape(row[l.index]) + "</b>");
    if (state.mode !== "level") {
      const v = valuesFor(state.mode)[0][index];
      const what = state.mode === "yoy" ? "on a year earlier"
        : index > 0 ? "since " + rows.keyed[index - 1][0] : "";
      parts.push(v == null ? "No earlier value to compare" : "Change " + escape(what) + ": <b>" + percent(v) + "</b>");
    }
    return "<div>" + escape(row[0]) + "</div>" + parts.join("<br>");
  }

  // ---- Period: presets, the slider, and its summary -------------------------

  const nav = echarts.init(document.getElementById("navigator"));
  let steering = false;

  // A slim overview of the whole series with a slider over it: the same
  // control under the chart and under the calendar.
  function renderNavigator() {
    const muted = css("--muted-foreground"), line = css("--border");
    steering = true;
    nav.setOption({
      animation: false,
      grid: { left: 0, right: 0, top: 0, bottom: 0 },
      xAxis: { type: "time", min: first, max: last, show: false },
      yAxis: { type: "value", scale: true, show: false },
      series: [{ type: "line", showSymbol: false, silent: true, lineStyle: { opacity: 0 },
        data: rows.time.map((t, i) => [t, level[0][i]]) }],
      dataZoom: [{
        type: "slider",
        xAxisIndex: 0,
        startValue: state.from,
        endValue: state.to,
        left: 8, right: 8, top: 4, bottom: 4,
        brushSelect: false,
        borderColor: line,
        textStyle: { color: muted },
        labelFormatter: (v) => stamp(v),
      }],
    }, true);
    steering = false;
  }

  nav.on("datazoom", () => {
    if (steering) return;
    const zoom = nav.getOption().dataZoom[0];
    const width = last - first;
    state.from = first + width * (zoom.start / 100);
    state.to = first + width * (zoom.end / 100);
    state.preset = null;
    press(document.getElementById("ranges"), null);
    schedule();
  });

  // The period's own summary: each line's first and last published value in
  // it, the change between them, and that change as a compound annual rate.
  function renderStats() {
    const [lo, hi] = periodRows();
    const period = document.getElementById("period");
    const stats = document.getElementById("stats");
    if (lo > hi) {
      period.textContent = "No published values in this period.";
      stats.innerHTML = "";
      return;
    }
    const years = (rows.time[hi] - rows.time[lo]) / YEAR;
    period.textContent = rows.keyed[lo][0] + " → " + rows.keyed[hi][0] + " · " +
      (years >= 1 ? years.toFixed(1) + " years" : Math.round(years * 365.2425) + " days") +
      " · " + (hi - lo + 1) + " rows";
    stats.innerHTML = lines.map((l, k) => {
      const a = level[k][lo], b = level[k][hi];
      const total = (b / a - 1) * 100;
      // Annualised pro rata over whatever span is chosen, a few weeks
      // included. A period with no length (a single row) has no rate.
      const cagr = years > 0 ? percent((Math.pow(b / a, 1 / years) - 1) * 100) + " a year" : "—";
      const item = (name, value) => '<span class="stat-item"><span>' + name + "</span>" + value + "</span>";
      return '<div class="stat"><span class="stat-name">' + escape(l.label) + "</span>" +
        item("From", escape(rows.keyed[lo][l.index]) + " → " + escape(rows.keyed[hi][l.index])) +
        item("Change", percent(total)) +
        item("CAGR", cagr) + "</div>";
    }).join("");
  }

  function renderNote() {
    const parts = ["Change and CAGR are computed in your browser from the published values; they are not themselves published."];
    if (state.view === "calendar") {
      parts.push(monthly
        ? "Cells show the published value of " + lines[0].label + "; their colour follows Values."
        : "Colour follows " + lines[0].label + ". Empty squares are days with no published value: weekends, bank holidays, or days the source did not publish. Faded squares are outside the chosen period. At most a decade is drawn; drag the period back for earlier years.");
    } else if (state.mode === "change" && lines.length > 1) {
      parts.push("Change is drawn for " + lines[0].label + ".");
    }
    document.getElementById("note").textContent = parts.join(" ");
  }

  // ---- Controls ------------------------------------------------------------

  function press(group, value) {
    group.querySelectorAll("button").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.value === value)));
  }

  function buttons(id, options, current, onPick) {
    const group = document.getElementById(id);
    group.innerHTML = options.map(([value, label]) =>
      '<button type="button" class="btn btn-outline" data-value="' + value + '">' + escape(label) + "</button>").join("");
    group.addEventListener("click", (e) => {
      const button = e.target.closest("button");
      if (!button) return;
      press(group, button.dataset.value);
      onPick(button.dataset.value);
    });
    press(group, current == null ? null : String(current));
  }

  buttons("views", [["chart", "Chart"], ["calendar", "Calendar"]], state.view, (v) => { state.view = v; render(); });
  buttons("modes", MODES, state.mode, (v) => { state.mode = v; render(); });
  buttons("ranges", RANGES, state.preset, (v) => {
    setPreset(Number(v));
    renderNavigator();
    render();
  });

  // Exact start and end days. They follow the presets and the slider, and
  // setting either one makes the period custom.
  const fromInput = document.getElementById("from");
  const toInput = document.getElementById("to");
  for (const input of [fromInput, toInput]) {
    input.min = ymd(first);
    input.max = ymd(last);
    input.addEventListener("change", () => {
      const from = parseDay(fromInput.value), to = parseDay(toInput.value);
      if (from == null || to == null) return;
      setPeriod(from, to);
      press(document.getElementById("ranges"), null);
      renderNavigator();
      render();
    });
  }

  function copyButton() {
    const button = document.getElementById("copy-url");
    button.addEventListener("click", async () => {
      try {
        await window.navigator.clipboard.writeText(button.dataset.url);
        button.textContent = "Copied";
      } catch {
        button.textContent = "Copy failed";
      }
      setTimeout(() => { button.textContent = "Copy URL"; }, 1500);
    });
  }

  let pending = false;
  function schedule() {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => { pending = false; render(); });
  }

  function render() {
    const onChart = state.view === "chart";
    document.getElementById("chart-panel").hidden = !onChart;
    document.getElementById("calendar-panel").hidden = onChart;
    if (onChart) {
      renderChart();
      chart.resize();
    } else {
      renderCalendar();
    }
    renderStats();
    renderNote();
    fromInput.value = ymd(state.from);
    toInput.value = ymd(state.to);
    writeUrl();
  }

  window.addEventListener("resize", () => { nav.resize(); render(); });
  // Colours come from the stylesheet, so a theme change needs a redraw.
  window.addEventListener("themechange", () => { renderNavigator(); render(); });

  status.textContent = "";
  status.hidden = true;
  renderNavigator();
  render();
})();
