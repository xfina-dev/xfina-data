<script setup>
import { useHead } from "@unhead/vue";
import { Button, Card, CardContent, CardDescription, CardHeader } from "xfina-ui";

useHead({ title: "Not Found · Xfina Data", meta: [{ name: "robots", content: "noindex" }] });
</script>

<template>
  <Card>
    <CardHeader>
      <h1 class="text-2xl font-semibold leading-none tracking-tight">Not found</h1>
      <CardDescription>Nothing is published at this path.</CardDescription>
    </CardHeader>
    <CardContent class="flex flex-wrap gap-2">
      <Button as-child size="sm"><RouterLink to="/">All datasets</RouterLink></Button>
      <Button as="a" href="/v1/metadata.json" size="sm" variant="outline">metadata.json</Button>
    </CardContent>
  </Card>
</template>
