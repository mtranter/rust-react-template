import { type ThemeTokens, themeCss } from "./theme-css";

export const themeStyleId = "app-theme";
/** `index.html` injects this before first paint. */
export const themeCacheKey = "app-theme-css";

let saved = "";
let previewing = false;

function write(css: string) {
  let style = document.getElementById(themeStyleId);
  if (!style) {
    style = document.createElement("style");
    style.id = themeStyleId;
  }
  style.textContent = css;
  // Last in <head>, so it follows every stylesheet Vite adds.
  document.head.appendChild(style);
}

function cache(css: string) {
  try {
    if (css) localStorage.setItem(themeCacheKey, css);
    else localStorage.removeItem(themeCacheKey);
  } catch {}
}

/** Applies the saved theme and caches it for the next load; under a preview, it shows once that ends. */
export function applyTheme(tokens: ThemeTokens) {
  saved = themeCss(tokens);
  cache(saved);
  if (!previewing) write(saved);
}

export function clearTheme() {
  saved = "";
  previewing = false;
  cache("");
  document.getElementById(themeStyleId)?.remove();
}

/** Shows unsaved tokens until `endPreview`, which restores the saved theme. */
export function previewTheme(tokens: ThemeTokens) {
  previewing = true;
  write(themeCss(tokens));
}

export function endPreview() {
  previewing = false;
  write(saved);
}
