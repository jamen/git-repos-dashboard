import "./main.css";

const PARTIAL_PARAM = "partial";
const PRELOAD_TTL = 5000;
const HOVER_INTENT = 65;
const PRELOAD_MAX = 10;
const SCROLL_SAVE_DELAY = 200;

const parser = new DOMParser();
const shellId = document.querySelector("meta#shell")?.getAttribute("content") ?? "";

let navigationId = 0;

type Fetched = { html: string; url: URL };

function withPartialParam(url: URL): URL {
  const target = new URL(url);
  target.searchParams.set(PARTIAL_PARAM, "1");
  return target;
}

function withoutPartialParam(href: string): URL {
  const url = new URL(href);
  url.searchParams.delete(PARTIAL_PARAM);
  return url;
}

type Preloaded = { at: number; fetched: Promise<Fetched> };

const preloaded = new Map<string, Preloaded>();

async function request(url: URL): Promise<Fetched> {
  const response = await fetch(withPartialParam(url).href, { credentials: "same-origin" });

  return {
    html: await response.text(),
    url: withoutPartialParam(response.url || url.href),
  };
}

function preload(url: URL): Promise<Fetched> {
  const key = url.pathname + url.search;

  const existing = preloaded.get(key);

  if (existing && Date.now() - existing.at < PRELOAD_TTL) return existing.fetched;

  const fetched = request(url);

  fetched.catch(() => {});

  preloaded.delete(key);
  preloaded.set(key, { at: Date.now(), fetched });

  while (preloaded.size > PRELOAD_MAX) {
    const oldest = preloaded.keys().next().value;
    if (oldest === undefined) break;
    preloaded.delete(oldest);
  }

  return fetched;
}

function applyPartials(html: string): boolean {
  if (/^\s*<!doctype/i.test(html)) return false;

  const doc = parser.parseFromString(html, "text/html");

  const incoming = [...doc.head.children, ...doc.body.children].filter((el) => el.id);

  if (incoming.length === 0) return false;

  const shell = incoming.find((el) => el.id === "shell");

  if (shell && shell.getAttribute("content") !== shellId) return false;

  const partials = incoming.filter((el) => el.id !== "shell");
  const swaps: Array<[Element, Element]> = [];

  for (const partial of partials) {
    const target = document.getElementById(partial.id);
    if (!target) return false;
    swaps.push([target, partial]);
  }

  if (swaps.length === 0) return false;

  for (const [target, partial] of swaps) target.replaceWith(partial);

  const title = partials.find((el) => el.id === "title");

  if (title) document.title = title.textContent ?? document.title;

  return true;
}

function syncUrlBoundControls(url: URL) {
  for (const element of document.querySelectorAll("[data-param]")) {
    if (!(element instanceof HTMLInputElement)) continue;
    if (element === document.activeElement) continue;

    element.value = url.searchParams.get(element.dataset.param ?? "") ?? "";
  }
}

function fragmentTarget(hash: string): HTMLElement | null {
  if (!hash || hash === "#") return null;

  const raw = hash.slice(1);

  let id: string;
  try {
    id = decodeURIComponent(raw);
  } catch {
    id = raw;
  }

  return document.getElementById(id);
}

function focusAfterNavigation(target: HTMLElement | null) {
  const main = document.getElementById("main");
  const destination = target ?? main;

  if (!(destination instanceof HTMLElement)) return;

  if (!destination.hasAttribute("tabindex")) {
    destination.setAttribute("tabindex", "-1");
    destination.addEventListener("blur", () => destination.removeAttribute("tabindex"), {
      once: true,
    });
  }

  destination.focus({ preventScroll: true });
}

let scrollSaveTimer: number | undefined;

function saveScroll() {
  clearTimeout(scrollSaveTimer);
  history.replaceState({ ...history.state, scroll: window.scrollY }, "");
}

window.addEventListener(
  "scroll",
  () => {
    clearTimeout(scrollSaveTimer);
    scrollSaveTimer = setTimeout(saveScroll, SCROLL_SAVE_DELAY);
  },
  { passive: true },
);

async function navigate(url: URL, push: boolean): Promise<boolean> {
  const id = ++navigationId;

  let fetched: Fetched;

  try {
    fetched = await preload(url);
  } catch {
    preloaded.delete(url.pathname + url.search);
    return false;
  }

  if (id !== navigationId) return true;

  const canonical = fetched.url;

  if (canonical.origin !== location.origin) return false;

  if (push) saveScroll();

  if (!applyPartials(fetched.html)) return false;

  if (push) {
    history.pushState({ scroll: 0 }, "", canonical.pathname + canonical.search + canonical.hash);
  }

  const fragment = push ? fragmentTarget(canonical.hash) : null;

  if (fragment) {
    fragment.scrollIntoView();
  } else if (push) {
    window.scrollTo(0, 0);
  } else {
    window.scrollTo(0, history.state?.scroll ?? 0);
  }

  syncUrlBoundControls(canonical);
  updateSearchScope();
  closeSearch();
  focusAfterNavigation(fragment);

  document.dispatchEvent(new CustomEvent("partials:applied"));

  return true;
}

function routableUrl(target: EventTarget | null): URL | null {
  if (!(target instanceof Element)) return null;

  const link = target.closest("a");

  if (!(link instanceof HTMLAnchorElement)) return null;
  if (!link.hasAttribute("href")) return null;
  if (link.hasAttribute("download") || link.hasAttribute("data-hard")) return null;
  if (link.target && link.target !== "_self") return null;
  if (link.relList.contains("external")) return null;

  let url: URL;

  try {
    url = new URL(link.href);
  } catch {
    return null;
  }

  if (url.origin !== location.origin) return null;

  return url;
}

function isPlainClick(event: MouseEvent): boolean {
  return (
    !event.defaultPrevented &&
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey
  );
}

document.addEventListener("click", async (event) => {
  if (!isPlainClick(event)) return;

  const url = routableUrl(event.target);

  if (!url) return;

  if (url.hash && url.pathname === location.pathname && url.search === location.search) return;

  event.preventDefault();

  if (!(await navigate(url, true))) location.href = url.href;
});

document.addEventListener("submit", async (event) => {
  if (event.defaultPrevented) return;

  const form = event.target;

  if (!(form instanceof HTMLFormElement)) return;
  if (form.method.toLowerCase() !== "get") return;
  if (form.target && form.target !== "_self") return;

  if (form.hasAttribute("data-search-form")) {
    const input = form.querySelector<HTMLInputElement>("[data-search-input]");

    if (!input?.value.trim()) {
      event.preventDefault();

      const { repo, branch } = currentSearchScope();
      const target = scopeTarget(repo, branch);

      if (!(await navigate(target, true))) location.href = target.href;
      return;
    }
  }

  let url: URL;

  try {
    url = new URL(form.action, location.href);
  } catch {
    return;
  }

  if (url.origin !== location.origin) return;

  url.search = "";

  for (const [key, value] of new FormData(form, event.submitter)) {
    url.searchParams.append(key, value instanceof File ? value.name : value);
  }

  event.preventDefault();

  if (!(await navigate(url, true))) form.submit();
});

window.addEventListener("popstate", async () => {
  if (!(await navigate(new URL(location.href), false))) location.reload();
});

let hoverTimer: number | undefined;

document.addEventListener("mouseover", (event) => {
  const url = routableUrl(event.target);

  clearTimeout(hoverTimer);
  if (!url) return;

  hoverTimer = setTimeout(() => preload(url), HOVER_INTENT);
});

document.addEventListener("pointerdown", (event) => {
  const url = routableUrl(event.target);

  if (url) preload(url);
});

if ("scrollRestoration" in history) history.scrollRestoration = "manual";

history.replaceState({ ...history.state, scroll: window.scrollY }, "");

const searchOverlay = document.querySelector<HTMLElement>("[data-search-overlay]");
const searchInput =
  searchOverlay?.querySelector<HTMLInputElement>("[data-search-input]") ?? null;
const searchScope =
  searchOverlay?.querySelector<HTMLElement>("[data-search-scope]") ?? null;
const searchRepo =
  searchOverlay?.querySelector<HTMLInputElement>("[data-search-repo]") ?? null;
const searchBranch =
  searchOverlay?.querySelector<HTMLInputElement>("[data-search-branch]") ?? null;

function decodeSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function currentSearchScope(): { repo: string | null; branch: string | null } {
  const url = new URL(location.href);
  const segments = url.pathname.split("/").filter(Boolean);

  if (segments[0] === "search") {
    const repo = url.searchParams.get("repo") || null;
    const branch = repo ? url.searchParams.get("branch") || null : null;
    return { repo, branch };
  }

  if (segments[0] === "repos" && segments[1]) {
    return { repo: decodeSegment(segments[1]), branch: null };
  }

  if (segments[0] === "branches" && segments[1]) {
    return {
      repo: decodeSegment(segments[1]),
      branch:
        segments.length > 2
          ? segments.slice(2).map(decodeSegment).join("/")
          : null,
    };
  }

  if (segments[0] === "commits" && segments[1]) {
    return {
      repo: decodeSegment(segments[1]),
      branch: segments.length === 2 ? url.searchParams.get("branch") : null,
    };
  }

  return { repo: null, branch: null };
}

function updateSearchScope() {
  const { repo, branch } = currentSearchScope();

  if (searchScope) {
    const label = branch ? `${repo} / ${branch}` : (repo ?? "all repositories");
    searchScope.textContent = `in ${label}`;
  }

  if (searchRepo) {
    searchRepo.value = repo ?? "";
    searchRepo.disabled = repo === null;
  }

  if (searchBranch) {
    searchBranch.value = branch ?? "";
    searchBranch.disabled = branch === null;
  }
}

function updateSearchExpansion() {
  if (!searchOverlay || !searchInput) return;

  const expanded = !searchOverlay.hidden && searchInput.value.length > 0;

  searchOverlay.classList.toggle("search-expanded", expanded);
  document.body.classList.toggle("search-expanded", expanded);
}

function openSearch() {
  if (!searchOverlay || !searchInput) return;

  updateSearchScope();
  searchOverlay.hidden = false;
  document.body.classList.add("search-open");
  updateSearchExpansion();
  searchInput.focus();
  searchInput.select();
}

function closeSearch() {
  if (!searchOverlay || !searchInput) return;

  searchOverlay.hidden = true;
  searchOverlay.classList.remove("search-expanded");
  document.body.classList.remove("search-open", "search-expanded");
  searchInput.value = "";
  searchInput.blur();
}

function scopeTarget(repo: string | null, branch: string | null): URL {
  if (!repo) return new URL("/", location.origin);

  if (!branch) {
    return new URL(`/repos/${encodeURIComponent(repo)}`, location.origin);
  }

  const branchPath = branch.split("/").map(encodeURIComponent).join("/");
  return new URL(
    `/branches/${encodeURIComponent(repo)}/${branchPath}`,
    location.origin,
  );
}

document.addEventListener("keydown", (event) => {
  if (event.defaultPrevented) return;

  const target = event.target;
  const typing =
    target instanceof HTMLElement &&
    (target.isContentEditable || target.matches("input, textarea, select"));

  if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
    event.preventDefault();
    openSearch();
  } else if (event.key === "Escape" && searchOverlay && !searchOverlay.hidden) {
    event.preventDefault();
    closeSearch();
  }
});

searchInput?.addEventListener("input", updateSearchExpansion);

updateSearchScope();
