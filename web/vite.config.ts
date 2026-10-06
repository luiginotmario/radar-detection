import { defineConfig } from 'vite';

// while working on the screen, `pnpm dev` serves it with instant reload and forwards
// the live feed from blip (cargo run) running on port 7070.
export default defineConfig({
  server: {
    host: true,
    proxy: { '/ws': { target: 'ws://127.0.0.1:7070', ws: true } },
  },
  build: { chunkSizeWarningLimit: 1000 },
});
