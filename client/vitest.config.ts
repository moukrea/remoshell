import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';

export default defineConfig({
  // @ts-expect-error - vite version mismatch between vitest and vite-plugin-solid
  plugins: [solid()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    deps: {
      optimizer: {
        web: {
          include: ['solid-js'],
        },
      },
    },
  },
});
