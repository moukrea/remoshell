import { Component, createSignal, onMount, onCleanup, createEffect } from 'solid-js';
import { Terminal, ITerminalOptions } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import '@xterm/xterm/css/xterm.css';
import {
  startTiming,
  endTiming,
  recordSample,
  MetricNames,
} from '../../lib/performance';

/** Flow control watermarks in bytes */
const HIGH_WATERMARK = 500 * 1024; // 500KB - pause when buffer exceeds this
const LOW_WATERMARK = 100 * 1024;  // 100KB - resume when buffer drops below this

/** Performance optimization: scrollback limit for memory efficiency */
const SCROLLBACK_LIMIT = 3000;

/** Write counter for unique timing IDs */
let writeCounter = 0;

export interface TerminalTheme {
  background?: string;
  foreground?: string;
  cursor?: string;
  cursorAccent?: string;
  selectionBackground?: string;
  selectionForeground?: string;
  selectionInactiveBackground?: string;
  black?: string;
  red?: string;
  green?: string;
  yellow?: string;
  blue?: string;
  magenta?: string;
  cyan?: string;
  white?: string;
  brightBlack?: string;
  brightRed?: string;
  brightGreen?: string;
  brightYellow?: string;
  brightBlue?: string;
  brightMagenta?: string;
  brightCyan?: string;
  brightWhite?: string;
}

export interface XTermWrapperProps {
  /** Called when user types input */
  onData?: (data: string) => void;
  /** Called when terminal is resized */
  onResize?: (cols: number, rows: number) => void;
  /** Called when flow control signals pause (buffer too full) */
  onPause?: () => void;
  /** Called when flow control signals resume (buffer drained) */
  onResume?: () => void;
  /** Terminal theme configuration */
  theme?: TerminalTheme;
  /** Additional terminal options */
  options?: Partial<ITerminalOptions>;
  /** CSS class for the container */
  class?: string;
  /** Callback ref for parent access to terminal handle */
  ref?: (handle: XTermWrapperHandle) => void;
}

export interface XTermWrapperHandle {
  /** Write data to the terminal with flow control */
  write: (data: string | Uint8Array) => void;
  /** Clear the terminal screen */
  clear: () => void;
  /** Focus the terminal */
  focus: () => void;
  /** Get current dimensions */
  getDimensions: () => { cols: number; rows: number } | null;
  /** Get the underlying Terminal instance (use sparingly) */
  getTerminal: () => Terminal | null;
}

/**
 * SolidJS wrapper for xterm.js with WebGL rendering and flow control.
 *
 * Flow control prevents memory exhaustion when receiving data faster than
 * it can be rendered. When the write buffer exceeds HIGH_WATERMARK, onPause
 * is called. When it drops below LOW_WATERMARK, onResume is called.
 */
const XTermWrapper: Component<XTermWrapperProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let terminal: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let webglAddon: WebglAddon | null = null;
  let resizeObserver: ResizeObserver | null = null;

  const [bufferSize, setBufferSize] = createSignal(0);
  const [isPaused, setIsPaused] = createSignal(false);

  // Expose handle via a ref-like pattern for parent access
  const handle: XTermWrapperHandle = {
    write: (data: string | Uint8Array) => {
      if (!terminal) return;

      // Start performance timing for this write
      const writeId = `${++writeCounter}`;
      startTiming(MetricNames.TERMINAL_WRITE, writeId);

      const dataSize = typeof data === 'string' ? data.length : data.length;
      const newBufferSize = bufferSize() + dataSize;
      setBufferSize(newBufferSize);

      // Check if we need to pause
      if (!isPaused() && newBufferSize >= HIGH_WATERMARK) {
        setIsPaused(true);
        props.onPause?.();
      }

      terminal.write(data, () => {
        // End performance timing - includes render time
        const duration = endTiming(MetricNames.TERMINAL_WRITE, writeId);
        if (duration !== undefined) {
          recordSample(MetricNames.TERMINAL_RENDER, duration);
        }

        // Callback fires when data has been processed
        const updatedSize = Math.max(0, bufferSize() - dataSize);
        setBufferSize(updatedSize);

        // Check if we can resume
        if (isPaused() && updatedSize <= LOW_WATERMARK) {
          setIsPaused(false);
          props.onResume?.();
        }
      });
    },
    clear: () => {
      terminal?.clear();
    },
    focus: () => {
      terminal?.focus();
    },
    getDimensions: () => {
      if (!terminal) return null;
      return { cols: terminal.cols, rows: terminal.rows };
    },
    getTerminal: () => terminal,
  };

  onMount(() => {
    if (!containerRef) return;

    // Create terminal with merged options - performance optimized
    const terminalOptions: ITerminalOptions = {
      cursorBlink: true,
      cursorStyle: 'block',
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      fontSize: 14,
      lineHeight: 1.2,
      // Performance: limit scrollback to reduce memory usage
      scrollback: SCROLLBACK_LIMIT,
      // Performance: enable fast scroll with alt key
      fastScrollModifier: 'alt',
      fastScrollSensitivity: 5,
      // Performance: disable smooth scrolling for faster rendering
      smoothScrollDuration: 0,
      // Performance: use GPU for text rendering when available
      allowProposedApi: true,
      ...props.options,
      theme: props.theme,
    };

    terminal = new Terminal(terminalOptions);

    // Load FitAddon
    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    // Open terminal in container
    terminal.open(containerRef);

    // Try to load WebGL addon, fallback to canvas if it fails
    try {
      webglAddon = new WebglAddon();
      webglAddon.onContextLoss(() => {
        // WebGL context was lost, dispose and let terminal fall back to canvas
        webglAddon?.dispose();
        webglAddon = null;
      });
      terminal.loadAddon(webglAddon);
    } catch (e) {
      // WebGL not supported, terminal will use canvas renderer
      console.warn('WebGL addon failed to load, using canvas renderer:', e);
      webglAddon = null;
    }

    // Initial fit
    fitAddon.fit();

    // Forward user input
    terminal.onData((data) => {
      props.onData?.(data);
    });

    // Forward resize events with timing
    terminal.onResize(({ cols, rows }) => {
      startTiming(MetricNames.TERMINAL_RESIZE);
      props.onResize?.(cols, rows);
      endTiming(MetricNames.TERMINAL_RESIZE);
    });

    // Set up ResizeObserver for container resize handling
    resizeObserver = new ResizeObserver(() => {
      // Use requestAnimationFrame to batch resize operations
      requestAnimationFrame(() => {
        if (fitAddon && terminal) {
          fitAddon.fit();
        }
      });
    });
    resizeObserver.observe(containerRef);

    // Call the ref callback to expose handle to parent
    props.ref?.(handle);
  });

  // Effect to update theme when props change
  createEffect(() => {
    if (terminal && props.theme) {
      terminal.options.theme = props.theme;
    }
  });

  onCleanup(() => {
    resizeObserver?.disconnect();
    webglAddon?.dispose();
    fitAddon?.dispose();
    terminal?.dispose();

    resizeObserver = null;
    webglAddon = null;
    fitAddon = null;
    terminal = null;
  });

  return (
    <div
      ref={containerRef}
      class={props.class}
      style={{ width: '100%', height: '100%' }}
      role="application"
      aria-label="Terminal"
      aria-roledescription="Interactive terminal"
      data-testid="xterm-container"
    />
  );
};

// Export the handle type and watermark constants for testing
export { HIGH_WATERMARK, LOW_WATERMARK };
export default XTermWrapper;
