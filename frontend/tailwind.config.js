/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
      colors: {
        ink: "#0b0d0e",
        panel: "#101314",
        line: "#1f2326",
        mint: "#4ade80",
        amber: "#f5b942",
        fog: "#8b969e",
      },
    },
  },
  plugins: [],
};
