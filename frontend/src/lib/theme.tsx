import { type ReactNode, createContext, useCallback, useContext, useEffect, useState } from "react";

export type Theme = "light" | "dark" | "system";

/** Also read by the inline script in `index.html`, which applies it before first paint. */
const storageKey = "app-theme";

const darkQuery = "(prefers-color-scheme: dark)";

function storedTheme(): Theme {
  try {
    const stored = localStorage.getItem(storageKey);
    if (stored === "light" || stored === "dark" || stored === "system") return stored;
  } catch {
    // Storage can be unavailable (private windows, blocked site data): follow the system.
  }
  return "system";
}

function systemIsDark() {
  return typeof window.matchMedia === "function" && window.matchMedia(darkQuery).matches;
}

type Scheme = "light" | "dark";

const ThemeContext = createContext<{
  theme: Theme;
  /** What shows: `theme`, or the system's scheme when it is "system". */
  scheme: Scheme;
  setTheme: (theme: Theme) => void;
} | null>(null);

/** Puts `.dark` on `<html>` for the user's chosen theme, following the system's when "system". */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(storedTheme);
  const [scheme, setScheme] = useState<Scheme>(() =>
    theme === "dark" || (theme === "system" && systemIsDark()) ? "dark" : "light",
  );

  useEffect(() => {
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && systemIsDark());
      document.documentElement.classList.toggle("dark", dark);
      setScheme(dark ? "dark" : "light");
    };
    apply();
    if (theme !== "system" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(darkQuery);
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    try {
      localStorage.setItem(storageKey, next);
    } catch {
      // Not remembered, but still applied for this visit.
    }
    setThemeState(next);
  }, []);

  return (
    <ThemeContext.Provider value={{ theme, scheme, setTheme }}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("useTheme needs a ThemeProvider");
  return context;
}
