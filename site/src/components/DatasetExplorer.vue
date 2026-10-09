<script setup>
// A dataset's published CSV as a chart or a calendar.
//
// The two views are twins. They share one set of controls (what to show: the
// level or its change; which period, chosen by preset, by date, or by
// dragging the slider under either view) and one summary of that period, CAGR
// included for a level (a rate changes in points and has none). Switching view keeps everything else as it was.
//
// The controls are in the static HTML, built from the dataset's first and last
// period in site-data.json. The CSV is fetched, and everything drawn, in the
// browser. ECharts is loaded only there, so the static build never runs it.
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from "vue";
import { Input, Segmented } from "xfina-ui";
import {
  customPeriod,
  defaultPreset,
  derive,
  modes,
  parse,
  parseDay,
  presetPeriod,
  ranges,
  readQuery,
  summary,
  toTime,
  writeQuery,
  ymd,
  periodRows,
} from "../explorer/series.js";
import { renderCalendar, renderChart, renderNavigator } from "../explorer/render.js";

const props = defineProps({ dataset: { type: Object, required: true } });

const monthly = props.dataset.frequency === "monthly";
const first = toTime(props.dataset.published.first);
const last = toTime(props.dataset.published.last);
const MODES = modes(props.dataset.views);
const RANGES = ranges(first, last);
const DEFAULT_YEARS = defaultPreset(first, last, monthly);
const VIEWS = [
  { value: "chart", label: "Chart" },
  { value: "calendar", label: "Calendar" },
];

const state = reactive({ view: "chart", mode: "level", ...presetPeriod(DEFAULT_YEARS, first, last) });

const loaded = shallowRef(null); // { rows, derived } once the CSV is in
const status = ref("Loading…");

// ---- Controls --------------------------------------------------------------

const modeModel = computed({ get: () => state.mode, set: (v) => (state.mode = v) });
const viewModel = computed({ get: () => state.view, set: (v) => (state.view = v) });
// No preset is pressed while the period is custom.
const rangeModel = computed({
  get: () => (state.preset == null ? "" : String(state.preset)),
  set: (v) => {
    Object.assign(state, presetPeriod(Number(v), first, last));
    steerNavigator();
  },
});
// Exact start and end days. They follow the presets and the slider, and
// setting either one makes the period custom.
function setDay(which, text) {
  const day = parseDay(text);
  if (day == null) return;
  const next = which === "from" ? [day, state.to] : [state.from, day];
  Object.assign(state, customPeriod(next[0], next[1], first, last, monthly));
  steerNavigator();
}
const fromModel = computed({ get: () => ymd(state.from), set: (v) => setDay("from", v) });
const toModel = computed({ get: () => ymd(state.to), set: (v) => setDay("to", v) });

const period = computed(() => {
  if (!loaded.value) return null;
  const { rows, derived } = loaded.value;
  const [lo, hi] = periodRows(rows.time, state.from, state.to);
  return summary(rows, derived, lo, hi) ?? { text: "No published values in this period.", stats: [] };
});

const note = computed(() => {
  const lines = props.dataset.lines;
  const parts = [
    props.dataset.change === "points"
      ? "Change, in percentage points, is computed in your browser from the published values; it is not itself published."
      : "Change and CAGR are computed in your browser from the published values; they are not themselves published.",
  ];
  if (state.view === "calendar") {
    parts.push(
      monthly
        ? `Cells show the published value of ${lines[0].label}; their colour follows Values.`
        : `Colour follows ${lines[0].label}. Empty squares are days with no published value: weekends, bank holidays, or days the source did not publish. Faded squares are outside the chosen period. At most a decade is drawn; drag the period back for earlier years.`,
    );
  } else if (state.mode === "change" && lines.length > 1) {
    parts.push(`Change is drawn for ${lines[0].label}.`);
  }
  return parts.join(" ");
});

// ---- Drawing ---------------------------------------------------------------

const chartEl = ref(null);
const calendarEl = ref(null);
const navigatorEl = ref(null);
let plots = null;
let steering = false;

const context = () => ({
  ...loaded.value,
  state,
  monthly,
  unit: props.dataset.unit,
  dark: window.xfinaTheme.isDark(),
});

function draw() {
  if (!plots || !loaded.value) return;
  if (state.view === "chart") {
    renderChart(plots.chart, context());
    plots.chart.resize();
  } else {
    renderCalendar(plots.calendar, context());
  }
}

// The slider follows the presets and the dates; while it is being set, its own
// "datazoom" event is ignored, or setting it would read back as a drag.
function steerNavigator() {
  if (!plots || !loaded.value) return;
  steering = true;
  renderNavigator(plots.navigator, context());
  steering = false;
}

let pending = false;
function schedule() {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    draw();
  });
}

const redrawAll = () => {
  steerNavigator();
  draw();
};
const onResize = () => {
  plots?.navigator.resize();
  draw();
};

watch(
  () => [state.view, state.mode, state.from, state.to],
  async () => {
    await nextTick(); // the view's panel is shown before it is measured
    schedule();
    // The address carries what is on screen, so a view can be shared.
    history.replaceState(history.state, "", location.pathname + writeQuery(state, DEFAULT_YEARS));
  },
);

onMounted(async () => {
  Object.assign(state, readQuery(location.search, { modes: MODES, ranges: RANGES, first, last, monthly }));
  try {
    const [{ default: echarts }, response] = await Promise.all([
      import("../explorer/echarts.js"),
      fetch(`/${props.dataset.published.path}`),
    ]);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const rows = parse(await response.text());
    loaded.value = { rows, derived: derive(rows, props.dataset) };
    plots = {
      chart: echarts.init(chartEl.value),
      calendar: echarts.init(calendarEl.value),
      navigator: echarts.init(navigatorEl.value),
    };
  } catch (error) {
    status.value = `Could not load ${props.dataset.published.path}: ${error.message}`;
    return;
  }
  status.value = "";

  plots.navigator.on("datazoom", () => {
    if (steering) return;
    const zoom = plots.navigator.getOption().dataZoom[0];
    const width = last - first;
    state.from = first + width * (zoom.start / 100);
    state.to = first + width * (zoom.end / 100);
    state.preset = null;
  });
  window.addEventListener("resize", onResize);
  // Colours come from the stylesheet, so a theme change needs a redraw.
  window.addEventListener("themechange", redrawAll);
  await nextTick();
  redrawAll();
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", onResize);
  window.removeEventListener("themechange", redrawAll);
  if (plots) Object.values(plots).forEach((p) => p.dispose());
});

const controlLabel = "mb-1.5 block text-xs font-semibold uppercase tracking-wide text-muted-foreground";
</script>

<template>
  <div>
    <div class="flex flex-wrap items-end gap-x-6 gap-y-3 px-6 max-sm:px-4" aria-label="Controls">
      <div>
        <span :class="controlLabel">View</span>
        <Segmented v-model="viewModel" :options="VIEWS" label="View" />
      </div>
      <div>
        <span :class="controlLabel">Values</span>
        <Segmented v-model="modeModel" :options="MODES.map(([value, label]) => ({ value, label }))" label="Values" />
      </div>
      <div>
        <span :class="controlLabel">Period</span>
        <Segmented v-model="rangeModel" :options="RANGES.map(([value, label]) => ({ value, label }))" label="Period" />
      </div>
      <div>
        <span :class="controlLabel">From – to</span>
        <div class="flex items-center gap-1.5 text-muted-foreground">
          <Input v-model="fromModel" type="date" class="h-9 w-auto tabular-nums" :min="ymd(first)" :max="ymd(last)" aria-label="Period starts" />
          <span aria-hidden="true">→</span>
          <Input v-model="toModel" type="date" class="h-9 w-auto tabular-nums" :min="ymd(first)" :max="ymd(last)" aria-label="Period ends" />
        </div>
      </div>
      <p v-if="period" class="pb-2 text-sm tabular-nums text-muted-foreground">{{ period.text }}</p>
    </div>

    <div v-if="period?.stats.length" class="flex flex-wrap gap-x-3 gap-y-2 px-6 pt-4 max-sm:px-4" aria-live="polite">
      <div
        v-for="s in period.stats"
        :key="s.label"
        class="flex flex-wrap items-baseline gap-x-4 gap-y-1 rounded-md border px-3 py-2 text-sm tabular-nums"
      >
        <span class="font-semibold">{{ s.label }}</span>
        <span><span class="mr-1.5 text-muted-foreground">From</span>{{ s.from }} → {{ s.to }}</span>
        <span><span class="mr-1.5 text-muted-foreground">Change</span>{{ s.change }}</span>
        <span v-if="s.cagr"><span class="mr-1.5 text-muted-foreground">CAGR</span>{{ s.cagr }}</span>
      </div>
    </div>

    <div class="p-6 max-sm:p-4">
      <div v-show="state.view === 'chart'">
        <div ref="chartEl" role="img" :aria-label="`${dataset.title}, chart`" class="h-[440px] w-full max-sm:h-80" />
      </div>
      <div v-show="state.view === 'calendar'" class="overflow-x-auto">
        <div ref="calendarEl" role="img" :aria-label="`${dataset.title}, calendar`" class="w-full min-w-[640px]" />
      </div>
      <div ref="navigatorEl" aria-label="Period: drag to choose" class="mt-2 h-12 w-full" />
      <p class="mt-2 text-xs text-muted-foreground">{{ note }}</p>
      <p v-if="status" role="status" class="mt-2 text-xs text-muted-foreground">{{ status }}</p>
    </div>
  </div>
</template>
