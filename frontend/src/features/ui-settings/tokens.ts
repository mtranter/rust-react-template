import type { ThemeTokens } from "@/lib/theming";

export type Scheme = "light" | "dark";

export type ColourGroup = { label: string; description: string; tokens: string[] };

const withForeground = (...names: string[]) => names.flatMap((n) => [n, `${n}-foreground`]);

// The server's allowed colour keys (crates/platform/ui-settings/core/src/domain/theme.rs), grouped.
export const colourGroups: ColourGroup[] = [
  {
    label: "Brand",
    description: "What acts or is current, what stands out without acting, and the focus ring.",
    tokens: [...withForeground("primary", "highlight"), "ring"],
  },
  {
    label: "Status",
    description: "For status only, never decoration.",
    tokens: [...withForeground("success", "warning", "info"), "destructive"],
  },
  {
    label: "Surfaces",
    description: "The page, cards, popovers, neutrals and lines.",
    tokens: [
      "background",
      "foreground",
      ...withForeground("card", "popover", "secondary", "muted", "accent"),
      "border",
      "input",
    ],
  },
  {
    label: "Sidebar",
    description: "The navigation down the side.",
    tokens: [
      ...withForeground("sidebar", "sidebar-primary", "sidebar-accent"),
      "sidebar-border",
      "sidebar-ring",
    ],
  },
  {
    label: "Charts",
    description: "The series of a chart, in order.",
    tokens: ["chart-1", "chart-2", "chart-3", "chart-4", "chart-5"],
  },
];

export const colourTokens = colourGroups.flatMap((g) => g.tokens);

export const shadowTokens = [
  "shadow-2xs",
  "shadow-xs",
  "shadow-sm",
  "shadow-md",
  "shadow-lg",
  "shadow-xl",
  "shadow-2xl",
] as const;

export const fontTokens = ["font-sans", "font-heading", "font-mono"] as const;

export const sharedTokens = ["radius", ...fontTokens, ...shadowTokens];

function shadows(alpha: number, scale: number): Record<string, string> {
  const a = (n: number) => `rgb(0 0 0 / ${Math.min(1, n * alpha).toFixed(2)})`;
  const px = (n: number) => `${Math.round(n * scale)}px`;
  return {
    "shadow-2xs": `0 1px ${a(0.05)}`,
    "shadow-xs": `0 1px ${px(2)} 0 ${a(0.05)}`,
    "shadow-sm": `0 1px ${px(3)} 0 ${a(0.1)}, 0 1px ${px(2)} -1px ${a(0.1)}`,
    "shadow-md": `0 ${px(4)} ${px(6)} -1px ${a(0.1)}, 0 ${px(2)} ${px(4)} -2px ${a(0.1)}`,
    "shadow-lg": `0 ${px(10)} ${px(15)} -3px ${a(0.1)}, 0 ${px(4)} ${px(6)} -4px ${a(0.1)}`,
    "shadow-xl": `0 ${px(20)} ${px(25)} -5px ${a(0.1)}, 0 ${px(8)} ${px(10)} -6px ${a(0.1)}`,
    "shadow-2xl": `0 ${px(25)} ${px(50)} -12px ${a(0.25)}`,
  };
}

/** `default` writes no shadow keys, leaving `theme.css`' (Tailwind's). */
export const shadowPresets = {
  none: {
    label: "None",
    values: Object.fromEntries(shadowTokens.map((t) => [t, "0 0 #0000"])),
  },
  soft: { label: "Soft", values: shadows(0.5, 1.5) },
  default: { label: "Default", values: {} as Record<string, string> },
  strong: { label: "Strong", values: shadows(2.5, 1) },
} as const;

export type ShadowPreset = keyof typeof shadowPresets;

export function shadowPresetOf(shared: Record<string, string>): ShadowPreset | "custom" {
  for (const [name, preset] of Object.entries(shadowPresets)) {
    if (shadowTokens.every((t) => (shared[t] ?? "") === (preset.values[t] ?? "")))
      return name as ShadowPreset;
  }
  return "custom";
}

const geist = `"Geist Variable", ui-sans-serif, system-ui, sans-serif`;

/** System stacks and the bundled Geist only: no remote fonts. An empty value is the default. */
export const fontChoices: Record<(typeof fontTokens)[number], { label: string; value: string }[]> =
  {
    "font-sans": [
      { label: "Default (Geist)", value: "" },
      { label: "System", value: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif" },
      {
        label: "Humanist",
        value: "Seravek, 'Gill Sans Nova', Ubuntu, Calibri, 'DejaVu Sans', sans-serif",
      },
      { label: "Serif", value: "ui-serif, Charter, 'Bitstream Charter', Georgia, serif" },
      { label: "Monospace", value: "ui-monospace, SFMono-Regular, Menlo, monospace" },
    ],
    "font-heading": [
      { label: "Default (as body)", value: "" },
      { label: "Geist", value: geist },
      { label: "System", value: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif" },
      { label: "Serif", value: "ui-serif, Charter, 'Bitstream Charter', Georgia, serif" },
      {
        label: "Rounded",
        value: "ui-rounded, 'Hiragino Maru Gothic ProN', Quicksand, Comfortaa, sans-serif",
      },
    ],
    "font-mono": [
      { label: "Default (system mono)", value: "" },
      {
        label: "Code",
        value: "ui-monospace, 'Cascadia Code', 'Source Code Pro', Menlo, monospace",
      },
      { label: "Courier", value: "'Courier New', Courier, monospace" },
    ],
  };

export const fontLabels: Record<(typeof fontTokens)[number], string> = {
  "font-sans": "Body",
  "font-heading": "Headings",
  "font-mono": "Code",
};

export function tokenLabel(token: string): string {
  if (token === "foreground") return "Text";
  const words = token.replace(/-foreground$/, "").replaceAll("-", " ");
  const name = words.charAt(0).toUpperCase() + words.slice(1);
  return token.endsWith("-foreground") ? `${name} text` : name;
}

/** The form's values: every key the editor shows, `""` for "the default". */
export function draftOf(tokens: Partial<ThemeTokens> | null | undefined): ThemeTokens {
  const fill = (keys: string[], values: Record<string, string> = {}) =>
    Object.fromEntries(keys.map((k) => [k, values[k] ?? ""]));
  return {
    light: fill(colourTokens, tokens?.light),
    dark: fill(colourTokens, tokens?.dark),
    shared: fill(sharedTokens, tokens?.shared),
  };
}

/** Only the overrides: what is saved and previewed. */
export function overridesOf(draft: ThemeTokens): ThemeTokens {
  const keep = (values: Record<string, string>) =>
    Object.fromEntries(Object.entries(values).filter(([, v]) => v.trim() !== ""));
  return { light: keep(draft.light), dark: keep(draft.dark), shared: keep(draft.shared) };
}

export const formFields = [
  ...colourTokens.flatMap((t) => [`light.${t}`, `dark.${t}`]),
  ...sharedTokens.map((t) => `shared.${t}`),
];

/**
 * `theme.css`' own values, read from the stylesheets rather than computed, so the org's theme
 * (`<style id="app-theme">`) and the viewer's light or dark don't colour them.
 */
export function readDefaults(): ThemeTokens {
  const defaults: ThemeTokens = { light: {}, dark: {}, shared: {} };
  const visit = (rules: CSSRuleList) => {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSStyleRule) {
        const selectors = rule.selectorText.split(",").map((s) => s.trim());
        const into = selectors.includes(".dark")
          ? defaults.dark
          : selectors.includes(":root")
            ? defaults.light
            : null;
        if (!into) continue;
        for (const property of Array.from(rule.style)) {
          if (!property.startsWith("--")) continue;
          const name = property.slice(2);
          const value = rule.style.getPropertyValue(property).trim();
          if (sharedTokens.includes(name)) defaults.shared[name] ??= value;
          else if (into === defaults.dark || !(name in into)) into[name] = value;
        }
      } else if ("cssRules" in rule) {
        visit((rule as CSSGroupingRule).cssRules);
      }
    }
  };
  for (const sheet of Array.from(document.styleSheets)) {
    if ((sheet.ownerNode as Element | null)?.id === "app-theme") continue;
    try {
      visit(sheet.cssRules);
    } catch {}
  }
  return defaults;
}

let canvas: CanvasRenderingContext2D | null | undefined;

/** Any CSS colour as `#rrggbb` for `<input type=color>`, which takes nothing else. */
export function toHex(colour: string): string {
  if (/^#[0-9a-f]{6}$/i.test(colour)) return colour.toLowerCase();
  if (canvas === undefined) {
    try {
      // jsdom has a canvas without a context, and logs an error for asking.
      canvas = /jsdom/.test(navigator.userAgent)
        ? null
        : document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    } catch {
      canvas = null;
    }
  }
  if (!canvas || !colour) return "#000000";
  canvas.clearRect(0, 0, 1, 1);
  canvas.fillStyle = "#000";
  canvas.fillStyle = colour;
  canvas.fillRect(0, 0, 1, 1);
  const [r, g, b] = canvas.getImageData(0, 0, 1, 1).data;
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}
