import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config';

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      globals: true,
      deps: {
        optimizer: {
          web: {
            include: ['solid-js'],
          },
        },
      },
    },
  })
);
