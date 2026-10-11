import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onThemeUpdated } from "../lib/api";
import { applyTheme, cachedThemePreference, normalizeThemePreference, THEME_STORAGE_KEY } from "../lib/theme";
import type { ThemePreference } from "../lib/types";
import { formatInvokeError } from "../lib/errors";

const ThemeContext = createContext<{
  preference: ThemePreference;
  setPreference: (value: ThemePreference) => Promise<void>;
  saving: boolean;
  error: string | null;
} | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState(cachedThemePreference);
  const [systemDark, setSystemDark] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(0);
  const native = "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const changed = () => setSystemDark(media.matches);
    media.addEventListener("change", changed);
    changed();
    return () => media.removeEventListener("change", changed);
  }, []);

  useEffect(() => {
    applyTheme(preference, systemDark);
    if (native) {
      // null restores native Windows title bar and menu theme tracking.
      void getCurrentWindow().setTheme(preference === "system" ? null : preference)
        .catch((cause) => console.warn("Unable to update native window theme:", cause));
    }
  }, [preference, systemDark, native]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const readRevision = revision.current;
    if (native) {
      // Subscribe before reading so a settings change cannot disappear between
      // the initial snapshot and listener registration (including hidden windows).
      void (async () => {
        try {
          const stop = await onThemeUpdated((nextPreference) => {
            if (disposed) return;
            revision.current++;
            setPreferenceState(normalizeThemePreference(nextPreference));
          });
          if (disposed) { stop(); return; }
          unlisten = stop;
          const settings = await api.getSettings();
          if (!disposed && revision.current === readRevision) {
            setPreferenceState(normalizeThemePreference(settings.themePreference));
          }
        } catch (cause) {
          if (!disposed) console.warn("Unable to load theme preference:", cause);
        }
      })();
    }
    const storageChanged = (event: StorageEvent) => {
      if (!native && (event.key === THEME_STORAGE_KEY || event.key === null)) {
        revision.current++;
        setPreferenceState(cachedThemePreference());
      }
    };
    window.addEventListener("storage", storageChanged);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("storage", storageChanged);
    };
  }, [native]);

  const setPreference = async (value: ThemePreference) => {
    if (saving) return;
    const request = ++revision.current;
    setSaving(true);
    setError(null);
    try {
      if (native) await api.setThemePreference(value);
      // A newer event from the other window wins over this command's response.
      if (revision.current === request) setPreferenceState(value);
    } catch (cause) {
      setError(`主题保存失败：${formatInvokeError(cause)}`);
    } finally {
      setSaving(false);
    }
  };

  return <ThemeContext.Provider value={{ preference, setPreference, saving, error }}>{children}</ThemeContext.Provider>;
}

export function useThemePreference() {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useThemePreference outside ThemeProvider");
  return value;
}
