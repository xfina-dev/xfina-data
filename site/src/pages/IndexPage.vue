<script setup>
// The index: every public dataset under its group, published or not, then how
// to use the files. A dataset still being brought up says so, so a reader can
// see what is coming without mistaking it for something to use today.
import { useHead } from "@unhead/vue";
import { Button, Card, CardContent, CardDescription, CardHeader, CardTitle } from "xfina-ui";
import DatasetFacts from "../components/DatasetFacts.vue";
import { ORIGIN, groups, pageOf, published } from "../site.js";

useHead({
  title: "Xfina Data",
  meta: [
    {
      name: "description",
      content: "Open Indian financial datasets as plain CSV, each value traceable to an archived source document.",
    },
  ],
});

// Listed in the same order as the groups above them.
const endpoints = published.map((d) => `${ORIGIN}/${d.published.path}`).join("\n");
</script>

<template>
  <div class="space-y-8">
    <section v-for="group in groups" :key="group.name">
      <h2 class="mb-4 text-xl font-semibold tracking-tight">{{ group.name }}</h2>
      <div class="grid gap-6 md:grid-cols-2">
        <Card v-for="d in group.datasets" :key="d.id" class="flex flex-col">
          <CardHeader>
            <CardTitle class="text-xl leading-tight">{{ d.title }}</CardTitle>
            <CardDescription>{{ d.summary }}</CardDescription>
          </CardHeader>
          <CardContent class="mt-auto flex flex-col gap-4">
            <DatasetFacts :dataset="d" />
            <div v-if="d.published" class="flex flex-wrap gap-2">
              <Button as-child size="sm"><RouterLink :to="pageOf(d)">Preview</RouterLink></Button>
              <Button as="a" :href="`/${d.published.path}`" download size="sm" variant="outline">Download CSV</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </section>

    <Card>
      <CardHeader><CardTitle>For developers</CardTitle></CardHeader>
      <CardContent class="space-y-3 text-sm text-muted-foreground">
        <p>Each series is one CSV with its full history, at a URL that does not change:</p>
        <pre class="overflow-x-auto rounded-md bg-muted px-3.5 py-3 font-mono text-[13px] text-foreground">{{ endpoints }}</pre>
        <p>
          <a href="/v1/metadata.json" class="text-foreground underline underline-offset-4"><code>{{ ORIGIN }}/v1/metadata.json</code></a>
          lists every series with its path, columns, source, licence, row count, date range, sha256 and last update;
          read it to discover what is available and whether anything is new.
        </p>
        <p>
          Downloads are compressed with Brotli or gzip when the client asks for it, which brings the larger series to
          about a quarter of their size (the BIS rates go from 254 KB to 65 KB). With curl:
        </p>
        <pre class="overflow-x-auto rounded-md bg-muted px-3.5 py-3 font-mono text-[13px] text-foreground">curl --compressed -O {{ ORIGIN }}/v1/fx/bis-usd-inr.csv</pre>
        <p>
          Over plain HTTP, ask with <code class="text-foreground">Accept-Encoding</code> and the response says which one
          it used. Browsers and most HTTP libraries do this on their own:
        </p>
        <pre class="overflow-x-auto rounded-md bg-muted px-3.5 py-3 font-mono text-[13px] text-foreground">GET /v1/fx/bis-usd-inr.csv HTTP/2
Host: data.xfina.dev
Accept-Encoding: br, gzip

HTTP/2 200
content-type: text/csv; charset=utf-8
content-encoding: br
access-control-allow-origin: *</pre>
        <p>Paths and columns under <code class="text-foreground">/v1/</code> are only ever added to, never renamed or removed.</p>
      </CardContent>
    </Card>
  </div>
</template>
