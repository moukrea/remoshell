import { defineConfig, loadEnv } from 'vite';
import solid from 'vite-plugin-solid';
import UnoCSS from 'unocss/vite';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const base = env.BASE_URL || '/';

  return {
    base,
    plugins: [
      UnoCSS(),
      solid(),
      VitePWA({
        registerType: 'autoUpdate',
        includeAssets: ['icon.svg', 'apple-touch-icon.png'],
        manifest: {
          name: 'RemoteShell',
          short_name: 'RemoteShell',
          description: 'Remote terminal access via WebRTC',
          theme_color: '#1a1a2e',
          background_color: '#0f0f1a',
          display: 'standalone',
          orientation: 'any',
          start_url: base,
          scope: base,
          icons: [
            {
              src: 'pwa-192x192.png',
              sizes: '192x192',
              type: 'image/png',
            },
            {
              src: 'pwa-512x512.png',
              sizes: '512x512',
              type: 'image/png',
            },
            {
              src: 'pwa-512x512.png',
              sizes: '512x512',
              type: 'image/png',
              purpose: 'maskable',
            },
          ],
        },
        workbox: {
          globPatterns: ['**/*.{js,css,html,ico,png,svg,woff,woff2}'],
          runtimeCaching: [
            {
              urlPattern: /^https:\/\/api\./i,
              handler: 'NetworkFirst',
              options: {
                cacheName: 'api-cache',
                expiration: {
                  maxEntries: 50,
                  maxAgeSeconds: 60 * 60 * 24, // 24 hours
                },
                cacheableResponse: {
                  statuses: [0, 200],
                },
              },
            },
            {
              urlPattern: /\.(?:js|css|woff2?)$/i,
              handler: 'CacheFirst',
              options: {
                cacheName: 'static-assets',
                expiration: {
                  maxEntries: 100,
                  maxAgeSeconds: 60 * 60 * 24 * 30, // 30 days
                },
              },
            },
            {
              urlPattern: /\.(?:png|jpg|jpeg|svg|gif|webp|ico)$/i,
              handler: 'StaleWhileRevalidate',
              options: {
                cacheName: 'image-cache',
                expiration: {
                  maxEntries: 50,
                  maxAgeSeconds: 60 * 60 * 24 * 7, // 7 days
                },
              },
            },
          ],
          navigateFallback: 'index.html',
          navigateFallbackDenylist: [/^\/api\//],
        },
        devOptions: {
          enabled: false, // Disable in development
        },
      }),
    ],
    server: {
      port: 3000,
    },
    build: {
      target: 'esnext',
      // Performance: minify for smaller bundle size
      minify: 'esbuild',
      // Performance: enable source maps for debugging but keep them separate
      sourcemap: true,
      // Performance: set chunk size warning threshold
      chunkSizeWarningLimit: 500, // 500KB target
      rollupOptions: {
        output: {
          // Performance: manual chunk splitting for better caching
          manualChunks: {
            // Core framework - cached separately
            'vendor-solid': ['solid-js'],
            // Terminal - heavy dependency, loaded on demand
            'vendor-xterm': ['@xterm/xterm', '@xterm/addon-webgl', '@xterm/addon-fit'],
            // WebRTC - for peer connections
            'vendor-webrtc': ['simple-peer'],
            // QR scanning - only needed for pairing
            'vendor-qr': ['jsqr'],
            // Protocol/serialization
            'vendor-msgpack': ['@msgpack/msgpack'],
          },
        },
      },
    },
    // Performance: optimize dependencies
    optimizeDeps: {
      include: ['solid-js', '@xterm/xterm'],
    },
  };
});
