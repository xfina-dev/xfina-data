<script setup>
// Every page: the shared header with the dataset picker, the page itself, and
// the family cards. The header's title is the page's <h1> only on the index;
// a dataset page and the 404 carry their own.
import { computed } from "vue";
import { useRoute } from "vue-router";
import { useHead } from "@unhead/vue";
import { XfinaFamily, XfinaHeader, XfinaProvider } from "xfina-ui";
import logo from "xfina-ui/logo.svg?url";
import DatasetPicker from "./components/DatasetPicker.vue";

const route = useRoute();
const isIndex = computed(() => route.path === "/");

useHead({
  htmlAttrs: { lang: "en" },
  link: [{ rel: "icon", type: "image/svg+xml", href: logo }],
});
</script>

<template>
  <XfinaProvider>
    <div class="xf-container space-y-8">
      <XfinaHeader site="data" home="/" :heading="isIndex">
        <template #context><DatasetPicker /></template>
      </XfinaHeader>
      <main>
        <RouterView />
      </main>
      <XfinaFamily site="data" class="pt-8" />
    </div>
  </XfinaProvider>
</template>
