<script setup>
// The header's dataset picker, on every page: "All datasets" (the index), then
// each published dataset under its index group, by its short name. The page
// being read is the one selected. Only published datasets are offered, so no
// choice leads to a 404.
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "xfina-ui";
import { groups, pageOf } from "../site.js";

const route = useRoute();
const router = useRouter();

const offered = groups
  .map((g) => ({ name: g.name, datasets: g.datasets.filter((d) => d.published) }))
  .filter((g) => g.datasets.length);

const value = computed({
  get: () => (route.path === "/" ? "/" : offered.flatMap((g) => g.datasets).some((d) => pageOf(d) === route.path) ? route.path : ""),
  set: (path) => path && path !== route.path && router.push(path),
});
</script>

<template>
  <Select v-model="value">
    <SelectTrigger class="h-9 w-auto min-w-[140px] gap-2 shadow-sm" aria-label="Dataset">
      <SelectValue placeholder="Datasets" />
    </SelectTrigger>
    <SelectContent>
      <SelectItem value="/">All datasets</SelectItem>
      <template v-for="g in offered" :key="g.name">
        <SelectSeparator />
        <SelectGroup>
          <SelectLabel>{{ g.name }}</SelectLabel>
          <SelectItem v-for="d in g.datasets" :key="d.id" :value="pageOf(d)">{{ d.name }}</SelectItem>
        </SelectGroup>
      </template>
    </SelectContent>
  </Select>
</template>
