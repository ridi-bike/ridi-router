import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    minify: 'esbuild',
  },
  base: '/',  // Relative paths for embedding
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:1337'
    }
  },
})
