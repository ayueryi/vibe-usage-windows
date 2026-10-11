import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme, cachedThemePreference, normalizeThemePreference, resolveTheme, THEME_STORAGE_KEY } from "../src/lib/theme";

afterEach(() => vi.unstubAllGlobals());

describe("theme preference", () => {
  it("follows system unless explicitly overridden, including old/invalid settings", () => {
    for (const value of [undefined, null, "unknown", 42]) {
      expect(normalizeThemePreference(value)).toBe("system");
    }
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("applies system changes without replacing the saved system preference", () => {
    const dataset: Record<string, string> = {};
    const setItem = vi.fn();
    vi.stubGlobal("document", { documentElement: { dataset } });
    vi.stubGlobal("localStorage", { setItem });
    applyTheme("system", false);
    expect(dataset.theme).toBe("light");
    applyTheme("system", true);
    expect(dataset.theme).toBe("dark");
    expect(setItem).toHaveBeenLastCalledWith(THEME_STORAGE_KEY, "system");
  });

  it("still renders when first-paint storage is unavailable", () => {
    const dataset: Record<string, string> = {};
    vi.stubGlobal("document", { documentElement: { dataset } });
    vi.stubGlobal("localStorage", {
      getItem: () => { throw new Error("blocked"); },
      setItem: () => { throw new Error("full"); },
    });
    expect(cachedThemePreference()).toBe("system");
    expect(() => applyTheme("light", true)).not.toThrow();
    expect(dataset.theme).toBe("light");
  });
});
