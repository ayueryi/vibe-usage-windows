/**
 * Semantic colors shared by both themes. The original macOS palette is the
 * basis of the dark theme; src/styles/themes.css defines each theme's values.
 */
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ...Object.fromEntries([
          "app", "card", "card-border", "filter-row", "t-primary", "t-secondary",
          "t-tertiary", "t-muted", "cost", "active", "link", "danger", "quota-warn",
          "quota-critical", "button", "input", "input-border", "focus", "hover-border",
          "pressed", "tooltip", "action", "on-action", "on-danger", "toggle-thumb",
        ].map((name) => [name, `var(--color-${name})`])),
      },
      fontFamily: {
        sans: ["-apple-system", "Segoe UI", "Microsoft YaHei UI", "PingFang SC", "sans-serif"],
        mono: ["JetBrains Mono", "SF Mono", "Consolas", "monospace"],
      },
      borderRadius: {
        card: "4px",
        panel: "12px",
      },
    },
  },
  plugins: [],
};
