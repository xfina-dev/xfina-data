<script setup>
// The one-line facts under a dataset's title: frequency, range, rows and the
// day its content last changed. An unpublished dataset says so instead.
import { Badge } from "xfina-ui";
import { frequencyName } from "../site.js";

defineProps({ dataset: { type: Object, required: true } });
</script>

<template>
  <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-muted-foreground">
    <span><b class="font-medium text-foreground">{{ frequencyName(dataset.frequency) }}</b></span>
    <template v-if="dataset.published">
      <span><b class="font-medium text-foreground">{{ dataset.published.first }}</b> → <b class="font-medium text-foreground">{{ dataset.published.last }}</b></span>
      <span><b class="font-medium text-foreground">{{ dataset.published.rows }}</b> rows</span>
      <span>updated {{ dataset.published.updated }}</span>
    </template>
    <Badge v-else variant="soon">Not published yet</Badge>
  </div>
</template>
