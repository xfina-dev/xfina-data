/* xfina-ui 0.2.0 */
// The parts every xfina.dev site draws the same way: <xfina-header>,
// <xfina-footer> and <xfina-select>. Plain custom elements with no framework,
// so the same file serves data.xfina.dev (static HTML) and the Vue sites
// (xfina.dev, labs.xfina.dev) alike.
//
// Both render into a shadow root, so neither Tailwind's preflight nor a site's
// own stylesheet can restyle them. That is what keeps three sites, styled
// three ways, looking like one. Colours still come from the page's tokens
// (dist/xfina-ui.css), because custom properties reach into a shadow root.
//
// A classic script, not a module, so a static page can load it with a plain
// <script defer> and a Vite app with a side-effect import.

(() => {
  if (window.XfinaUI) return;

  const VERSION = "0.2.0";
  const LOGO = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"103 101 310 310\"><defs><linearGradient id=\"xGrad\" x1=\"0%\" y1=\"100%\" x2=\"100%\" y2=\"0%\"><stop offset=\"0%\" stop-color=\"#243B92\"/><stop offset=\"100%\" stop-color=\"#4457D9\"/></linearGradient><linearGradient id=\"accent\" x1=\"0%\" y1=\"100%\" x2=\"0%\" y2=\"0%\"><stop offset=\"0%\" stop-color=\"#3EDFD8\"/><stop offset=\"100%\" stop-color=\"#5BE5FF\"/></linearGradient></defs><!-- X --><path fill=\"url(#xGrad)\" d=\" M118 120 L190 120 L256 210 L322 120 L394 120 L292 255 L396 392 L326 392 L256 298 L186 392 L116 392 L220 255 Z\"/><!-- Data Bars --><rect x=\"318\" y=\"228\" width=\"22\" height=\"56\" rx=\"5\" fill=\"url(#accent)\"/><rect x=\"348\" y=\"198\" width=\"22\" height=\"86\" rx=\"5\" fill=\"url(#accent)\"/><rect x=\"378\" y=\"164\" width=\"22\" height=\"120\" rx=\"5\" fill=\"url(#accent)\"/></svg>";

  // The family, in the order the switcher and footer show it. Xfina, the
  // flagship, comes first. A new member is a row here and a release, and every
  // site's header learns it on its next upgrade.
  //
  // Xfingine is part of the family but has no site: it is a library,
  // documented on crates.io, npm and PyPI, so its entry opens its repository.
  // `external` keeps it out of SITES, so <xfina-header site="xfingine"> stays
  // an error rather than drawing a header for a site that does not exist.
  const FAMILY = [
    {
      id: "xfina",
      label: "Xfina",
      title: "Xfina",
      url: "https://xfina.dev/",
      repo: "xfina-dev/xfina",
      tagline:
        "e<strong>X</strong>tract <strong>fina</strong>ncial statements entirely in your browser with Rust/Wasm<br>" +
        "Fast, private, zero-setup, and without uploading your files to any server.",
    },
    {
      id: "labs",
      label: "Labs",
      title: "Xfina Labs",
      url: "https://labs.xfina.dev/",
      repo: "xfina-dev/xfina-labs",
      tagline:
        "Experimental finance tools for long-term investors.<br>" +
        "Everything runs in your browser; nothing is uploaded to any server.",
    },
    { id: "xfingine", label: "Xfingine", title: "Xfingine", url: "https://github.com/xfina-dev/xfingine", external: true },
    {
      id: "data",
      label: "Data",
      title: "Xfina Data",
      url: "https://data.xfina.dev/",
      repo: "xfina-dev/xfina-data",
      tagline:
        "Open Indian financial data as plain CSV, previewed before you use it.<br>" +
        "Every value traces back to a source document kept unchanged in a public archive.",
    },
  ];
  const SITES = FAMILY.filter((s) => !s.external);

  // The author's writing on personal finance, where these tools come from.
  // Labelled by what the reader finds there rather than by its domain.
  const AUTHOR = { label: "Building Wealth", url: "https://sakthipriyan.com/building-wealth" };

  // Lucide icons, inlined so the header needs no icon package or asset path.
  const ICONS = {
    github:
      '<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>',
    activity:
      '<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>',
    sun:
      '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>',
    moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
    x: '<path d="M18 6 6 18M6 6l12 12"/>',
  };
  const icon = (name, cls = "") =>
    `<svg class="${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;

  const escape = (text) =>
    String(text).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

  // A misspelt site id would otherwise draw a header for no site at all, with
  // no switcher entry marked and a GitHub button to nowhere. Fail where the
  // author will see it.
  function siteFor(element) {
    const id = element.getAttribute("site");
    const site = SITES.find((s) => s.id === id);
    if (!site) {
      throw new Error(
        `<${element.localName} site="${id}">: unknown site; expected one of ${SITES.map((s) => s.id).join(", ")}`,
      );
    }
    return site;
  }

  // Shared by the header and footer: the outline button, as shadcn's
  // `variant="outline"` draws it.
  const BUTTON_CSS = `
    .btn {
      display: inline-flex; align-items: center; justify-content: center; gap: 6px;
      box-sizing: border-box; height: 36px; padding: 0 12px;
      border: 1px solid hsl(var(--input)); border-radius: calc(var(--radius) - 2px);
      background: hsl(var(--background)); color: hsl(var(--foreground));
      box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05);
      font: 500 0.875rem/1.25rem var(--font-sans); white-space: nowrap;
      text-decoration: none; cursor: pointer;
      transition: background-color 150ms, color 150ms;
    }
    .btn:hover { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
    .btn:focus-visible { outline: none; box-shadow: 0 0 0 1px hsl(var(--ring)); }
    .btn.icon { width: 36px; padding: 0; }
    svg { width: 1.2rem; height: 1.2rem; fill: none; stroke: currentColor;
          stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
    .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
               overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  `;

  const HEADER_CSS = `
    :host { display: block; color: hsl(var(--foreground)); font-family: var(--font-sans); }
    [hidden] { display: none !important; }
    /* The buttons share the title's line, and the tagline runs the full width
       beneath both. In a column of their own, beside the title and tagline
       together, the buttons left the tagline under 480px and wrapped it to
       four lines, with empty space under the buttons.
       Below 1024px the buttons move to their own row under the tagline. At
       1024px the 960px column holds the logo (84px), the widest title with
       its picker (xfina.dev with version and commit hash, ~330px) and the
       ~480px of buttons, with room to spare. */
    header {
      display: grid; grid-template-columns: 64px minmax(0, 1fr);
      grid-template-areas: "logo title" "logo tagline" "actions actions";
      column-gap: 20px; align-items: start;
    }
    @media (min-width: 1024px) {
      header {
        grid-template-columns: 64px minmax(0, 1fr) auto;
        grid-template-areas: "logo title actions" "logo tagline tagline";
      }
      /* Scoped to header so they outrank the base rules further down. */
      header .actions, header .row { flex-wrap: nowrap; }
      header .actions { margin-top: 0; }
    }
    .logo { grid-area: logo; line-height: 0; transition: opacity 150ms; }
    .logo:hover { opacity: 0.8; }
    .logo svg { width: 64px; height: 64px; stroke: none; }
    .title-row { grid-area: title; display: flex; flex-direction: column; gap: 12px; }
    @media (min-width: 640px) { .title-row { flex-direction: row; align-items: center; gap: 16px; } }
    .title { color: inherit; text-decoration: none; transition: color 150ms; }
    .title:hover { color: hsl(var(--primary)); }
    .name { white-space: nowrap; margin: 0; font-size: 1.875rem; line-height: 2.25rem; font-weight: 700; letter-spacing: -0.025em; }
    .tagline { grid-area: tagline; margin: 8px 0 0; color: hsl(var(--muted-foreground)); line-height: 1.625; }
    .actions { grid-area: actions; display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-top: 16px; }
    .row { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
    .sites { display: inline-flex; }
    .sites .btn { border-radius: 0; margin-left: -1px; }
    .sites .btn:first-child { margin-left: 0; border-radius: calc(var(--radius) - 2px) 0 0 calc(var(--radius) - 2px); }
    .sites .btn:last-child { border-radius: 0 calc(var(--radius) - 2px) calc(var(--radius) - 2px) 0; }
    .sites .btn:focus-visible { position: relative; }
    dialog {
      box-sizing: border-box; width: calc(100% - 32px); max-width: 32rem; padding: 24px;
      border: 1px solid hsl(var(--border)); border-radius: var(--radius);
      background: hsl(var(--background)); color: hsl(var(--foreground));
      box-shadow: 0 10px 15px -3px rgb(0 0 0 / 0.1);
    }
    dialog::backdrop { background: rgb(0 0 0 / 0.8); }
    dialog h2 { margin: 0 32px 8px 0; font-size: 1.125rem; line-height: 1.75rem; font-weight: 600; }
    dialog p { margin: 0 0 8px; color: hsl(var(--muted-foreground)); font-size: 0.875rem; line-height: 1.5; }
    dialog .close { position: absolute; top: 12px; right: 12px; border: 0; box-shadow: none; }
  `;

  class XfinaHeader extends HTMLElement {
    connectedCallback() {
      if (this.shadowRoot) return;
      const site = siteFor(this);
      const home = this.getAttribute("home") || "/";
      const name = this.hasAttribute("heading") ? "h1" : "span";
      const host = new URL(site.url).host;

      // The current site is left out: the title already says where the reader
      // is, and the button would only take them to the page they are on.
      const switcher = FAMILY.filter((s) => s.id !== site.id).map((s) =>
        s.external
          ? `<a class="btn" href="${s.url}" target="_blank" rel="noopener noreferrer">${escape(s.label)}<span class="sr-only"> (GitHub, opens in a new tab)</span></a>`
          : `<a class="btn" href="${s.url}">${escape(s.label)}</a>`,
      ).join("");

      const root = this.attachShadow({ mode: "open" });
      root.innerHTML = `
        <style>${BUTTON_CSS}${HEADER_CSS}</style>
        <header part="header">
          <a class="logo" href="${escape(home)}" aria-label="${escape(site.title)} home">${LOGO}</a>
          <div class="title-row">
            <a class="title" href="${escape(home)}"><${name} class="name">${escape(site.title)}</${name}></a>
            <slot name="context"></slot>
          </div>
          <p class="tagline"><slot name="tagline">${site.tagline}</slot></p>
          <nav class="actions" aria-label="Xfina sites and links">
            <div class="sites">${switcher}</div>
            <div class="row">
              <a class="btn" href="${AUTHOR.url}" target="_blank" rel="noopener noreferrer">${AUTHOR.label}</a>
              <a class="btn icon" href="https://github.com/${site.repo}" target="_blank" rel="noopener noreferrer" title="GitHub repository">
                ${icon("github")}<span class="sr-only">GitHub repository</span>
              </a>
              <slot name="actions"></slot>
              <button type="button" class="btn icon" data-action="privacy" title="Privacy &amp; analytics">
                ${icon("activity")}<span class="sr-only">Privacy &amp; analytics</span>
              </button>
              <button type="button" class="btn icon" data-action="theme" title="Toggle theme">
                ${icon("sun", "when-dark")}${icon("moon", "when-light")}<span class="sr-only">Toggle theme</span>
              </button>
            </div>
          </nav>
        </header>
        <dialog aria-labelledby="privacy-title">
          <button type="button" class="btn icon close" data-action="close" title="Close">${icon("x")}<span class="sr-only">Close</span></button>
          <h2 id="privacy-title">Privacy &amp; analytics</h2>
          <p>${escape(host)} runs no analytics: no tracking script, no tracking cookie, and nothing about your visit is sent anywhere by this page.</p>
          <p>Cloudflare, which serves the site, sees each request the way any web server does.</p>
          <p>The one cookie is <code>xfina-theme</code> on xfina.dev, which remembers whether you chose light or dark.</p>
        </dialog>
      `;

      root.addEventListener("click", (event) => {
        const action = event.target.closest("[data-action]")?.dataset.action;
        if (action === "theme") window.xfinaTheme.toggle();
        if (action === "close") root.querySelector("dialog").close();
        if (action === "privacy") this.openPrivacy();
      });

      // The theme script owns light and dark. Without it the toggle would do
      // nothing when pressed, so it is hidden and the omission is reported
      // instead of shipped.
      const toggle = root.querySelector('[data-action="theme"]');
      if (!window.xfinaTheme) {
        toggle.hidden = true;
        console.error("<xfina-header>: xfina-theme.js is not loaded in <head>, so the theme toggle is hidden");
      } else {
        this.showTheme(window.xfinaTheme.isDark());
        this.onTheme = (event) => this.showTheme(event.detail.dark);
        window.addEventListener("themechange", this.onTheme);
      }
    }

    disconnectedCallback() {
      if (this.onTheme) window.removeEventListener("themechange", this.onTheme);
    }

    // The icon shows where a press takes you: a sun on a dark page, a moon on
    // a light one.
    showTheme(dark) {
      for (const svg of this.shadowRoot.querySelectorAll(".when-dark")) svg.style.display = dark ? "" : "none";
      for (const svg of this.shadowRoot.querySelectorAll(".when-light")) svg.style.display = dark ? "none" : "";
    }

    // A site that runs analytics has its own consent dialog, and takes over by
    // calling preventDefault() on `xfina-privacy`. Every other site gets the
    // dialog above, which says plainly that nothing is collected.
    openPrivacy() {
      const event = new CustomEvent("xfina-privacy", { bubbles: true, composed: true, cancelable: true });
      if (this.dispatchEvent(event)) this.shadowRoot.querySelector("dialog").showModal();
    }
  }

  const FOOTER_CSS = `
    :host { display: block; color: hsl(var(--muted-foreground)); font-family: var(--font-sans); font-size: 0.875rem; }
    footer { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 12px;
             padding-top: 24px; border-top: 1px solid hsl(var(--border)); }
    nav, .links { display: flex; flex-wrap: wrap; gap: 16px; }
    a { color: inherit; text-decoration: none; }
    a:hover { color: hsl(var(--foreground)); text-decoration: underline; text-underline-offset: 4px; }
    a[aria-current="page"] { color: hsl(var(--foreground)); font-weight: 600; }
  `;

  class XfinaFooter extends HTMLElement {
    connectedCallback() {
      if (this.shadowRoot) return;
      const site = siteFor(this);
      const links = FAMILY.map((s) =>
        s.external
          ? `<a href="${s.url}" target="_blank" rel="noopener noreferrer">${escape(s.title)} on GitHub</a>`
          : `<a href="${s.url}"${s.id === site.id ? ' aria-current="page"' : ""}>${escape(s.title)}</a>`,
      ).join("");
      this.attachShadow({ mode: "open" }).innerHTML = `
        <style>${FOOTER_CSS}</style>
        <footer part="footer">
          <nav aria-label="Xfina sites">${links}</nav>
          <div class="links">
            <a href="https://github.com/${site.repo}" target="_blank" rel="noopener noreferrer">Source on GitHub</a>
            <a href="${AUTHOR.url}" target="_blank" rel="noopener noreferrer">${AUTHOR.label}</a>
          </div>
        </footer>
      `;
    }
  }

  // <xfina-select>: the picker a site puts beside its title, drawn as Labs and
  // xfina.dev draw theirs with shadcn's Select, so a site without Vue (Data)
  // gets the same control. A native <select> cannot match: its open menu is
  // drawn by the operating system, in its own fonts and colours.
  //
  //   <xfina-select slot="context" label="App">
  //     <option value="portfolio" selected>Portfolio Engine</option>
  //   </xfina-select>
  //
  // The <option> and <optgroup> children are read once, as data. A group is
  // drawn as shadcn's SelectGroup with a SelectLabel, and sections are divided
  // by a SelectSeparator. Choosing one sets `value`
  // and fires `change` on the element; what a choice does (navigate, filter)
  // is the site's business. Keyboard use follows the ARIA combobox pattern:
  // focus stays on the trigger, and the arrow keys move the highlight.
  const SELECT_CSS = `
    :host { position: relative; display: inline-block; vertical-align: middle;
            color: hsl(var(--foreground)); font-family: var(--font-sans); }
    [hidden] { display: none !important; }
    svg { fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
    .trigger {
      display: flex; align-items: center; justify-content: space-between; gap: 8px;
      box-sizing: border-box; height: 36px; min-width: 140px; padding: 8px 12px;
      border: 1px solid hsl(var(--border)); border-radius: calc(var(--radius) - 2px);
      background: hsl(var(--background)); color: inherit;
      box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05);
      font: 400 0.875rem/1.25rem var(--font-sans); text-align: start; cursor: pointer;
    }
    .trigger:focus-visible { outline: none; box-shadow: 0 0 0 1px hsl(var(--ring)); }
    .value { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .trigger svg { flex: none; width: 16px; height: 16px; opacity: 0.5; }
    [role="listbox"] {
      /* As wide as its widest line, and never narrower than the trigger. An
         absolute box would otherwise shrink to the trigger and wrap labels. */
      position: absolute; z-index: 50; top: calc(100% + 4px); left: 0; min-width: 100%; width: max-content;
      box-sizing: border-box; max-height: 384px; overflow-y: auto; margin: 0; padding: 4px;
      border: 1px solid hsl(var(--border)); border-radius: calc(var(--radius) - 2px);
      background: hsl(var(--popover)); color: hsl(var(--popover-foreground));
      box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1);
      animation: xf-open 100ms ease-out;
    }
    @keyframes xf-open { from { opacity: 0; transform: scale(0.95) translateY(-4px); } }
    [role="option"] {
      position: relative; display: flex; align-items: center; padding: 6px 8px 6px 32px;
      border-radius: calc(var(--radius) - 4px); font-size: 0.875rem; line-height: 1.25rem;
      white-space: nowrap; cursor: default; user-select: none;
    }
    [role="option"].active { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
    [role="option"] svg { position: absolute; left: 8px; width: 16px; height: 16px; visibility: hidden; }
    [role="option"][aria-selected="true"] svg { visibility: visible; }
    .group-label { white-space: nowrap; padding: 6px 8px 6px 32px; font-size: 0.875rem; line-height: 1.25rem; font-weight: 600; }
    .separator { height: 1px; margin: 4px -4px; background: hsl(var(--muted)); }
  `;

  let selects = 0;

  class XfinaSelect extends HTMLElement {
    connectedCallback() {
      if (this.shadowRoot) return;
      // Sections in document order: each <optgroup>, and each run of options
      // outside one. Choices are numbered across all of them, so the keyboard
      // moves through the whole list as one.
      const sections = [];
      this.choices = [];
      const read = (option) => {
        this.choices.push({ value: option.value, label: option.textContent.trim(), selected: option.hasAttribute("selected") });
        return this.choices.length - 1;
      };
      for (const child of this.children) {
        if (child.localName === "optgroup") {
          sections.push({ label: child.label, items: [...child.querySelectorAll("option")].map(read) });
        } else if (child.localName === "option") {
          const last = sections.at(-1);
          if (last && last.label == null) last.items.push(read(child));
          else sections.push({ label: null, items: [read(child)] });
        }
      }
      if (!this.choices.length) throw new Error("<xfina-select>: needs at least one <option>");

      const id = `xfina-select-${++selects}`;
      const label = escape(this.getAttribute("label") || "");
      const root = this.attachShadow({ mode: "open" });
      root.innerHTML = `
        <style>${SELECT_CSS}</style>
        <button type="button" class="trigger" part="trigger" role="combobox" aria-haspopup="listbox"
                aria-expanded="false" aria-controls="${id}" aria-label="${label}">
          <span class="value"></span>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>
        </button>
        <div role="listbox" id="${id}" aria-label="${label}" hidden>
          ${sections
            .map((section, k) => {
              const options = section.items
                .map(
                  (i) =>
                    `<div role="option" id="${id}-${i}" data-index="${i}" aria-selected="false">` +
                    `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>${escape(this.choices[i].label)}</div>`,
                )
                .join("");
              if (section.label == null) return options;
              return (
                `<div role="group" aria-labelledby="${id}-g${k}">` +
                `<div class="group-label" id="${id}-g${k}">${escape(section.label)}</div>${options}</div>`
              );
            })
            .join('<div class="separator" role="separator"></div>')}
        </div>
      `;
      this.trigger = root.querySelector(".trigger");
      this.list = root.querySelector('[role="listbox"]');
      this.items = [...root.querySelectorAll('[role="option"]')];

      const initial = this.getAttribute("value") ?? this.choices.find((c) => c.selected)?.value ?? this.choices[0].value;
      this.show(this.indexOf(initial));

      this.trigger.addEventListener("click", () => (this.list.hidden ? this.open() : this.close()));
      this.trigger.addEventListener("keydown", (event) => this.key(event));
      this.list.addEventListener("mousedown", (event) => event.preventDefault()); // keep focus on the trigger
      this.list.addEventListener("click", (event) => {
        const item = event.target.closest('[role="option"]');
        if (item) this.choose(Number(item.dataset.index));
      });
      this.list.addEventListener("mousemove", (event) => {
        const item = event.target.closest('[role="option"]');
        if (item) this.highlight(Number(item.dataset.index));
      });
      this.outside = (event) => {
        if (!event.composedPath().includes(this)) this.close();
      };
    }

    disconnectedCallback() {
      document.removeEventListener("pointerdown", this.outside);
    }

    get value() {
      return this.choices[this.selected].value;
    }

    set value(value) {
      this.show(this.indexOf(value));
    }

    // A value no option has would leave the trigger naming one thing while the
    // page acts on another. Fail where the author will see it.
    indexOf(value) {
      const index = this.choices.findIndex((c) => c.value === value);
      if (index < 0) {
        throw new Error(`<xfina-select>: no option has the value "${value}"`);
      }
      return index;
    }

    show(index) {
      this.selected = index;
      this.shadowRoot.querySelector(".value").textContent = this.choices[index].label;
      this.items.forEach((item, i) => item.setAttribute("aria-selected", String(i === index)));
    }

    open() {
      this.list.hidden = false;
      this.trigger.setAttribute("aria-expanded", "true");
      this.highlight(this.selected);
      document.addEventListener("pointerdown", this.outside);
    }

    close() {
      if (this.list.hidden) return;
      this.list.hidden = true;
      this.trigger.setAttribute("aria-expanded", "false");
      this.trigger.removeAttribute("aria-activedescendant");
      document.removeEventListener("pointerdown", this.outside);
    }

    highlight(index) {
      this.active = Math.max(0, Math.min(index, this.items.length - 1));
      this.items.forEach((item, i) => item.classList.toggle("active", i === this.active));
      this.trigger.setAttribute("aria-activedescendant", this.items[this.active].id);
      this.items[this.active].scrollIntoView?.({ block: "nearest" });
    }

    choose(index) {
      this.close();
      this.trigger.focus();
      if (index === this.selected) return;
      this.show(index);
      this.dispatchEvent(new CustomEvent("change", { bubbles: true, detail: { value: this.value } }));
    }

    key(event) {
      const closed = this.list.hidden;
      const keys = {
        ArrowDown: () => (closed ? this.open() : this.highlight(this.active + 1)),
        ArrowUp: () => (closed ? this.open() : this.highlight(this.active - 1)),
        Home: () => !closed && this.highlight(0),
        End: () => !closed && this.highlight(this.items.length - 1),
        Enter: () => (closed ? this.open() : this.choose(this.active)),
        " ": () => (closed ? this.open() : this.choose(this.active)),
        Escape: () => this.close(),
        Tab: () => this.close(),
      };
      if (!keys[event.key]) return;
      if (event.key !== "Tab") event.preventDefault();
      keys[event.key]();
    }
  }

  customElements.define("xfina-header", XfinaHeader);
  customElements.define("xfina-footer", XfinaFooter);
  customElements.define("xfina-select", XfinaSelect);

  const frozen = (list) => Object.freeze(list.map((s) => Object.freeze({ ...s })));
  window.XfinaUI = Object.freeze({ VERSION, FAMILY: frozen(FAMILY), SITES: frozen(SITES) });
})();
