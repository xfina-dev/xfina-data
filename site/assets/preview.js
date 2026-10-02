// A dataset's page: its published CSV as a chart or a calendar.
//
// Configured per dataset from the `preview` block in datasets.yaml, embedded
// in the page as JSON. Tooltips and calendar cells show the CSV's own text, so
// a value reads exactly as published (90.50 stays 90.50). Numbers are parsed
// only to place points and pick colours, and every number computed here — a
// change, a year-on-year rate — is labelled as computed and never published.

(async () => {
  const config = JSON.parse(document.getElementById("config").textContent);
  const status = document.getElementById("status");
  const RANGES = [["1Y", 1], ["5Y", 5], ["10Y", 10], ["All", 0]];
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const monthly = config.frequency === "monthly";
  const yoyOffered = config.views.includes("year-on-year");

  let rows;
  try {
    const response = await fetch("/" + config.path);
    if (!response.ok) throw new Error("HTTP " + response.status);
    rows = parse(await response.text());
  } catch (error) {
    status.textContent = "Could not load " + config.path + ": " + error.message;
    return;
  }

  // The first drawn column is the one a calendar colours by.
  const colourColumn = rows.columns.indexOf(config.lines[0].column);
  const lastYear = Number(rows.keyed[rows.keyed.length - 1][0].slice(0, 4));
  const firstYear = Number(rows.keyed[0][0].slice(0, 4));

  const modes = monthly
    ? [["mom", "Month-on-month"]].concat(yoyOffered ? [["yoy", "Year-on-year"]] : []).concat([["level", "Level"]])
    : [["change", "Daily change"], ["level", "Level"]];

  const state = {
    tab: "chart",
    view: "level",
    range: monthly ? 0 : 5,
    mode: modes[0][0],
    year: lastYear,
    span: 1,
  };
  const chart = echarts.init(document.getElementById("chart"));
  const calendar = echarts.init(document.getElementById("calendar"));

  // The published CSVs are written by this project and never quote a field,
  // so splitting on commas is the whole parser.
  function parse(text) {
    const [header, ...lines] = text.trim().split("\n");
    const columns = header.split(",");
    const keyed = lines.map((line) => line.split(","));
    return { columns, keyed, time: keyed.map((r) => toTime(r[0])) };
  }

  function toTime(key) {
    const [y, m, d] = key.split("-").map(Number);
    return Date.UTC(y, m - 1, d || 1);
  }

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

  // Year-on-year change for a monthly series: this month over the same month
  // a year earlier. Only where both months are published; never filled.
  function yearOnYear(index) {
    const byKey = new Map(rows.keyed.map((r) => [r[0], Number(r[index])]));
    return rows.keyed.map((r) => {
      const [y, m] = r[0].split("-");
      const before = byKey.get(String(Number(y) - 1) + "-" + m);
      return before ? (Number(r[index]) / before - 1) * 100 : null;
    });
  }

  // Change on the previous published row: the previous trading day for a
  // daily series, the previous month for a monthly one. A monthly series has
  // no gaps (the tool refuses one), so the previous row is the previous month.
  function previousChange(index) {
    return rows.keyed.map((r, i) =>
      i === 0 ? null : (Number(r[index]) / Number(rows.keyed[i - 1][index]) - 1) * 100);
  }

  // ---- Chart ---------------------------------------------------------------

  function renderChart() {
    const yoy = state.view === "yoy";
    const colours = [css("--series-1"), css("--series-2")];
    const series = config.lines.map((line, i) => {
      const index = rows.columns.indexOf(line.column);
      const values = yoy ? yearOnYear(index) : rows.keyed.map((r) => Number(r[index]));
      return {
        name: line.label,
        type: "line",
        showSymbol: false,
        connectNulls: false,
        lineStyle: { width: 2 },
        itemStyle: { color: colours[i] },
        emphasis: { focus: "series" },
        data: rows.time.map((t, j) => [t, values[j]]),
      };
    });

    const last = rows.time[rows.time.length - 1];
    const lastDate = new Date(last);
    const start = state.range
      ? Date.UTC(lastDate.getUTCFullYear() - state.range, lastDate.getUTCMonth(), lastDate.getUTCDate())
      : rows.time[0];
    const ink = css("--foreground"), muted = css("--muted-foreground"), line = css("--border");

    chart.setOption({
      animation: false,
      textStyle: { color: ink, fontFamily: "inherit" },
      grid: { left: 8, right: 16, top: 40, bottom: 56, containLabel: true },
      legend: config.lines.length > 1 ? { top: 0, right: 0, textStyle: { color: ink }, icon: "roundRect" } : { show: false },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "line", lineStyle: { color: muted } },
        backgroundColor: css("--card"),
        borderColor: line,
        textStyle: { color: ink },
        formatter: (points) => chartTooltip(points, yoy),
      },
      xAxis: {
        type: "time",
        axisLine: { lineStyle: { color: line } },
        axisLabel: { color: muted, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: !yoy,
        name: yoy ? "% change on a year earlier" : config.unit,
        nameLocation: "end",
        nameTextStyle: { color: muted, align: "left" },
        axisLabel: { color: muted, formatter: yoy ? "{value}%" : undefined },
        splitLine: { lineStyle: { color: line } },
      },
      dataZoom: [
        { type: "slider", startValue: start, endValue: last, height: 22, bottom: 8,
          borderColor: line, textStyle: { color: muted }, brushSelect: false },
      ],
      series,
    }, true);

    const note = document.getElementById("chart-note");
    note.hidden = !yoy;
    note.textContent = yoy
      ? "Computed in your browser from the published index; not itself a published value. Months without a value a year earlier are left blank."
      : "";
  }

  function chartTooltip(points, yoy) {
    if (!points.length) return "";
    const row = rows.keyed[points[0].dataIndex];
    const lines = points.map((p) => {
      const index = rows.columns.indexOf(config.lines[p.seriesIndex].column);
      const value = yoy
        ? (p.value[1] == null ? "—" : p.value[1].toFixed(2) + "%")
        : row[index];
      return p.marker + " " + escape(p.seriesName) + ": <b>" + escape(value) + "</b>";
    });
    return "<div>" + escape(row[0]) + "</div>" + lines.join("<br>");
  }

  // ---- Calendar ------------------------------------------------------------

  // What colours a cell, for every row: [value or null, scale kind, legend].
  function colouring() {
    if (state.mode === "level") {
      return { values: rows.keyed.map((r) => Number(r[colourColumn])), diverging: false };
    }
    if (state.mode === "yoy") return { values: yearOnYear(colourColumn), diverging: true };
    return { values: previousChange(colourColumn), diverging: true };
  }

  // Diverging scales are symmetric about zero, clipped at the 95th percentile
  // of the size of a change so one outlier does not wash every other cell to
  // grey. A clipped cell takes the end colour; its tooltip has the exact value.
  function scale(values, diverging) {
    const present = values.filter((v) => v != null && Number.isFinite(v));
    if (!present.length) return [0, 1];
    if (!diverging) return [Math.min(...present), Math.max(...present)];
    const sizes = present.map(Math.abs).sort((a, b) => a - b);
    const bound = sizes[Math.min(sizes.length - 1, Math.floor(sizes.length * 0.95))] || 1;
    return [-bound, bound];
  }

  // Text on a strong cell flips to the opposite ink so it stays readable.
  function labelInk(value, [min, max], diverging) {
    const t = Math.max(0, Math.min(1, (value - min) / (max - min || 1)));
    const strength = diverging ? Math.abs(2 * t - 1) : t;
    const strong = strength > 0.6;
    return strong !== isDark() ? "#fafafa" : "#09090b";
  }

  function modeLabel() {
    const column = config.lines[0].label;
    if (state.mode === "level") return column;
    if (state.mode === "yoy") return column + ", change on a year earlier";
    if (state.mode === "mom") return column + ", change on the month before";
    return column + ", change on the previous published day";
  }

  function renderCalendar() {
    const { values, diverging } = colouring();
    const ramp = css(diverging ? "--div" : "--seq").split(",").map((c) => c.trim());
    const ink = css("--foreground"), muted = css("--muted-foreground"), line = css("--border");

    const base = {
      animation: false,
      textStyle: { color: ink, fontFamily: "inherit" },
      tooltip: {
        backgroundColor: css("--card"),
        borderColor: line,
        textStyle: { color: ink },
        formatter: (p) => {
          const data = Array.isArray(p.data) ? p.data : p.data.value;
          return calendarTooltip(data[data.length - 1], values);
        },
      },
    };

    if (monthly) {
      renderMonthGrid(base, values, diverging, ramp, { muted, line });
    } else {
      renderYears(base, values, diverging, ramp, { muted, line });
    }

    document.getElementById("calendar-note").textContent =
      "Colour: " + modeLabel() + ". " +
      (state.mode === "level" ? "" : "Changes are computed in your browser from the published values and are not themselves published. ") +
      (monthly
        ? "Cells show the published value."
        : "Empty squares are days with no published rate: weekends, bank holidays, or days the source did not publish.");
  }

  // `dimension` is where the colour value sits in a data item: [day, value, row]
  // on a calendar, [month, year, value, row] on the month grid.
  function visualMap(range, diverging, ramp, muted, dimension, seriesIndex = 0) {
    return {
      type: "continuous",
      seriesIndex,
      dimension,
      min: range[0],
      max: range[1],
      calculable: false,
      orient: "horizontal",
      // Left-aligned, so it is in view on a phone before the calendar is
      // scrolled sideways.
      left: dimension === 1 ? 36 : 52,
      bottom: 0,
      itemHeight: 160,
      inRange: { color: ramp },
      text: diverging ? ["rose", "fell"] : ["high", "low"],
      textStyle: { color: muted },
      formatter: (v) => (diverging ? percent(v) : v.toFixed(2)),
    };
  }

  // A daily series as GitHub draws contributions: a square per day, one strip
  // per year, newest on top. A year, or a decade of strips at a smaller size.
  function renderYears(base, values, diverging, ramp, { muted, line }) {
    const span = state.span;
    const years = [];
    for (let y = state.year; y > state.year - span && y >= firstYear; y -= 1) years.push(y);
    const shown = rows.keyed.map((r, i) => i)
      .filter((i) => years.includes(Number(rows.keyed[i][0].slice(0, 4))));
    // Level scales to the years shown, so movement within them is visible;
    // changes scale to the whole series, so a calm year looks calm.
    const range = diverging ? scale(values, true) : scale(shown.map((i) => values[i]), false);

    // Square cells, as large as fit across 53 weeks, smaller for a decade so
    // ten strips stay on one screen.
    const left = span > 1 ? 64 : 36, right = 8, monthRow = 24, gap = span > 1 ? 14 : 0;
    const width = calendar.getDom().parentElement.clientWidth;
    const cell = Math.max(8, Math.min(span > 1 ? 14 : 20, Math.floor((Math.max(width, 760) - left - right) / 54)));
    const strip = 7 * cell;

    const calendars = years.map((year, k) => ({
      // No `right`: given both edges, ECharts stretches cells to fill the
      // width and squares become bars. The cell size alone sets the width.
      top: monthRow + k * (strip + gap),
      left,
      range: String(year),
      cellSize: [cell, cell],
      splitLine: { lineStyle: { color: muted, width: 1, opacity: 0.4 } },
      // A day with no value is an outline on the page background, so it can
      // never be read as the grey of "no change".
      itemStyle: { color: css("--background"), borderColor: css("--border"), borderWidth: 1 },
      yearLabel: { show: span > 1, position: "left", margin: 28, color: muted, fontSize: 12 },
      monthLabel: { show: k === 0, color: muted, nameMap: "en" },
      dayLabel: {
        color: muted, firstDay: 1, fontSize: span > 1 ? 9 : 12,
        nameMap: span > 1 ? ["", "M", "", "W", "", "F", ""] : ["S", "M", "T", "W", "T", "F", "S"],
      },
    }));

    const series = [];
    years.forEach((year, k) => {
      const inYear = shown.filter((i) => rows.keyed[i][0].startsWith(String(year)));
      series.push({ type: "heatmap", coordinateSystem: "calendar", calendarIndex: k,
        data: inYear.filter((i) => values[i] != null).map((i) => [rows.keyed[i][0], values[i], i]),
        itemStyle: { borderColor: css("--background"), borderWidth: span > 1 ? 1 : 2 } });
      // The first day of the series has no previous day to change from: it
      // is drawn, so it is not mistaken for a missing day, but not coloured.
      series.push({ type: "heatmap", coordinateSystem: "calendar", calendarIndex: k,
        data: inYear.filter((i) => values[i] == null).map((i) => [rows.keyed[i][0], 0, i]),
        itemStyle: { color: line, borderColor: css("--background"), borderWidth: span > 1 ? 1 : 2 } });
    });

    calendar.getDom().style.height = monthRow + years.length * strip + (years.length - 1) * gap + 64 + "px";
    calendar.resize();
    calendar.setOption(Object.assign(base, {
      calendar: calendars,
      visualMap: visualMap(range, diverging, ramp, muted, 1, series.map((_, i) => i).filter((i) => i % 2 === 0)),
      series,
    }), true);

    const oldest = years[years.length - 1];
    document.getElementById("year-label").textContent =
      span > 1 ? oldest + "–" + state.year : String(state.year);
    document.getElementById("prev-year").disabled = oldest <= firstYear;
    document.getElementById("next-year").disabled = state.year >= lastYear;
    document.getElementById("prev-year").title = span > 1 ? "Previous decade" : "Previous year";
    document.getElementById("next-year").title = span > 1 ? "Next decade" : "Next year";
  }

  // A monthly series: years down, months across, each cell its published value.
  function renderMonthGrid(base, values, diverging, ramp, { muted, line }) {
    const years = [];
    for (let y = lastYear; y >= firstYear; y -= 1) years.push(String(y));
    const range = scale(values, diverging);
    const cells = rows.keyed.map((r, i) => {
      const [y, m] = r[0].split("-");
      const value = values[i];
      const coloured = value != null;
      return {
        value: [Number(m) - 1, years.indexOf(y), coloured ? value : 0, i],
        label: { color: coloured ? labelInk(value, range, diverging) : css("--foreground") },
        itemStyle: coloured ? undefined : { color: line },
      };
    });
    const coloured = cells.filter((c, i) => values[i] != null);
    const uncoloured = cells.filter((c, i) => values[i] == null);

    // Shorter rows for a long series, so seventy years stay scrollable
    // rather than becoming a page of their own.
    const compact = years.length > 20;
    calendar.getDom().style.height = (compact ? 22 : 30) * years.length + 96 + "px";
    calendar.resize();
    calendar.setOption(Object.assign(base, {
      grid: { top: 24, left: 52, right: 8, bottom: 64 },
      xAxis: { type: "category", data: MONTHS, position: "top", axisLine: { show: false },
        axisTick: { show: false }, axisLabel: { color: muted }, splitArea: { show: false } },
      // Category axes run bottom-up; inverted, the newest year is on top.
      yAxis: { type: "category", data: years, inverse: true, axisLine: { show: false }, axisTick: { show: false },
        axisLabel: { color: muted } },
      visualMap: visualMap(range, diverging, ramp, muted, 2),
      series: [
        { type: "heatmap", data: coloured,
          label: { show: true, fontSize: compact ? 10 : 11, formatter: (p) => cellText(rows.keyed[p.data.value[3]][colourColumn]) },
          itemStyle: { borderColor: css("--background"), borderWidth: 2 } },
        { type: "heatmap", data: uncoloured,
          label: { show: true, fontSize: compact ? 10 : 11, formatter: (p) => cellText(rows.keyed[p.data.value[3]][colourColumn]) },
          itemStyle: { color: line, borderColor: css("--background"), borderWidth: 2 } },
      ],
    }), true);
  }

  // A published value short enough for a calendar cell. Digits past the
  // second decimal are cut, not rounded, and the cut is marked with an
  // ellipsis; the tooltip always has the whole text.
  function cellText(text) {
    const [whole, fraction] = text.split(".");
    return fraction && fraction.length > 2 ? whole + "." + fraction.slice(0, 2) + "…" : text;
  }

  // Every published value of the row, as published, then the computed number.
  function calendarTooltip(index, values) {
    const row = rows.keyed[index];
    const lines = config.lines.map((l) =>
      escape(l.label) + ": <b>" + escape(row[rows.columns.indexOf(l.column)]) + "</b>");
    if (state.mode !== "level") {
      const value = values[index];
      const since = index > 0 ? rows.keyed[index - 1][0] : null;
      const what = state.mode === "yoy" ? "on a year earlier"
        : state.mode === "mom" ? "on the month before" : "since " + since;
      lines.push(value == null ? "<span>No earlier value to compare</span>"
        : "Change " + escape(what) + ": <b>" + percent(value) + "</b>");
    }
    return "<div>" + escape(row[0]) + "</div>" + lines.join("<br>");
  }

  // ---- Controls ------------------------------------------------------------

  function escape(text) {
    return String(text).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  }

  function buttons(group, options, current, onPick) {
    group.innerHTML = options.map(([value, label]) =>
      '<button type="button" class="btn btn-outline" data-value="' + value + '">' + escape(label) + "</button>").join("");
    const press = (value) => group.querySelectorAll("button").forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.value === value)));
    group.addEventListener("click", (e) => {
      const button = e.target.closest("button");
      if (!button) return;
      press(button.dataset.value);
      onPick(button.dataset.value);
    });
    press(String(current));
  }

  function render() {
    const onChart = state.tab === "chart";
    document.getElementById("chart-panel").hidden = !onChart;
    document.getElementById("calendar-panel").hidden = onChart;
    document.getElementById("ranges").hidden = !onChart;
    document.getElementById("views").hidden = !onChart || !config.views.length;
    document.getElementById("modes").hidden = onChart;
    document.getElementById("year-nav").hidden = onChart || monthly;
    document.getElementById("spans").hidden = onChart || monthly;
    document.querySelectorAll("[data-tab]").forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.tab === state.tab)));
    if (onChart) {
      renderChart();
      chart.resize();
    } else {
      renderCalendar();
    }
  }

  buttons(document.getElementById("ranges"), RANGES.map(([l, y]) => [String(y), l]), state.range,
    (v) => { state.range = Number(v); renderChart(); });
  if (config.views.length) {
    buttons(document.getElementById("views"),
      [["level", config.lines.length > 1 ? "Values" : config.lines[0].label]].concat(yoyOffered ? [["yoy", "YoY %"]] : []),
      state.view, (v) => { state.view = v; renderChart(); });
  }
  buttons(document.getElementById("modes"), modes, state.mode, (v) => { state.mode = v; renderCalendar(); });

  document.querySelectorAll("[data-tab]").forEach((b) =>
    b.addEventListener("click", () => { state.tab = b.dataset.tab; render(); }));
  buttons(document.getElementById("spans"), [["1", "Year"], ["10", "Decade"]], state.span,
    (v) => { state.span = Number(v); renderCalendar(); });
  // Arrows step by what is shown: a year at a time, or a decade.
  document.getElementById("prev-year").addEventListener("click", () => {
    state.year -= state.span; renderCalendar();
  });
  document.getElementById("next-year").addEventListener("click", () => {
    state.year = Math.min(lastYear, state.year + state.span); renderCalendar();
  });
  window.addEventListener("resize", () => { chart.resize(); calendar.resize(); });
  // Colours are read from the stylesheet, so a theme change needs a redraw.
  // theme.js announces both the toggle and an OS change it is following.
  window.addEventListener("themechange", render);

  status.textContent = "";
  status.hidden = true;
  render();
})();
