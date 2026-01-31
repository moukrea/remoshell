import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import {
  WebCameraScanner,
  getWebCameraScanner,
  resetWebCameraScanner,
  type QRCodeResult,
  type ScannerState,
  type ScannerError,
} from './WebCameraScanner';

// Mock jsQR
vi.mock('jsqr', () => {
  return {
    default: vi.fn(() => null),
  };
});

import jsQR from 'jsqr';

/**
 * Mock QR code result for testing
 */
const mockQRCodeResult = {
  data: 'https://example.com/pairing/ABC123',
  binaryData: new Uint8ClampedArray([]),
  location: {
    topLeftCorner: { x: 10, y: 10 },
    topRightCorner: { x: 100, y: 10 },
    bottomLeftCorner: { x: 10, y: 100 },
    bottomRightCorner: { x: 100, y: 100 },
    topLeftFinderPattern: { x: 10, y: 10 },
    topRightFinderPattern: { x: 100, y: 10 },
    bottomLeftFinderPattern: { x: 10, y: 100 },
    alignmentPattern: null,
  },
};

/**
 * Create a mock MediaStream
 */
function createMockMediaStream(): MediaStream {
  const mockTrack = {
    stop: vi.fn(),
    kind: 'video',
    enabled: true,
    id: 'mock-track-id',
    label: 'Mock Camera',
    muted: false,
    readyState: 'live',
    onended: null,
    onmute: null,
    onunmute: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
    applyConstraints: vi.fn(),
    clone: vi.fn(),
    getCapabilities: vi.fn(() => ({})),
    getConstraints: vi.fn(() => ({})),
    getSettings: vi.fn(() => ({})),
  };

  return {
    active: true,
    id: 'mock-stream-id',
    onaddtrack: null,
    onremovetrack: null,
    getTracks: vi.fn(() => [mockTrack]),
    getVideoTracks: vi.fn(() => [mockTrack]),
    getAudioTracks: vi.fn(() => []),
    addTrack: vi.fn(),
    removeTrack: vi.fn(),
    clone: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  } as unknown as MediaStream;
}

interface MockVideoElement extends HTMLVideoElement {
  _loadedmetadataHandler: ((ev: Event) => void) | null;
  _triggerLoadedMetadata: () => void;
  _pendingStream: MediaStream | null;
}

/**
 * Create a mock HTMLVideoElement that properly handles async event triggering
 */
function createMockVideoElement(): MockVideoElement {
  let srcObject: MediaStream | null = null;
  let loadedmetadataHandler: ((ev: Event) => void) | null = null;
  let pendingStream: MediaStream | null = null;

  const mockVideo = {
    videoWidth: 640,
    videoHeight: 480,
    readyState: 4, // HAVE_ENOUGH_DATA
    HAVE_ENOUGH_DATA: 4, // HTMLVideoElement constant
    muted: false,
    get srcObject() {
      return srcObject;
    },
    set srcObject(value: MediaStream | null) {
      srcObject = value;
      // Store pending stream to trigger when handler is added
      if (value) {
        pendingStream = value;
      }
    },
    _loadedmetadataHandler: null as ((ev: Event) => void) | null,
    _pendingStream: null as MediaStream | null,
    _triggerLoadedMetadata: () => {
      if (loadedmetadataHandler) {
        loadedmetadataHandler(new Event('loadedmetadata'));
      }
    },
    setAttribute: vi.fn(),
    play: vi.fn().mockResolvedValue(undefined),
    pause: vi.fn(),
    addEventListener: vi.fn((event: string, handler: (ev: Event) => void) => {
      if (event === 'loadedmetadata') {
        loadedmetadataHandler = handler;
        mockVideo._loadedmetadataHandler = handler;
        // If we have a pending stream, trigger the event immediately
        if (pendingStream) {
          pendingStream = null;
          // Use queueMicrotask to ensure it happens after the listener is registered
          queueMicrotask(() => {
            if (loadedmetadataHandler) {
              loadedmetadataHandler(new Event('loadedmetadata'));
            }
          });
        }
      }
    }),
    removeEventListener: vi.fn((event: string, handler: (ev: Event) => void) => {
      if (event === 'loadedmetadata' && loadedmetadataHandler === handler) {
        loadedmetadataHandler = null;
        mockVideo._loadedmetadataHandler = null;
      }
    }),
  };
  return mockVideo as unknown as MockVideoElement;
}

// RAF state for tests
let rafId = 0;
let rafCallbacks: Map<number, FrameRequestCallback>;

function triggerAnimationFrame(timestamp: number = performance.now()) {
  const callbacks = Array.from(rafCallbacks.entries());
  for (const [id, callback] of callbacks) {
    rafCallbacks.delete(id);
    callback(timestamp);
  }
}

/**
 * Setup global mocks for browser APIs
 */
function setupBrowserMocks() {
  // Mock navigator.mediaDevices
  const mockGetUserMedia = vi.fn().mockResolvedValue(createMockMediaStream());
  const mockEnumerateDevices = vi.fn().mockResolvedValue([
    { kind: 'videoinput', deviceId: 'camera1', label: 'Front Camera' },
    { kind: 'videoinput', deviceId: 'camera2', label: 'Back Camera' },
  ]);

  Object.defineProperty(globalThis.navigator, 'mediaDevices', {
    value: {
      getUserMedia: mockGetUserMedia,
      enumerateDevices: mockEnumerateDevices,
    },
    writable: true,
    configurable: true,
  });

  // Mock navigator.permissions
  Object.defineProperty(globalThis.navigator, 'permissions', {
    value: {
      query: vi.fn().mockResolvedValue({ state: 'granted' }),
    },
    writable: true,
    configurable: true,
  });

  // Mock document.createElement for canvas
  const mockCanvas = {
    width: 0,
    height: 0,
    getContext: vi.fn(() => ({
      drawImage: vi.fn(),
      getImageData: vi.fn(() => ({
        data: new Uint8ClampedArray(640 * 480 * 4),
        width: 640,
        height: 480,
      })),
    })),
  };

  const originalCreateElement = document.createElement.bind(document);
  vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
    if (tagName === 'canvas') {
      return mockCanvas as unknown as HTMLCanvasElement;
    }
    return originalCreateElement(tagName);
  });

  // Mock requestAnimationFrame
  rafId = 0;
  rafCallbacks = new Map();
  vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation((callback) => {
    const id = ++rafId;
    rafCallbacks.set(id, callback);
    return id;
  });

  vi.spyOn(globalThis, 'cancelAnimationFrame').mockImplementation((id) => {
    rafCallbacks.delete(id);
  });

  return {
    mockGetUserMedia,
    mockEnumerateDevices,
    mockCanvas,
  };
}

describe('WebCameraScanner', () => {
  let scanner: WebCameraScanner;
  let mocks: ReturnType<typeof setupBrowserMocks>;

  beforeEach(() => {
    resetWebCameraScanner();
    mocks = setupBrowserMocks();
    scanner = new WebCameraScanner();
    (jsQR as Mock).mockReturnValue(null);
  });

  afterEach(() => {
    scanner.destroy();
    resetWebCameraScanner();
    vi.restoreAllMocks();
  });

  describe('Constructor and Configuration', () => {
    it('should use default configuration when none provided', () => {
      const s = new WebCameraScanner();
      expect(s.getFacingMode()).toBe('environment');
    });

    it('should use custom configuration when provided', () => {
      const s = new WebCameraScanner({
        facingMode: 'user',
        scanInterval: 200,
      });
      expect(s.getFacingMode()).toBe('user');
    });

    it('should start in idle state', () => {
      expect(scanner.getState()).toBe('idle');
      expect(scanner.isScanning()).toBe(false);
    });
  });

  describe('Video Element Binding', () => {
    it('should bind video element', () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      expect(video.setAttribute).toHaveBeenCalledWith('playsinline', 'true');
      expect(video.setAttribute).toHaveBeenCalledWith('autoplay', 'true');
      expect(video.muted).toBe(true);
    });

    it('should unbind video element', () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);
      scanner.unbindVideoElement();

      expect(video.srcObject).toBeNull();
    });

    it('should attach stream when binding after start', async () => {
      const video = createMockVideoElement();

      await scanner.start();
      scanner.bindVideoElement(video);

      // Stream should be attached
      expect(video.srcObject).not.toBeNull();
    });
  });

  describe('Camera Permission and Start', () => {
    it('should request camera permission on start', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();

      expect(mocks.mockGetUserMedia).toHaveBeenCalledWith({
        audio: false,
        video: {
          facingMode: { ideal: 'environment' },
          width: { ideal: 1280 },
          height: { ideal: 720 },
        },
      });
    });

    it('should transition to scanning state after start', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      const states: ScannerState[] = [];
      scanner.subscribe((event) => {
        if (event.type === 'state_change') {
          states.push(event.state);
        }
      });

      await scanner.start();

      expect(states).toContain('requesting');
      expect(states).toContain('scanning');
      expect(scanner.isScanning()).toBe(true);
    });

    it('should emit error event on permission denied', async () => {
      mocks.mockGetUserMedia.mockRejectedValue(
        new DOMException('Permission denied', 'NotAllowedError')
      );

      const errors: ScannerError[] = [];
      scanner.subscribe((event) => {
        if (event.type === 'error') {
          errors.push(event.error);
        }
      });

      await expect(scanner.start()).rejects.toThrow();

      expect(errors).toHaveLength(1);
      expect(errors[0].type).toBe('permission_denied');
      expect(scanner.getState()).toBe('error');
    });

    it('should handle camera not found error', async () => {
      mocks.mockGetUserMedia.mockRejectedValue(
        new DOMException('No camera found', 'NotFoundError')
      );

      const errors: ScannerError[] = [];
      scanner.subscribe((event) => {
        if (event.type === 'error') {
          errors.push(event.error);
        }
      });

      await expect(scanner.start()).rejects.toThrow();

      expect(errors[0].type).toBe('not_found');
    });

    it('should not start if already scanning', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      const callCount = mocks.mockGetUserMedia.mock.calls.length;

      await scanner.start();

      expect(mocks.mockGetUserMedia.mock.calls.length).toBe(callCount);
    });
  });

  describe('QR Code Detection', () => {
    it('should detect QR codes from video frames', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      const detectedCodes: QRCodeResult[] = [];
      scanner.subscribe((event) => {
        if (event.type === 'code_detected') {
          detectedCodes.push(event.result);
        }
      });

      await scanner.start();

      // Setup mock to return a QR code
      (jsQR as Mock).mockReturnValue(mockQRCodeResult);

      // Trigger animation frame with enough time passed
      triggerAnimationFrame(100);

      expect(detectedCodes).toHaveLength(1);
      expect(detectedCodes[0].data).toBe('https://example.com/pairing/ABC123');
    });

    it('should call onCodeDetected callback', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      const callback = vi.fn();
      scanner.setOnCodeDetected(callback);

      await scanner.start();

      (jsQR as Mock).mockReturnValue(mockQRCodeResult);
      triggerAnimationFrame(100);

      expect(callback).toHaveBeenCalledWith(
        expect.objectContaining({
          data: 'https://example.com/pairing/ABC123',
        })
      );
    });

    it('should debounce repeated detections of the same code', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      const detectedCodes: QRCodeResult[] = [];
      scanner.subscribe((event) => {
        if (event.type === 'code_detected') {
          detectedCodes.push(event.result);
        }
      });

      await scanner.start();

      (jsQR as Mock).mockReturnValue(mockQRCodeResult);

      // First detection
      triggerAnimationFrame(100);

      // Second detection (same code, should be debounced)
      triggerAnimationFrame(200);

      expect(detectedCodes).toHaveLength(1);
    });

    it('should include QR code location in result', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      let detectedResult: QRCodeResult | null = null;
      scanner.subscribe((event) => {
        if (event.type === 'code_detected') {
          detectedResult = event.result;
        }
      });

      await scanner.start();

      (jsQR as Mock).mockReturnValue(mockQRCodeResult);
      triggerAnimationFrame(100);

      expect(detectedResult).not.toBeNull();
      expect(detectedResult!.location).toEqual({
        topLeftCorner: { x: 10, y: 10 },
        topRightCorner: { x: 100, y: 10 },
        bottomLeftCorner: { x: 10, y: 100 },
        bottomRightCorner: { x: 100, y: 100 },
      });
    });
  });

  describe('Pause and Resume', () => {
    it('should pause scanning', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      expect(scanner.getState()).toBe('scanning');

      scanner.pause();

      expect(scanner.getState()).toBe('paused');
      expect(scanner.isScanning()).toBe(false);
    });

    it('should resume scanning after pause', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      scanner.pause();
      scanner.resume();

      expect(scanner.getState()).toBe('scanning');
      expect(scanner.isScanning()).toBe(true);
    });

    it('should not resume if not paused', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      scanner.stop();
      scanner.resume();

      expect(scanner.getState()).toBe('idle');
    });
  });

  describe('Stop', () => {
    it('should stop scanning and release camera', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      const stream = video.srcObject as MediaStream;

      scanner.stop();

      expect(scanner.getState()).toBe('idle');
      expect(stream.getTracks()[0].stop).toHaveBeenCalled();
      expect(video.srcObject).toBeNull();
    });
  });

  describe('Camera Toggle', () => {
    it('should toggle between front and back camera', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      expect(scanner.getFacingMode()).toBe('environment');

      await scanner.toggleCamera();
      expect(scanner.getFacingMode()).toBe('user');

      await scanner.toggleCamera();
      expect(scanner.getFacingMode()).toBe('environment');
    });

    it('should restart camera with new facing mode', async () => {
      // Start with a video element
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);
      await scanner.start();

      const initialCallCount = mocks.mockGetUserMedia.mock.calls.length;
      expect(scanner.getFacingMode()).toBe('environment');

      // Change facing mode - this should restart the camera
      await scanner.setFacingMode('user');

      expect(scanner.getFacingMode()).toBe('user');
      expect(mocks.mockGetUserMedia.mock.calls.length).toBe(initialCallCount + 1);
      const lastCall = mocks.mockGetUserMedia.mock.calls[mocks.mockGetUserMedia.mock.calls.length - 1];
      expect(lastCall[0].video.facingMode).toEqual({ ideal: 'user' });
    });

    it('should not restart if same facing mode', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      await scanner.start();
      const callCount = mocks.mockGetUserMedia.mock.calls.length;

      await scanner.setFacingMode('environment');

      expect(mocks.mockGetUserMedia.mock.calls.length).toBe(callCount);
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', async () => {
      const subscriber = vi.fn();
      scanner.subscribe(subscriber);

      const video = createMockVideoElement();
      scanner.bindVideoElement(video);
      await scanner.start();

      expect(subscriber).toHaveBeenCalled();
    });

    it('should allow unsubscribing from events', async () => {
      const subscriber = vi.fn();
      const unsubscribe = scanner.subscribe(subscriber);

      unsubscribe();

      const video = createMockVideoElement();
      scanner.bindVideoElement(video);
      await scanner.start();

      expect(subscriber).not.toHaveBeenCalled();
    });

    it('should handle subscriber errors gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      scanner.subscribe(errorSubscriber);
      scanner.subscribe(goodSubscriber);

      const video = createMockVideoElement();
      scanner.bindVideoElement(video);
      await scanner.start();

      expect(errorSubscriber).toHaveBeenCalled();
      expect(goodSubscriber).toHaveBeenCalled();
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Static Methods', () => {
    it('should check camera permission', async () => {
      const state = await WebCameraScanner.checkPermission();
      expect(state).toBe('granted');
    });

    it('should return null when permissions API not available', async () => {
      Object.defineProperty(globalThis.navigator, 'permissions', {
        value: undefined,
        writable: true,
        configurable: true,
      });

      const state = await WebCameraScanner.checkPermission();
      expect(state).toBeNull();
    });

    it('should check if camera is available', async () => {
      const available = await WebCameraScanner.isCameraAvailable();
      expect(available).toBe(true);
    });

    it('should return false when no camera devices', async () => {
      mocks.mockEnumerateDevices.mockResolvedValue([
        { kind: 'audioinput', deviceId: 'mic1', label: 'Microphone' },
      ]);

      const available = await WebCameraScanner.isCameraAvailable();
      expect(available).toBe(false);
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getWebCameraScanner', () => {
      const instance1 = getWebCameraScanner();
      const instance2 = getWebCameraScanner();

      expect(instance1).toBe(instance2);
    });

    it('should create new instance after resetWebCameraScanner', () => {
      const instance1 = getWebCameraScanner();
      resetWebCameraScanner();
      const instance2 = getWebCameraScanner();

      expect(instance1).not.toBe(instance2);
    });
  });

  describe('Destroy', () => {
    it('should clean up all resources on destroy', async () => {
      const video = createMockVideoElement();
      scanner.bindVideoElement(video);

      const subscriber = vi.fn();
      scanner.subscribe(subscriber);
      scanner.setOnCodeDetected(vi.fn());

      await scanner.start();
      scanner.destroy();

      expect(scanner.getState()).toBe('idle');
      // Subscribers should be cleared, so no more events
      subscriber.mockClear();
      // This should not emit to the old subscriber
    });
  });
});

describe('Error Type Mapping', () => {
  let scanner: WebCameraScanner;
  let mocks: ReturnType<typeof setupBrowserMocks>;

  beforeEach(() => {
    resetWebCameraScanner();
    mocks = setupBrowserMocks();
    scanner = new WebCameraScanner();
  });

  afterEach(() => {
    scanner.destroy();
    resetWebCameraScanner();
    vi.restoreAllMocks();
  });

  const errorCases: Array<[string, string]> = [
    ['NotAllowedError', 'permission_denied'],
    ['PermissionDeniedError', 'permission_denied'],
    ['NotFoundError', 'not_found'],
    ['DevicesNotFoundError', 'not_found'],
    ['NotReadableError', 'not_readable'],
    ['TrackStartError', 'not_readable'],
    ['OverconstrainedError', 'overconstrained'],
    ['ConstraintNotSatisfiedError', 'overconstrained'],
    ['SecurityError', 'security'],
    ['AbortError', 'abort'],
    ['SomeOtherError', 'unknown'],
  ];

  errorCases.forEach(([domExceptionName, expectedType]) => {
    it(`should map ${domExceptionName} to ${expectedType}`, async () => {
      mocks.mockGetUserMedia.mockRejectedValue(
        new DOMException('Error message', domExceptionName)
      );

      let errorType: string | null = null;
      scanner.subscribe((event) => {
        if (event.type === 'error') {
          errorType = event.error.type;
        }
      });

      await expect(scanner.start()).rejects.toThrow();

      expect(errorType).toBe(expectedType);
    });
  });
});

describe('Debounce behavior with timing', () => {
  let scanner: WebCameraScanner;

  beforeEach(() => {
    vi.useFakeTimers();
    resetWebCameraScanner();
    setupBrowserMocks();
    scanner = new WebCameraScanner();
    (jsQR as Mock).mockReturnValue(null);
  });

  afterEach(() => {
    scanner.destroy();
    resetWebCameraScanner();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('should detect same code again after debounce period', async () => {
    // We need a video element for analyzeFrame to work
    const video = createMockVideoElement();
    scanner.bindVideoElement(video);

    const startPromise = scanner.start();
    await vi.runAllTimersAsync();
    await startPromise;

    const detectedCodes: QRCodeResult[] = [];
    scanner.subscribe((event) => {
      if (event.type === 'code_detected') {
        detectedCodes.push(event.result);
      }
    });

    (jsQR as Mock).mockReturnValue(mockQRCodeResult);

    // First detection
    triggerAnimationFrame(100);

    expect(detectedCodes).toHaveLength(1);

    // Wait for debounce to clear (1000ms)
    vi.advanceTimersByTime(1100);

    // Second detection (same code, should be detected again)
    triggerAnimationFrame(1200);

    expect(detectedCodes).toHaveLength(2);
  });
});
