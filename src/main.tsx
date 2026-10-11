import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/globals.css";
import { PopoverApp } from "./PopoverApp";
import { SettingsApp } from "./SettingsApp";
import { AppStateProvider } from "./state/AppStateContext";
import { ThemeProvider } from "./state/ThemeContext";
import { applyTheme, cachedThemePreference } from "./lib/theme";

// Single HTML entry. Packaged Tauri windows route by label; the query param is
// kept as a lightweight browser/dev fallback.
function getWindowRoute() {
  const route = new URLSearchParams(window.location.search).get("window");
  if (route) return route;

  try {
    return getCurrentWindow().label;
  } catch {
    return "popover";
  }
}

const route = getWindowRoute();
document.documentElement.dataset.window = route;
applyTheme(cachedThemePreference(), matchMedia("(prefers-color-scheme: dark)").matches);

const root = ReactDOM.createRoot(document.getElementById("root")!);

if (route === "settings") {
  root.render(
    <React.StrictMode>
      <ThemeProvider>
        <SettingsApp />
      </ThemeProvider>
    </React.StrictMode>,
  );
} else {
  root.render(
    <React.StrictMode>
      <ThemeProvider>
        <AppStateProvider>
          <PopoverApp />
        </AppStateProvider>
      </ThemeProvider>
    </React.StrictMode>,
  );
}
