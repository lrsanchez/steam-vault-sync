/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        accent: {
          blue: "#378ADD",
          green: "#1D9E75",
          coral: "#E47B6B",
          purple: "#9B7BD4",
        },
      },
      fontFamily: {
        sans: ["Segoe UI", "system-ui", "-apple-system", "sans-serif"],
      },
      gridTemplateColumns: {
        games: "repeat(auto-fill, minmax(148px, 1fr))",
      },
    },
  },
  plugins: [],
};
