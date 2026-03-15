/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{svelte,js,ts}"],
  theme: {
    extend: {
      colors: {
        bg: {
          primary: "#020617",
          secondary: "#0F172A",
          tertiary: "#1E293B",
          elevated: "#334155",
        },
        accent: {
          DEFAULT: "#22C55E",
          hover: "#16A34A",
          muted: "rgba(34, 197, 94, 0.15)",
        },
        border: {
          DEFAULT: "#334155",
          hover: "#475569",
        },
      },
      fontFamily: {
        sans: [
          "Fira Sans",
          "system-ui",
          "-apple-system",
          "BlinkMacSystemFont",
          "sans-serif",
        ],
        mono: ["Fira Code", "monospace"],
      },
    },
  },
  plugins: [],
};
