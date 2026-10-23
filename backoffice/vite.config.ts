import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const rootDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(rootDir, '..');

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    port: 5174,
    strictPort: true,
    fs: {
      strict: false,
      allow: [repoRoot, rootDir]
    }
  }
});
