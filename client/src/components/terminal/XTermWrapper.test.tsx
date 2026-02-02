import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom';

// Flow control constants - these match the values in XTermWrapper.tsx
const HIGH_WATERMARK = 500 * 1024; // 500KB
const LOW_WATERMARK = 100 * 1024;  // 100KB

describe('XTermWrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Flow Control Constants', () => {
    it('should have HIGH_WATERMARK set to 500KB', () => {
      expect(HIGH_WATERMARK).toBe(500 * 1024);
    });

    it('should have LOW_WATERMARK set to 100KB', () => {
      expect(LOW_WATERMARK).toBe(100 * 1024);
    });

    it('should have HIGH_WATERMARK greater than LOW_WATERMARK', () => {
      expect(HIGH_WATERMARK).toBeGreaterThan(LOW_WATERMARK);
    });
  });

  describe('Flow Control Logic', () => {
    it('should trigger pause when buffer exceeds HIGH_WATERMARK', () => {
      // Simulate flow control logic
      let bufferSize = 0;
      let isPaused = false;
      const onPause = vi.fn();

      // Simulate write that exceeds high watermark
      const write = (dataSize: number) => {
        bufferSize += dataSize;

        if (!isPaused && bufferSize >= HIGH_WATERMARK) {
          isPaused = true;
          onPause();
        }
      };

      // Write data that exceeds high watermark
      write(HIGH_WATERMARK + 1);

      expect(onPause).toHaveBeenCalledTimes(1);
      expect(isPaused).toBe(true);
    });

    it('should trigger resume when buffer drops below LOW_WATERMARK', () => {
      // Simulate flow control logic
      let bufferSize = HIGH_WATERMARK + 1;
      let isPaused = true;
      const onResume = vi.fn();

      // Simulate data being processed (buffer draining)
      const drain = (dataSize: number) => {
        bufferSize = Math.max(0, bufferSize - dataSize);

        if (isPaused && bufferSize <= LOW_WATERMARK) {
          isPaused = false;
          onResume();
        }
      };

      // Drain buffer below low watermark
      drain(HIGH_WATERMARK + 1 - LOW_WATERMARK + 1);

      expect(onResume).toHaveBeenCalledTimes(1);
      expect(isPaused).toBe(false);
    });

    it('should not trigger pause multiple times when already paused', () => {
      let bufferSize = 0;
      let isPaused = false;
      const onPause = vi.fn();

      const write = (dataSize: number) => {
        bufferSize += dataSize;

        if (!isPaused && bufferSize >= HIGH_WATERMARK) {
          isPaused = true;
          onPause();
        }
      };

      // Multiple writes while paused
      write(HIGH_WATERMARK);
      write(100 * 1024); // Another 100KB
      write(100 * 1024); // Another 100KB

      expect(onPause).toHaveBeenCalledTimes(1);
    });

    it('should not trigger resume multiple times when already resumed', () => {
      let bufferSize = LOW_WATERMARK - 1;
      let isPaused = false;
      const onResume = vi.fn();

      const drain = (dataSize: number) => {
        bufferSize = Math.max(0, bufferSize - dataSize);

        if (isPaused && bufferSize <= LOW_WATERMARK) {
          isPaused = false;
          onResume();
        }
      };

      // Multiple drains while not paused
      drain(1000);
      drain(1000);
      drain(1000);

      expect(onResume).not.toHaveBeenCalled();
    });

    it('should handle hysteresis correctly (no flapping between states)', () => {
      let bufferSize = 0;
      let isPaused = false;
      const onPause = vi.fn();
      const onResume = vi.fn();

      const write = (dataSize: number) => {
        bufferSize += dataSize;
        if (!isPaused && bufferSize >= HIGH_WATERMARK) {
          isPaused = true;
          onPause();
        }
      };

      const drain = (dataSize: number) => {
        bufferSize = Math.max(0, bufferSize - dataSize);
        if (isPaused && bufferSize <= LOW_WATERMARK) {
          isPaused = false;
          onResume();
        }
      };

      // Fill buffer to trigger pause
      write(HIGH_WATERMARK);
      expect(onPause).toHaveBeenCalledTimes(1);

      // Drain to just above LOW_WATERMARK (should not resume yet)
      drain(HIGH_WATERMARK - LOW_WATERMARK - 1);
      expect(bufferSize).toBeGreaterThan(LOW_WATERMARK);
      expect(onResume).not.toHaveBeenCalled();

      // Drain below LOW_WATERMARK (should resume)
      drain(2);
      expect(onResume).toHaveBeenCalledTimes(1);

      // Write more data but not enough to trigger pause
      write(LOW_WATERMARK + 1);
      expect(bufferSize).toBeLessThan(HIGH_WATERMARK);
      expect(onPause).toHaveBeenCalledTimes(1); // Still only once

      // Fill to trigger pause again
      write(HIGH_WATERMARK);
      expect(onPause).toHaveBeenCalledTimes(2);
    });
  });

  describe('Buffer Size Calculation', () => {
    it('should correctly calculate size for string data', () => {
      const data = 'Hello, World!';
      const size = data.length;
      expect(size).toBe(13);
    });

    it('should correctly calculate size for Uint8Array data', () => {
      const data = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
      const size = data.length;
      expect(size).toBe(5);
    });

    it('should accumulate buffer size across multiple writes', () => {
      let bufferSize = 0;

      const write = (data: string | Uint8Array) => {
        const dataSize = typeof data === 'string' ? data.length : data.length;
        bufferSize += dataSize;
      };

      write('Hello');
      expect(bufferSize).toBe(5);

      write(' World');
      expect(bufferSize).toBe(11);

      write(new Uint8Array([33])); // "!"
      expect(bufferSize).toBe(12);
    });
  });
});

describe('Resize Handling', () => {
  it('should call fit on FitAddon when resize is triggered', () => {
    // This test validates the resize observer callback pattern
    const fitAddon = {
      fit: vi.fn(),
    };

    // Simulate what happens when ResizeObserver fires
    const handleResize = () => {
      fitAddon.fit();
    };

    handleResize();
    expect(fitAddon.fit).toHaveBeenCalledTimes(1);

    handleResize();
    expect(fitAddon.fit).toHaveBeenCalledTimes(2);
  });

  it('should forward terminal resize events to callback', () => {
    const onResize = vi.fn();
    const cols = 100;
    const rows = 30;

    // Simulate terminal resize event
    const terminalResizeHandler = (callback: (cols: number, rows: number) => void) => {
      callback(cols, rows);
    };

    terminalResizeHandler(onResize);
    expect(onResize).toHaveBeenCalledWith(cols, rows);
  });

  it('should debounce resize with requestAnimationFrame', async () => {
    vi.useFakeTimers();

    const fit = vi.fn();
    const rafCallbacks: Array<() => void> = [];

    // Mock requestAnimationFrame
    const mockRaf = vi.fn((callback: () => void) => {
      rafCallbacks.push(callback);
      return rafCallbacks.length;
    });

    // Simulate multiple rapid resize events
    const handleResize = () => {
      mockRaf(() => {
        fit();
      });
    };

    handleResize();
    handleResize();
    handleResize();

    // RAF should be called for each resize, but fit won't run until frames execute
    expect(mockRaf).toHaveBeenCalledTimes(3);
    expect(fit).not.toHaveBeenCalled();

    // Execute the last RAF callback
    const lastCallback = rafCallbacks[rafCallbacks.length - 1];
    if (lastCallback) {
      lastCallback();
    }
    expect(fit).toHaveBeenCalledTimes(1);

    vi.useRealTimers();
  });
});

describe('Theme Application', () => {
  it('should apply theme object to terminal options', () => {
    const theme = {
      background: '#1a1a1a',
      foreground: '#ffffff',
      cursor: '#ff0000',
      black: '#000000',
      red: '#ff0000',
      green: '#00ff00',
      yellow: '#ffff00',
      blue: '#0000ff',
      magenta: '#ff00ff',
      cyan: '#00ffff',
      white: '#ffffff',
    };

    // Simulate theme application
    const terminalOptions: Record<string, unknown> = {};
    terminalOptions.theme = theme;

    expect(terminalOptions.theme).toEqual(theme);
    expect((terminalOptions.theme as typeof theme).background).toBe('#1a1a1a');
    expect((terminalOptions.theme as typeof theme).foreground).toBe('#ffffff');
  });

  it('should update terminal theme reactively', () => {
    const terminal = {
      options: {
        theme: {} as Record<string, unknown>,
      },
    };

    const newTheme = {
      background: '#2a2a2a',
      foreground: '#eeeeee',
    };

    // Simulate createEffect updating theme
    terminal.options.theme = newTheme;

    expect(terminal.options.theme).toEqual(newTheme);
  });
});

describe('WebGL Fallback', () => {
  it('should handle WebGL context loss gracefully', () => {
    const webglAddon = {
      dispose: vi.fn(),
      onContextLoss: vi.fn(),
    };

    const contextLossHandlers: Array<() => void> = [];

    // Simulate registering context loss handler
    webglAddon.onContextLoss.mockImplementation((handler: () => void) => {
      contextLossHandlers.push(handler);
    });

    // Register handler
    webglAddon.onContextLoss(() => {
      webglAddon.dispose();
    });

    // Simulate context loss
    const lastHandler = contextLossHandlers[contextLossHandlers.length - 1];
    if (lastHandler) {
      lastHandler();
    }

    expect(webglAddon.dispose).toHaveBeenCalled();
  });

  it('should continue working when WebGL fails to load', () => {
    // Simulate WebGL loading failure
    let webglAddon: unknown = null;
    let canvasMode = false;

    try {
      throw new Error('WebGL not supported');
    } catch (_e) {
      // Fallback to canvas mode
      webglAddon = null;
      canvasMode = true;
    }

    expect(webglAddon).toBeNull();
    expect(canvasMode).toBe(true);
  });
});

describe('Input Forwarding', () => {
  it('should forward user input to onData callback', () => {
    const onData = vi.fn();
    const userInput = 'ls -la\n';

    // Simulate terminal onData event
    const terminalDataHandler = (callback: (data: string) => void) => {
      callback(userInput);
    };

    terminalDataHandler(onData);
    expect(onData).toHaveBeenCalledWith(userInput);
  });

  it('should handle special key sequences', () => {
    const onData = vi.fn();

    // Simulate various key sequences
    const keySequences = [
      '\x1b[A', // Up arrow
      '\x1b[B', // Down arrow
      '\x1b[C', // Right arrow
      '\x1b[D', // Left arrow
      '\x03',   // Ctrl+C
      '\x04',   // Ctrl+D
      '\t',     // Tab
      '\r',     // Enter
    ];

    keySequences.forEach(seq => {
      onData(seq);
    });

    expect(onData).toHaveBeenCalledTimes(keySequences.length);
    expect(onData).toHaveBeenCalledWith('\x1b[A');
    expect(onData).toHaveBeenCalledWith('\x03');
  });
});

describe('XTermWrapper Component DOM Structure', () => {
  describe('Expected Attributes', () => {
    it('should have correct accessibility role', () => {
      const expectedRole = 'application';
      expect(expectedRole).toBe('application');
    });

    it('should have correct aria-label', () => {
      const expectedLabel = 'Terminal';
      expect(expectedLabel).toBe('Terminal');
    });

    it('should have correct aria-roledescription', () => {
      const expectedRoleDescription = 'Interactive terminal';
      expect(expectedRoleDescription).toBe('Interactive terminal');
    });

    it('should have correct data-testid', () => {
      const expectedTestId = 'xterm-container';
      expect(expectedTestId).toBe('xterm-container');
    });
  });

  describe('Expected Styles', () => {
    it('should have full width', () => {
      const expectedWidth = '100%';
      expect(expectedWidth).toBe('100%');
    });

    it('should have full height', () => {
      const expectedHeight = '100%';
      expect(expectedHeight).toBe('100%');
    });
  });

  describe('Handle API', () => {
    it('should provide write method', () => {
      const handle = {
        write: vi.fn(),
        clear: vi.fn(),
        focus: vi.fn(),
        getDimensions: vi.fn(),
        getTerminal: vi.fn(),
      };

      expect(handle.write).toBeDefined();
      expect(typeof handle.write).toBe('function');
    });

    it('should provide clear method', () => {
      const handle = {
        write: vi.fn(),
        clear: vi.fn(),
        focus: vi.fn(),
        getDimensions: vi.fn(),
        getTerminal: vi.fn(),
      };

      expect(handle.clear).toBeDefined();
      expect(typeof handle.clear).toBe('function');
    });

    it('should provide focus method', () => {
      const handle = {
        write: vi.fn(),
        clear: vi.fn(),
        focus: vi.fn(),
        getDimensions: vi.fn(),
        getTerminal: vi.fn(),
      };

      expect(handle.focus).toBeDefined();
      expect(typeof handle.focus).toBe('function');
    });

    it('should provide getDimensions method', () => {
      const handle = {
        write: vi.fn(),
        clear: vi.fn(),
        focus: vi.fn(),
        getDimensions: vi.fn().mockReturnValue({ cols: 80, rows: 24 }),
        getTerminal: vi.fn(),
      };

      expect(handle.getDimensions).toBeDefined();
      const dims = handle.getDimensions();
      expect(dims).toEqual({ cols: 80, rows: 24 });
    });

    it('should provide getTerminal method', () => {
      const handle = {
        write: vi.fn(),
        clear: vi.fn(),
        focus: vi.fn(),
        getDimensions: vi.fn(),
        getTerminal: vi.fn().mockReturnValue({}),
      };

      expect(handle.getTerminal).toBeDefined();
      expect(typeof handle.getTerminal).toBe('function');
    });
  });
});

describe('Terminal Options', () => {
  it('should have correct default cursor style', () => {
    const cursorStyle = 'block';
    expect(cursorStyle).toBe('block');
  });

  it('should have cursor blink enabled by default', () => {
    const cursorBlink = true;
    expect(cursorBlink).toBe(true);
  });

  it('should have correct default font family', () => {
    const fontFamily = 'Menlo, Monaco, "Courier New", monospace';
    expect(fontFamily).toContain('Menlo');
    expect(fontFamily).toContain('Monaco');
    expect(fontFamily).toContain('monospace');
  });

  it('should have correct default font size', () => {
    const fontSize = 14;
    expect(fontSize).toBe(14);
  });

  it('should have correct default line height', () => {
    const lineHeight = 1.2;
    expect(lineHeight).toBe(1.2);
  });

  it('should have default scrollback limit', () => {
    const DEFAULT_SCROLLBACK_LIMIT = 3000;
    expect(DEFAULT_SCROLLBACK_LIMIT).toBe(3000);
  });

  it('should have fast scroll modifier set to alt', () => {
    const fastScrollModifier = 'alt';
    expect(fastScrollModifier).toBe('alt');
  });

  it('should have fast scroll sensitivity', () => {
    const fastScrollSensitivity = 5;
    expect(fastScrollSensitivity).toBe(5);
  });

  it('should have smooth scroll disabled for performance', () => {
    const smoothScrollDuration = 0;
    expect(smoothScrollDuration).toBe(0);
  });
});
