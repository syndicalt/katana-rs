import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri expects a fixed port for the dev server.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // assets/midi.xml demo copy lives in public/, no fs allow needed.
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})
