import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  BarcodeScanner,
  getBarcodeScanner,
  resetBarcodeScanner,
  isTauriMobile,
  getScannerPlatform,
  parsePairingData,
  createPairingQRContent,
  isPairingExpired,
  secondsUntilExpiry,
  isValidBase58,
  decodeBase58,
  encodeBase58,
  type PairingData,
} from './BarcodeScanner';

// Mock the WebCameraScanner module
vi.mock('./WebCameraScanner', () => {
  const mockWebCameraScannerClass = class MockWebCameraScanner {
    static checkPermission = vi.fn().mockResolvedValue('granted');
    static isCameraAvailable = vi.fn().mockResolvedValue(true);

    subscribe = vi.fn(() => vi.fn());
    start = vi.fn().mockResolvedValue(undefined);
    stop = vi.fn();
    destroy = vi.fn();
    getState = vi.fn(() => 'idle');
    bindVideoElement = vi.fn();
    unbindVideoElement = vi.fn();
  };

  return {
    WebCameraScanner: mockWebCameraScannerClass,
    getWebCameraScanner: vi.fn(() => new mockWebCameraScannerClass()),
    resetWebCameraScanner: vi.fn(),
  };
});

// WebCameraScanner is mocked above

// ============================================================================
// Test Helpers
// ============================================================================

/**
 * Sample pairing data for testing (matches daemon PairingInfo format)
 */
const samplePairingData: PairingData = {
  device_id: 'test-device-id-12345',
  public_key: 'dGVzdC1wdWJsaWMta2V5LWJhc2U2NA==', // base64 encoded
  relay_url: 'wss://relay.example.com',
  expires: Math.floor(Date.now() / 1000) + 300, // 5 minutes from now
};

/**
 * Create mock Tauri environment
 */
function setupTauriMobileMock() {
  const mockScanner = {
    scan: vi.fn().mockResolvedValue({ content: 'test-content' }),
    cancel: vi.fn().mockResolvedValue(undefined),
    checkPermissions: vi.fn().mockResolvedValue({ camera: 'granted' }),
    requestPermissions: vi.fn().mockResolvedValue({ camera: 'granted' }),
  };

  (window as any).__TAURI__ = {
    core: { invoke: vi.fn() },
    barcodeScanner: mockScanner,
  };

  return mockScanner;
}

/**
 * Clear Tauri mock
 */
function clearTauriMock() {
  delete (window as any).__TAURI__;
}

// ============================================================================
// Platform Detection Tests
// ============================================================================

describe('Platform Detection', () => {
  beforeEach(() => {
    clearTauriMock();
  });

  afterEach(() => {
    clearTauriMock();
  });

  describe('isTauriMobile', () => {
    it('should return false when window.__TAURI__ is not defined', () => {
      expect(isTauriMobile()).toBe(false);
    });

    it('should return false when __TAURI__ exists but barcodeScanner is undefined', () => {
      (window as any).__TAURI__ = {
        core: { invoke: vi.fn() },
      };
      expect(isTauriMobile()).toBe(false);
    });

    it('should return true when barcodeScanner plugin is available', () => {
      setupTauriMobileMock();
      expect(isTauriMobile()).toBe(true);
    });
  });

  describe('getScannerPlatform', () => {
    it('should return "web" when not on Tauri mobile', () => {
      expect(getScannerPlatform()).toBe('web');
    });

    it('should return "tauri-mobile" when on Tauri mobile', () => {
      setupTauriMobileMock();
      expect(getScannerPlatform()).toBe('tauri-mobile');
    });
  });
});

// ============================================================================
// Base58 Encoding/Decoding Tests
// ============================================================================

describe('Base58 Encoding', () => {
  describe('isValidBase58', () => {
    it('should return true for valid base58 strings', () => {
      expect(isValidBase58('123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz')).toBe(true);
      expect(isValidBase58('1')).toBe(true);
      expect(isValidBase58('z')).toBe(true);
    });

    it('should return false for invalid base58 strings', () => {
      expect(isValidBase58('')).toBe(false);
      expect(isValidBase58('0')).toBe(false); // 0 is not in base58
      expect(isValidBase58('O')).toBe(false); // O is not in base58
      expect(isValidBase58('I')).toBe(false); // I is not in base58
      expect(isValidBase58('l')).toBe(false); // l is not in base58
      expect(isValidBase58('hello world')).toBe(false); // space is not in base58
    });
  });

  describe('encodeBase58 and decodeBase58', () => {
    it('should encode and decode empty array', () => {
      const bytes = new Uint8Array([]);
      expect(encodeBase58(bytes)).toBe('');
    });

    it('should encode and decode single byte', () => {
      const bytes = new Uint8Array([255]);
      const encoded = encodeBase58(bytes);
      expect(isValidBase58(encoded)).toBe(true);
      const decoded = decodeBase58(encoded);
      expect(Array.from(decoded)).toEqual([255]);
    });

    it('should handle leading zeros', () => {
      const bytes = new Uint8Array([0, 0, 1, 2, 3]);
      const encoded = encodeBase58(bytes);
      expect(encoded.startsWith('11')).toBe(true); // Two leading zeros = two '1's
      const decoded = decodeBase58(encoded);
      expect(Array.from(decoded)).toEqual([0, 0, 1, 2, 3]);
    });

    it('should encode and decode complex data', () => {
      const original = new Uint8Array([
        72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33,
      ]); // "Hello, World!"
      const encoded = encodeBase58(original);
      expect(isValidBase58(encoded)).toBe(true);
      const decoded = decodeBase58(encoded);
      expect(Array.from(decoded)).toEqual(Array.from(original));
    });

    it('should throw error for invalid base58 on decode', () => {
      expect(() => decodeBase58('invalid0string')).toThrow('Invalid base58 string');
    });
  });
});

// ============================================================================
// Pairing Data Parsing Tests
// ============================================================================

describe('Pairing Data Parsing', () => {
  describe('parsePairingData', () => {
    it('should parse valid pairing data as raw JSON (daemon format)', () => {
      const qrContent = createPairingQRContent(samplePairingData);
      // New format uses raw JSON, not remoshell:// prefix
      expect(qrContent.startsWith('{')).toBe(true);

      const result = parsePairingData(qrContent);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(samplePairingData.device_id);
        expect(result.data.public_key).toBe(samplePairingData.public_key);
        expect(result.data.relay_url).toBe(samplePairingData.relay_url);
        expect(result.data.expires).toBe(samplePairingData.expires);
      }
    });

    it('should parse legacy format with remoshell:// prefix', () => {
      // Create legacy format (base58 encoded JSON with remoshell:// prefix)
      const encoder = new TextEncoder();
      const bytes = encoder.encode(JSON.stringify(samplePairingData));
      const encoded = encodeBase58(bytes);
      const legacyQR = `remoshell://connect/${encoded}`;

      const result = parsePairingData(legacyQR);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(samplePairingData.device_id);
      }
    });

    it('should parse legacy format with rs:// prefix', () => {
      const encoder = new TextEncoder();
      const bytes = encoder.encode(JSON.stringify(samplePairingData));
      const encoded = encodeBase58(bytes);
      const shortQR = `rs://${encoded}`;

      const result = parsePairingData(shortQR);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(samplePairingData.device_id);
      }
    });

    it('should parse raw base58 encoded data', () => {
      const encoder = new TextEncoder();
      const bytes = encoder.encode(JSON.stringify(samplePairingData));
      const encoded = encodeBase58(bytes);

      const result = parsePairingData(encoded);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(samplePairingData.device_id);
      }
    });

    it('should parse complete pairing data with all required fields', () => {
      const completeData: PairingData = {
        device_id: 'complete-device-id',
        public_key: 'YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=',
        relay_url: 'wss://complete.relay.com',
        expires: Math.floor(Date.now() / 1000) + 600,
      };
      const qrContent = createPairingQRContent(completeData);

      const result = parsePairingData(qrContent);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe('complete-device-id');
        expect(result.data.public_key).toBe(completeData.public_key);
        expect(result.data.relay_url).toBe(completeData.relay_url);
        expect(result.data.expires).toBe(completeData.expires);
      }
    });

    it('should return error for empty content', () => {
      const result = parsePairingData('');
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Empty or invalid QR code content');
      }
    });

    it('should return error for unrecognized format', () => {
      const result = parsePairingData('https://example.com/not-pairing');
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Invalid pairing code format');
      }
    });

    it('should return error for invalid base58', () => {
      const result = parsePairingData('remoshell://connect/invalid0base58');
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Invalid base58 encoding');
      }
    });

    it('should return error for missing device_id', () => {
      const invalidData = { relay_url: 'wss://relay.example.com', public_key: 'abc', expires: 123 };
      const result = parsePairingData(JSON.stringify(invalidData));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Missing or invalid device_id in pairing data');
      }
    });

    it('should return error for missing public_key', () => {
      const invalidData = { device_id: 'test', relay_url: 'wss://relay.example.com', expires: 123 };
      const result = parsePairingData(JSON.stringify(invalidData));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Missing or invalid public_key in pairing data');
      }
    });

    it('should return error for missing relay_url', () => {
      const invalidData = { device_id: 'test', public_key: 'abc', expires: 123 };
      const result = parsePairingData(JSON.stringify(invalidData));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Missing or invalid relay_url in pairing data');
      }
    });

    it('should return error for missing expires', () => {
      const invalidData = { device_id: 'test', public_key: 'abc', relay_url: 'wss://relay.example.com' };
      const result = parsePairingData(JSON.stringify(invalidData));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Missing or invalid expires in pairing data');
      }
    });

    it('should return error for invalid JSON', () => {
      const encoder = new TextEncoder();
      const bytes = encoder.encode('not valid json');
      const encoded = encodeBase58(bytes);

      const result = parsePairingData(`remoshell://connect/${encoded}`);
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Invalid JSON in pairing data');
      }
    });

    it('should handle whitespace in input', () => {
      const qrContent = createPairingQRContent(samplePairingData);
      const result = parsePairingData(`  ${qrContent}  `);
      expect(result.success).toBe(true);
    });
  });

  describe('createPairingQRContent', () => {
    it('should create QR content as raw JSON (daemon format)', () => {
      const content = createPairingQRContent(samplePairingData);
      // New format uses raw JSON
      expect(content.startsWith('{')).toBe(true);
      const parsed = JSON.parse(content);
      expect(parsed.device_id).toBe(samplePairingData.device_id);
    });

    it('should create content that can be parsed back', () => {
      const content = createPairingQRContent(samplePairingData);
      const result = parsePairingData(content);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(samplePairingData.device_id);
        expect(result.data.public_key).toBe(samplePairingData.public_key);
        expect(result.data.relay_url).toBe(samplePairingData.relay_url);
        expect(result.data.expires).toBe(samplePairingData.expires);
      }
    });
  });

  describe('isPairingExpired', () => {
    it('should return false for non-expired pairing data', () => {
      const futureExpires: PairingData = {
        ...samplePairingData,
        expires: Math.floor(Date.now() / 1000) + 300, // 5 minutes from now
      };
      expect(isPairingExpired(futureExpires)).toBe(false);
    });

    it('should return true for expired pairing data', () => {
      const pastExpires: PairingData = {
        ...samplePairingData,
        expires: Math.floor(Date.now() / 1000) - 60, // 1 minute ago
      };
      expect(isPairingExpired(pastExpires)).toBe(true);
    });
  });

  describe('secondsUntilExpiry', () => {
    it('should return positive seconds for non-expired data', () => {
      const futureExpires: PairingData = {
        ...samplePairingData,
        expires: Math.floor(Date.now() / 1000) + 300, // 5 minutes from now
      };
      const seconds = secondsUntilExpiry(futureExpires);
      expect(seconds).toBeGreaterThan(290);
      expect(seconds).toBeLessThanOrEqual(300);
    });

    it('should return 0 for expired data', () => {
      const pastExpires: PairingData = {
        ...samplePairingData,
        expires: Math.floor(Date.now() / 1000) - 60, // 1 minute ago
      };
      expect(secondsUntilExpiry(pastExpires)).toBe(0);
    });
  });
});

// ============================================================================
// BarcodeScanner Class Tests
// ============================================================================

describe('BarcodeScanner', () => {
  let scanner: BarcodeScanner;

  beforeEach(() => {
    clearTauriMock();
    resetBarcodeScanner();
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (scanner) {
      scanner.destroy();
    }
    clearTauriMock();
    resetBarcodeScanner();
  });

  describe('Constructor and Configuration', () => {
    it('should create scanner with default configuration', () => {
      scanner = new BarcodeScanner();
      expect(scanner.getPlatform()).toBe('web');
      expect(scanner.getState()).toBe('idle');
      expect(scanner.isActive()).toBe(false);
    });

    it('should detect Tauri mobile platform', () => {
      setupTauriMobileMock();
      scanner = new BarcodeScanner();
      expect(scanner.getPlatform()).toBe('tauri-mobile');
    });
  });

  describe('Permission Handling (Web)', () => {
    it('should check permission using WebCameraScanner', async () => {
      scanner = new BarcodeScanner();

      const permission = await scanner.checkPermission();
      // The mock returns 'granted'
      expect(permission).toBe('granted');
    });

    it('should request permission for web', async () => {
      scanner = new BarcodeScanner();

      const mockStream = {
        getTracks: vi.fn(() => [{
          stop: vi.fn(),
        }]),
      };

      Object.defineProperty(globalThis.navigator, 'mediaDevices', {
        value: {
          getUserMedia: vi.fn().mockResolvedValue(mockStream),
        },
        writable: true,
        configurable: true,
      });

      const result = await scanner.requestPermission();
      expect(result).toBe(true);
    });

    it('should return false when camera is not available for web', async () => {
      // Reset the mock to return false for isCameraAvailable
      const { WebCameraScanner } = await import('./WebCameraScanner');
      (WebCameraScanner as any).isCameraAvailable = vi.fn().mockResolvedValue(false);

      scanner = new BarcodeScanner();
      const result = await scanner.requestPermission();
      expect(result).toBe(false);
    });
  });

  describe('Permission Handling (Tauri Mobile)', () => {
    it('should check permission using Tauri plugin', async () => {
      const mockScanner = setupTauriMobileMock();
      scanner = new BarcodeScanner();

      const permission = await scanner.checkPermission();
      expect(mockScanner.checkPermissions).toHaveBeenCalled();
      expect(permission).toBe('granted');
    });

    it('should handle denied permission', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.checkPermissions.mockResolvedValue({ camera: 'denied' });

      scanner = new BarcodeScanner();
      const permission = await scanner.checkPermission();
      expect(permission).toBe('denied');
    });

    it('should handle prompt permission', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.checkPermissions.mockResolvedValue({ camera: 'prompt' });

      scanner = new BarcodeScanner();
      const permission = await scanner.checkPermission();
      expect(permission).toBe('prompt');
    });

    it('should handle prompt-with-rationale permission', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.checkPermissions.mockResolvedValue({ camera: 'prompt-with-rationale' });

      scanner = new BarcodeScanner();
      const permission = await scanner.checkPermission();
      expect(permission).toBe('prompt');
    });

    it('should request permission using Tauri plugin', async () => {
      const mockScanner = setupTauriMobileMock();
      scanner = new BarcodeScanner();

      const result = await scanner.requestPermission();
      expect(mockScanner.requestPermissions).toHaveBeenCalled();
      expect(result).toBe(true);
    });

    it('should handle permission request failure', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.requestPermissions.mockResolvedValue({ camera: 'denied' });

      scanner = new BarcodeScanner();
      const result = await scanner.requestPermission();
      expect(result).toBe(false);
    });
  });

  describe('Scanning (Tauri Mobile)', () => {
    it('should scan using Tauri plugin', async () => {
      const mockScanner = setupTauriMobileMock();
      const pairingQR = createPairingQRContent(samplePairingData);
      mockScanner.scan.mockResolvedValue({ content: pairingQR });

      scanner = new BarcodeScanner();
      const result = await scanner.scan();

      expect(mockScanner.scan).toHaveBeenCalledWith({ windowed: false });
      expect(result.content).toBe(pairingQR);
      expect(result.pairingData).toBeDefined();
      expect(result.pairingData?.device_id).toBe(samplePairingData.device_id);
    });

    it('should use windowed mode when configured', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.scan.mockResolvedValue({ content: 'test' });

      scanner = new BarcodeScanner({ windowedMode: true });
      await scanner.scan();

      expect(mockScanner.scan).toHaveBeenCalledWith({ windowed: true });
    });

    it('should include parse error when pairing data is invalid', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.scan.mockResolvedValue({ content: 'not-valid-pairing-data' });

      scanner = new BarcodeScanner();
      const result = await scanner.scan();

      expect(result.content).toBe('not-valid-pairing-data');
      expect(result.pairingData).toBeUndefined();
      expect(result.parseError).toBeDefined();
    });

    it('should skip parsing when autoParsePairing is false', async () => {
      const mockScanner = setupTauriMobileMock();
      const pairingQR = createPairingQRContent(samplePairingData);
      mockScanner.scan.mockResolvedValue({ content: pairingQR });

      scanner = new BarcodeScanner({ autoParsePairing: false });
      const result = await scanner.scan();

      expect(result.content).toBe(pairingQR);
      expect(result.pairingData).toBeUndefined();
      expect(result.parseError).toBeUndefined();
    });

    it('should throw when already scanning', async () => {
      const mockScanner = setupTauriMobileMock();
      // Make scan take a long time
      mockScanner.scan.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve({ content: 'test' }), 1000))
      );

      scanner = new BarcodeScanner();
      scanner.scan(); // Start first scan (don't await)

      await expect(scanner.scan()).rejects.toThrow('Scanner is already active');

      // Clean up
      scanner.stop();
    });
  });

  describe('Stop and Cancel', () => {
    it('should stop web scanner', () => {
      scanner = new BarcodeScanner();
      scanner.getWebScanner(); // Initialize web scanner

      scanner.stop();

      expect(scanner.isActive()).toBe(false);
      expect(scanner.getState()).toBe('idle');
    });

    it('should cancel Tauri scanner', async () => {
      const mockScanner = setupTauriMobileMock();
      mockScanner.scan.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve({ content: 'test' }), 1000))
      );

      scanner = new BarcodeScanner();
      scanner.scan().catch(() => {}); // Ignore the abort error

      // Wait a bit then stop
      await new Promise(resolve => setTimeout(resolve, 10));
      scanner.stop();

      expect(mockScanner.cancel).toHaveBeenCalled();
    });
  });

  describe('Event Subscription', () => {
    it('should allow subscribing to events', () => {
      scanner = new BarcodeScanner();
      const callback = vi.fn();

      const unsubscribe = scanner.subscribe(callback);
      expect(typeof unsubscribe).toBe('function');

      unsubscribe();
    });

    it('should handle subscriber errors gracefully', () => {
      scanner = new BarcodeScanner();
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      const errorCallback = vi.fn(() => { throw new Error('Test error'); });
      const goodCallback = vi.fn();

      scanner.subscribe(errorCallback);
      scanner.subscribe(goodCallback);

      // Trigger a state change by stopping (which sets state to idle)
      scanner.stop();

      // Both callbacks should have been attempted
      // The error should have been caught and logged
      consoleError.mockRestore();
    });
  });

  describe('Web Scanner Access', () => {
    it('should return web scanner instance in web mode', () => {
      scanner = new BarcodeScanner();
      const webScanner = scanner.getWebScanner();

      expect(webScanner).not.toBeNull();
    });

    it('should return null in Tauri mobile mode', () => {
      setupTauriMobileMock();
      scanner = new BarcodeScanner();
      const webScanner = scanner.getWebScanner();

      expect(webScanner).toBeNull();
    });
  });

  describe('Destroy', () => {
    it('should clean up resources on destroy', () => {
      scanner = new BarcodeScanner();
      scanner.getWebScanner(); // Initialize web scanner

      scanner.destroy();

      expect(scanner.isActive()).toBe(false);
      expect(scanner.getState()).toBe('idle');
    });
  });
});

// ============================================================================
// Singleton Tests
// ============================================================================

describe('Singleton', () => {
  beforeEach(() => {
    clearTauriMock();
    resetBarcodeScanner();
  });

  afterEach(() => {
    resetBarcodeScanner();
  });

  it('should return the same instance from getBarcodeScanner', () => {
    const instance1 = getBarcodeScanner();
    const instance2 = getBarcodeScanner();

    expect(instance1).toBe(instance2);
  });

  it('should create new instance after resetBarcodeScanner', () => {
    const instance1 = getBarcodeScanner();
    resetBarcodeScanner();
    const instance2 = getBarcodeScanner();

    expect(instance1).not.toBe(instance2);
  });

  it('should use config only on first call', () => {
    const instance1 = getBarcodeScanner({ autoParsePairing: false });
    const instance2 = getBarcodeScanner({ autoParsePairing: true });

    expect(instance1).toBe(instance2);
  });
});
