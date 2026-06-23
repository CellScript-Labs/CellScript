import { defineConfig } from "astro/config";

export default defineConfig({
  site: "http://cellscript.dev",
  base: "/",
  output: "static",
  devToolbar: {
    enabled: false,
  },
  vite: {
    resolve: {
      alias: {
        buffer: "buffer/",
      },
    },
    define: {
      global: "globalThis",
    },
    optimizeDeps: {
      include: ["buffer"],
    },
  },
});
