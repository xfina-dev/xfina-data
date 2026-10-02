// Light or dark, as xfina.dev does it: the OS decides until the reader
// presses the toggle, and then their choice is remembered on this device.
//
// Loaded in <head> without defer, so the stored choice is applied before the
// first paint and a dark-mode reader never sees a white flash.

(() => {
  const KEY = "xfina-data-theme";
  const root = document.documentElement;
  const dark = matchMedia("(prefers-color-scheme: dark)");

  // Storage can be missing or throw (private windows, blocked site data); the
  // page then just follows the OS.
  function stored() {
    try { return localStorage.getItem(KEY); } catch { return null; }
  }
  function store(value) {
    try { localStorage.setItem(KEY, value); } catch { /* follow the OS */ }
  }

  function isDark() {
    const theme = root.getAttribute("data-theme");
    return theme ? theme === "dark" : dark.matches;
  }

  const saved = stored();
  if (saved === "light" || saved === "dark") root.setAttribute("data-theme", saved);

  function announce() {
    window.dispatchEvent(new CustomEvent("themechange", { detail: { dark: isDark() } }));
  }
  dark.addEventListener("change", () => { if (!root.hasAttribute("data-theme")) announce(); });

  document.addEventListener("DOMContentLoaded", () => {
    const button = document.getElementById("theme-toggle");
    if (!button) return;
    button.addEventListener("click", () => {
      const next = isDark() ? "light" : "dark";
      root.setAttribute("data-theme", next);
      store(next);
      announce();
    });
  });
})();
