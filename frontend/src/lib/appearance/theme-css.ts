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
 * The CSS overriding `theme.css`: `light` on the root out of dark mode, `shared` on the root, `dark`
 * on `.dark` (the root, or the editor's preview). Unlayered, so it wins over Tailwind's `@theme`
 * values (`@layer theme`) for fonts and shadows as well. Each selector outranks `theme.css`'
 * (`:root`, `.dark`), so it wins wherever it sits in `<head>` (`index.html` writes it before
 * Vite's stylesheet), and a light override never reaches dark mode.
 */
export function themeCss(tokens: ThemeTokens): string {
  const rule = (selector: string, values: Record<string, string>) => {
    const body = declarations(values);
    return body ? `${selector}{${body}}` : "";
  };
  return (
    rule(":root:not(.dark)", tokens.light) +
    rule(":root:root", tokens.shared) +
    rule(".dark.dark", tokens.dark)
  );
}
