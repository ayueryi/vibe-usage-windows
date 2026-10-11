import type { ThemePreference } from "./types";

export const THEME_STORAGE_KEY = "vibe-usage-theme";

export function normalizeThemePreference(value: unknown): ThemePreference {
  return value === "light" || value === "dark" ? value : "system";
}

export function resolveTheme(preference: ThemePreference, systemDark: boolean): "light" | "dark" {
  return preference === "system" ? (systemDark ? "dark" : "light") : preference;
}

/** Cache only for first paint; the Rust settings file remains authoritative. */
export function cachedThemePreference(): ThemePreference {
  try {
    return normalizeThemePreference(localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "system";
  }
}

export function applyTheme(preference: ThemePreference, systemDark: boolean): void {
  document.documentElement.dataset.theme = resolveTheme(preference, systemDark);
  try {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // A blocked/full browser cache must not prevent a theme change.
  }
}
