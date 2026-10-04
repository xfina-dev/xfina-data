import IndexPage from "./pages/IndexPage.vue";
import DatasetPage from "./pages/DatasetPage.vue";
import NotFoundPage from "./pages/NotFoundPage.vue";

// Paths are the ones the site has always served. /404 is rendered to
// 404.html, which Cloudflare serves for any path that is not a file.
export const routes = [
  { path: "/", component: IndexPage },
  { path: "/datasets/:id/", component: DatasetPage, props: true },
  { path: "/404", component: NotFoundPage },
  { path: "/:pathMatch(.*)*", component: NotFoundPage },
];
