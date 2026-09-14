import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";

const SITE = "https://sachncs.github.io/find";

export default defineConfig({
  site: SITE,
  base: "/find",
  output: "static",
  trailingSlash: "ignore",
  integrations: [react(), sitemap()],
  vite: {
    plugins: [tailwindcss()],
    ssr: {
      noExternal: ["framer-motion"],
    },
  },
  build: {
    inlineStylesheets: "auto",
  },
  compressHTML: true,
});