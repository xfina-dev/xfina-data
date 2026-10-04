<script setup>
// One dataset: its title, the CSV's address and download, its facts, the
// explorer, and where the data comes from. Everything but the explorer's
// drawing is in the static HTML.
import { computed, ref } from "vue";
import { useHead } from "@unhead/vue";
import { Button, Card, CardContent, CardHeader } from "xfina-ui";
import DatasetFacts from "../components/DatasetFacts.vue";
import DatasetExplorer from "../components/DatasetExplorer.vue";
import NotFoundPage from "./NotFoundPage.vue";
import { csvUrl, datasetById } from "../site.js";

const props = defineProps({ id: { type: String, required: true } });
const dataset = computed(() => {
  const d = datasetById(props.id);
  return d?.published ? d : null;
});

useHead(() =>
  dataset.value
    ? { title: `${dataset.value.title} · Xfina Data`, meta: [{ name: "description", content: dataset.value.summary }] }
    : {},
);

const copied = ref("Copy URL");
async function copy() {
  try {
    await navigator.clipboard.writeText(csvUrl(dataset.value));
    copied.value = "Copied";
  } catch {
    copied.value = "Copy failed";
  }
  setTimeout(() => (copied.value = "Copy URL"), 1500);
}
</script>

<template>
  <NotFoundPage v-if="!dataset" />
  <div v-else class="space-y-8">
    <Card>
      <CardHeader class="gap-2">
        <p class="text-sm text-muted-foreground">
          <RouterLink to="/" class="hover:text-foreground">All datasets</RouterLink> /
          <code>{{ dataset.id }}</code>
        </p>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <h1 class="text-2xl font-semibold leading-tight tracking-tight">{{ dataset.title }}</h1>
          <div class="flex flex-wrap items-center gap-2">
            <a :href="`/${dataset.published.path}`" class="font-mono text-xs text-muted-foreground hover:text-foreground">{{ csvUrl(dataset) }}</a>
            <Button size="sm" variant="outline" @click="copy">{{ copied }}</Button>
            <Button as="a" :href="`/${dataset.published.path}`" download size="sm">Download CSV</Button>
          </div>
        </div>
        <p class="text-sm text-muted-foreground">{{ dataset.summary }}</p>
        <DatasetFacts :dataset="dataset" class="mt-1" />
      </CardHeader>
      <DatasetExplorer :key="dataset.id" :dataset="dataset" />
    </Card>

    <Card>
      <CardContent class="pt-6">
        <dl class="grid gap-x-6 gap-y-2.5 text-sm sm:grid-cols-[max-content_1fr]">
          <dt class="text-muted-foreground">Columns</dt>
          <dd><code>{{ dataset.published.columns.join(",") }}</code></dd>
          <dt class="text-muted-foreground">Source</dt>
          <dd><a :href="dataset.published.source.url" class="underline underline-offset-4">{{ dataset.published.source.text }}</a></dd>
          <dt class="text-muted-foreground">Licence</dt>
          <dd class="break-words">{{ dataset.published.licence }}</dd>
          <dt class="text-muted-foreground">Raw documents</dt>
          <dd>
            <a :href="`/${dataset.published.manifest}`" class="underline underline-offset-4">{{ dataset.published.raw_files }} archived files</a>, each
            with its sha256, fetch time and source URL
          </dd>
        </dl>
      </CardContent>
    </Card>
  </div>
</template>
