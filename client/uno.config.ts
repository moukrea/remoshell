import { defineConfig, presetUno, presetWebFonts } from 'unocss';

export default defineConfig({
  presets: [
    presetUno(),
    presetWebFonts({
      fonts: {
        mono: 'JetBrains Mono',
      },
    }),
  ],
  theme: {
    colors: {
      terminal: {
        bg: '#1e1e2e',
        fg: '#cdd6f4',
        selection: '#45475a',
        cursor: '#f5e0dc',
      },
      // High contrast colors for accessibility
      'hc-bg': '#000000',
      'hc-fg': '#ffffff',
      'hc-link': '#ffff00',
      'hc-focus': '#00ffff',
    },
  },
  shortcuts: {
    'terminal-container': 'w-full h-full bg-terminal-bg font-mono',
    // Skip link styling - hidden until focused
    'skip-link': 'sr-only focus:not-sr-only focus:absolute focus:z-50 focus:top-4 focus:left-4 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded focus:outline-2 focus:outline-offset-2 focus:outline-blue-300',
    // Focus visible styles
    'focus-visible-ring': 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-500',
    // Accessible button base
    'btn-a11y': 'focus-visible-ring min-h-11 min-w-11 cursor-pointer',
    // High contrast mode support
    'hc-mode': '@media (prefers-contrast: more) { bg-hc-bg text-hc-fg }',
  },
  // Safelist for dynamic classes
  safelist: [
    'sr-only',
    'not-sr-only',
    'focus-visible:outline',
    'focus-visible:outline-2',
    'focus-visible:outline-offset-2',
  ],
  // Custom preflights for accessibility
  preflights: [
    {
      getCSS: () => `
        /* Screen reader only class */
        .sr-only {
          position: absolute;
          width: 1px;
          height: 1px;
          padding: 0;
          margin: -1px;
          overflow: hidden;
          clip: rect(0, 0, 0, 0);
          white-space: nowrap;
          border: 0;
        }

        /* Remove sr-only when focused */
        .not-sr-only {
          position: static;
          width: auto;
          height: auto;
          padding: 0;
          margin: 0;
          overflow: visible;
          clip: auto;
          white-space: normal;
        }

        /* Reduced motion preference */
        @media (prefers-reduced-motion: reduce) {
          .motion-reduce {
            animation: none;
            transition: none;
          }
        }
      `,
    },
  ],
});
