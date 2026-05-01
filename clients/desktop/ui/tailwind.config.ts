import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./popover.html", "./modal.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
    },
  },
} satisfies Config;
