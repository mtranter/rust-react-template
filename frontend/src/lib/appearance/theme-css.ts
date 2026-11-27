export type ThemeTokens = {
  light: Record<string, string>;
  dark: Record<string, string>;
  shared: Record<string, string>;
};

export const emptyTheme: ThemeTokens = { light: {}, dark: {}, shared: {} };

// A value can't close the rule or open another: it is the admin's, but it ends up in a <style>.
function declarations(values: Record<string, string>): string {
  return Object.entries(values)
    .filter(([key, value]) => /^[a-zA-Z0-9-]+$/.test(key) && !/[{};<>]/.test(value) && value.trim())
    .map(([key, value]) => `--${key}:${value.trim()};`)
    .join("");
}

/**
 * The CSS overriding `theme.css`: `light` and `shared` on `:root`, `dark` on `.dark`. Unlayered, so
 * it wins over Tailwind's `@theme` values (`@layer theme`) for fonts and shadows as well.
 */
export function themeCss(tokens: ThemeTokens): string {
  const root = declarations({ ...tokens.light, ...tokens.shared });
  const dark = declarations(tokens.dark);
  return `${root ? `:root{${root}}` : ""}${dark ? `.dark{${dark}}` : ""}`;
}
