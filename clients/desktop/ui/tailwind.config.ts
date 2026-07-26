import type { Config } from "tailwindcss";

/**
 * Semantic names only. Every value resolves to a `:root` custom property in
 * `src/styles.css`, so the palette has exactly one definition and the Main
 * Window inherits it when it is converted.
 */
export default {
  content: ["./popover.html", "./main.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        void: {
          1000: "var(--void-1000)",
          900: "var(--void-900)",
          800: "var(--void-800)",
          700: "var(--void-700)",
          600: "var(--void-600)",
          400: "var(--void-400)",
        },
        cyan: {
          300: "var(--cyan-300)",
          400: "var(--cyan-400)",
          500: "var(--cyan-500)",
          600: "var(--cyan-600)",
          700: "var(--cyan-700)",
          800: "var(--cyan-800)",
        },
        amber: { 400: "var(--amber-400)" },
        alert: { 400: "var(--alert-400)" },
        nominal: { 400: "var(--nominal-400)" },
        standby: { 400: "var(--standby-400)" },
        surface: {
          panel: "var(--surface-panel)",
          recess: "var(--surface-recess)",
          band: "var(--surface-band)",
          active: "var(--surface-active)",
        },
        text: {
          emitter: "var(--text-emitter)",
          body: "var(--text-body)",
          muted: "var(--text-muted)",
          dim: "var(--text-dim)",
        },
      },
      borderColor: {
        hairline: "var(--cyan-a12)",
        emitter: "var(--cyan-a40)",
      },
      fontFamily: {
        ui: "var(--font-ui)",
        mono: "var(--font-mono)",
        display: "var(--font-display)",
      },
      fontSize: {
        data: ["var(--size-data)", { lineHeight: "1.3" }],
        chrome: ["10px", { lineHeight: "1.2" }],
        label: ["11px", { lineHeight: "1.2" }],
      },
      letterSpacing: {
        word: "var(--track-word)",
        phrase: "var(--track-phrase)",
      },
      height: { row: "var(--row-h)" },
      transitionDuration: {
        fast: "var(--dur-fast)",
        mid: "var(--dur-mid)",
      },
    },
  },
} satisfies Config;
