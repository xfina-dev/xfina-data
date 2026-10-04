import { ViteSSG } from "vite-ssg";
import "xfina-ui/style.css";
import "./style.css";
import App from "./App.vue";
import { routes } from "./routes.js";

export const createApp = ViteSSG(App, {
  routes,
  // A dataset page opens at its top; within a page the explorer keeps the
  // reader's place.
  scrollBehavior: (to, from, saved) => saved ?? (to.path !== from.path ? { top: 0 } : false),
});
