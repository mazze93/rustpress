import { defineConfig } from "astro/config";

// Fully static Workers Assets; no runtime adapter or Worker code.
export default defineConfig({
  site: "https://studio.mazzeleczzare.com",
  output: "static",
  trailingSlash: "always",
  build: { format: "directory", inlineStylesheets: "never" },
});
