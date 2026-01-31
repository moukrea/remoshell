/**
 * BarcodeScanner - Cross-platform barcode scanning abstraction
 *
 * This module provides a unified interface for QR code scanning that works
 * across platforms:
 * - On Tauri mobile (Android/iOS): Uses the Tauri barcode scanner plugin
 * - On web browsers: Falls back to WebCameraScanner using jsQR
 *
 * It also includes utilities for parsing pairing data from scanned QR codes.
 */

import {
  WebCameraScanner,
  type QRCodeResult,
  type ScannerState,
  type ScannerError,
  type ScannerErrorType,
  type AnyScannerEvent,
  type ScannerEventSubscriber,
} from './WebCameraScanner';

// ============================================================================
// Platform Detection
// ============================================================================

/**
 * Platform types for scanner
 */
export type ScannerPlatform = 'tauri-mobile' | 'web';

/**
 * Tauri barcode scanner plugin types
 */
interface TauriBarcodeScanner {
  scan: (options?: { windowed?: boolean }) => Promise<{ content: string }>;
  cancel: () => Promise<void>;
  checkPermissions: () => Promise<{ camera: PermissionStatus }>;
  requestPermissions: () => Promise<{ camera: PermissionStatus }>;
}

type PermissionStatus = 'granted' | 'denied' | 'prompt' | 'prompt-with-rationale';

/**
 * Tauri API with barcode scanner plugin
 */
interface TauriWithBarcodeScanner {
  core: {
    invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  };
  barcodeScanner?: TauriBarcodeScanner;
}

/**
 * Check if running in a Tauri mobile environment with barcode scanner support
 */
export function isTauriMobile(): boolean {
  if (typeof window === 'undefined') return false;

  const tauri = (window as { __TAURI__?: TauriWithBarcodeScanner }).__TAURI__;
  if (!tauri) return false;

  // Check if we're on mobile by looking for the barcode scanner plugin
  // The plugin is only loaded on Android/iOS
  return tauri.barcodeScanner !== undefined;
}

/**
 * Get the current platform for scanning
 */
export function getScannerPlatform(): ScannerPlatform {
  return isTauriMobile() ? 'tauri-mobile' : 'web';
}

/**
 * Get the Tauri barcode scanner plugin
 * @throws Error if not running on Tauri mobile
 */
function getTauriBarcodeScanner(): TauriBarcodeScanner {
  if (!isTauriMobile()) {
    throw new Error('Tauri barcode scanner is only available on mobile');
  }
  const tauri = (window as { __TAURI__?: TauriWithBarcodeScanner }).__TAURI__!;
  return tauri.barcodeScanner!;
}

// ============================================================================
// Pairing Data Types
// ============================================================================

/**
 * Parsed pairing data from a QR code
 *
 * The pairing QR code contains connection information for establishing
 * a secure connection to a remote device.
 */
export interface PairingData {
  /** Node ID (public key) of the remote device */
  nodeId: string;
  /** Optional relay URL for NAT traversal */
  relayUrl?: string;
  /** Optional direct addresses for peer-to-peer connection */
  directAddresses?: string[];
  /** Optional device name for display */
  deviceName?: string;
  /** Protocol version for compatibility checking */
  protocolVersion?: number;
}

/**
 * Result of parsing a QR code
 */
export type PairingParseResult =
  | { success: true; data: PairingData }
  | { success: false; error: string };

// ============================================================================
// Pairing Data Parsing
// ============================================================================

/**
 * QR code format prefixes
 * The QR code can be in different formats:
 * - remoshell://connect/<base58-encoded-data>
 * - rs://<base58-encoded-data>
 * - Raw base58 encoded data
 */
const REMOSHELL_URL_PREFIX = 'remoshell://connect/';
const SHORT_URL_PREFIX = 'rs://';

/**
 * Base58 character set (Bitcoin style)
 */
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

/**
 * Validate that a string is valid base58
 */
export function isValidBase58(str: string): boolean {
  if (!str || str.length === 0) return false;
  return str.split('').every(char => BASE58_ALPHABET.includes(char));
}

/**
 * Decode base58 string to bytes
 */
export function decodeBase58(str: string): Uint8Array {
  if (!isValidBase58(str)) {
    throw new Error('Invalid base58 string');
  }

  // Convert to big integer
  let num = BigInt(0);
  for (const char of str) {
    num = num * BigInt(58) + BigInt(BASE58_ALPHABET.indexOf(char));
  }

  // Convert to bytes
  const bytes: number[] = [];
  while (num > 0) {
    bytes.unshift(Number(num % BigInt(256)));
    num = num / BigInt(256);
  }

  // Add leading zeros for leading '1's in base58
  for (const char of str) {
    if (char === '1') {
      bytes.unshift(0);
    } else {
      break;
    }
  }

  return new Uint8Array(bytes);
}

/**
 * Encode bytes to base58 string
 */
export function encodeBase58(bytes: Uint8Array): string {
  if (bytes.length === 0) return '';

  // Count leading zeros
  let leadingZeros = 0;
  for (const byte of bytes) {
    if (byte === 0) {
      leadingZeros++;
    } else {
      break;
    }
  }

  // Convert to big integer
  let num = BigInt(0);
  for (const byte of bytes) {
    num = num * BigInt(256) + BigInt(byte);
  }

  // Convert to base58
  let result = '';
  while (num > 0) {
    result = BASE58_ALPHABET[Number(num % BigInt(58))] + result;
    num = num / BigInt(58);
  }

  // Add leading '1's for leading zeros
  return '1'.repeat(leadingZeros) + result;
}

/**
 * Parse pairing data from a QR code string
 *
 * Supports the following formats:
 * - remoshell://connect/<base58-data>
 * - rs://<base58-data>
 * - Raw base58 encoded data
 *
 * The base58 data decodes to a JSON object with pairing information.
 */
export function parsePairingData(qrContent: string): PairingParseResult {
  if (!qrContent || typeof qrContent !== 'string') {
    return { success: false, error: 'Empty or invalid QR code content' };
  }

  const trimmed = qrContent.trim();
  let encodedData: string;

  // Extract the base58 encoded portion
  if (trimmed.startsWith(REMOSHELL_URL_PREFIX)) {
    encodedData = trimmed.slice(REMOSHELL_URL_PREFIX.length);
  } else if (trimmed.startsWith(SHORT_URL_PREFIX)) {
    encodedData = trimmed.slice(SHORT_URL_PREFIX.length);
  } else if (isValidBase58(trimmed)) {
    encodedData = trimmed;
  } else {
    return { success: false, error: 'Unrecognized QR code format' };
  }

  // Validate and decode the base58 data
  if (!isValidBase58(encodedData)) {
    return { success: false, error: 'Invalid base58 encoding' };
  }

  try {
    const bytes = decodeBase58(encodedData);
    const decoder = new TextDecoder();
    const jsonString = decoder.decode(bytes);
    const data = JSON.parse(jsonString);

    // Validate required fields
    if (!data.nodeId || typeof data.nodeId !== 'string') {
      return { success: false, error: 'Missing or invalid nodeId in pairing data' };
    }

    // Build pairing data with validated fields
    const pairingData: PairingData = {
      nodeId: data.nodeId,
    };

    // Optional fields
    if (data.relayUrl && typeof data.relayUrl === 'string') {
      pairingData.relayUrl = data.relayUrl;
    }

    if (Array.isArray(data.directAddresses)) {
      pairingData.directAddresses = data.directAddresses.filter(
        (addr: unknown) => typeof addr === 'string'
      );
    }

    if (data.deviceName && typeof data.deviceName === 'string') {
      pairingData.deviceName = data.deviceName;
    }

    if (typeof data.protocolVersion === 'number') {
      pairingData.protocolVersion = data.protocolVersion;
    }

    return { success: true, data: pairingData };
  } catch (error) {
    if (error instanceof SyntaxError) {
      return { success: false, error: 'Invalid JSON in pairing data' };
    }
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Failed to decode pairing data',
    };
  }
}

/**
 * Create a pairing QR code content string from pairing data
 */
export function createPairingQRContent(data: PairingData): string {
  const encoder = new TextEncoder();
  const jsonString = JSON.stringify(data);
  const bytes = encoder.encode(jsonString);
  const encoded = encodeBase58(bytes);
  return `${REMOSHELL_URL_PREFIX}${encoded}`;
}

// ============================================================================
// BarcodeScanner Class
// ============================================================================

/**
 * Scanner result from any platform
 */
export interface ScanResult {
  /** The raw scanned content */
  content: string;
  /** Parsed pairing data if applicable */
  pairingData?: PairingData;
  /** Parse error if pairing data parsing failed */
  parseError?: string;
}

/**
 * Scanner configuration
 */
export interface BarcodeScannerConfig {
  /** Whether to automatically parse scanned content as pairing data */
  autoParsePairing?: boolean;
  /** Use windowed mode on Tauri (shows camera preview) */
  windowedMode?: boolean;
}

/**
 * BarcodeScanner provides a unified interface for QR code scanning
 * across Tauri mobile and web platforms.
 */
export class BarcodeScanner {
  private platform: ScannerPlatform;
  private webScanner: WebCameraScanner | null = null;
  private config: Required<BarcodeScannerConfig>;
  private state: ScannerState = 'idle';
  private subscribers: Set<ScannerEventSubscriber> = new Set();
  private isScanning = false;
  private abortController: AbortController | null = null;

  constructor(config: BarcodeScannerConfig = {}) {
    this.platform = getScannerPlatform();
    this.config = {
      autoParsePairing: config.autoParsePairing ?? true,
      windowedMode: config.windowedMode ?? false,
    };
  }

  /**
   * Get the current platform
   */
  getPlatform(): ScannerPlatform {
    return this.platform;
  }

  /**
   * Get the current scanner state
   */
  getState(): ScannerState {
    return this.state;
  }

  /**
   * Check if currently scanning
   */
  isActive(): boolean {
    return this.isScanning;
  }

  /**
   * Subscribe to scanner events
   * Note: Events are only emitted in web mode
   */
  subscribe(callback: ScannerEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Update internal state and notify subscribers
   */
  private setState(newState: ScannerState): void {
    if (this.state === newState) return;
    const previousState = this.state;
    this.state = newState;
    this.emit({
      type: 'state_change',
      state: newState,
      previousState,
    });
  }

  /**
   * Emit an event to subscribers
   */
  private emit(event: AnyScannerEvent): void {
    this.subscribers.forEach((subscriber) => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in scanner event subscriber:', error);
      }
    });
  }

  /**
   * Check camera permission status
   */
  async checkPermission(): Promise<'granted' | 'denied' | 'prompt' | null> {
    if (this.platform === 'tauri-mobile') {
      try {
        const scanner = getTauriBarcodeScanner();
        const result = await scanner.checkPermissions();
        switch (result.camera) {
          case 'granted':
            return 'granted';
          case 'denied':
            return 'denied';
          case 'prompt':
          case 'prompt-with-rationale':
            return 'prompt';
          default:
            return null;
        }
      } catch {
        return null;
      }
    } else {
      return WebCameraScanner.checkPermission();
    }
  }

  /**
   * Request camera permission
   */
  async requestPermission(): Promise<boolean> {
    if (this.platform === 'tauri-mobile') {
      try {
        const scanner = getTauriBarcodeScanner();
        const result = await scanner.requestPermissions();
        return result.camera === 'granted';
      } catch {
        return false;
      }
    } else {
      // For web, we need to try accessing the camera to trigger the permission prompt
      const available = await WebCameraScanner.isCameraAvailable();
      if (!available) return false;

      try {
        const stream = await navigator.mediaDevices.getUserMedia({ video: true });
        stream.getTracks().forEach(track => track.stop());
        return true;
      } catch {
        return false;
      }
    }
  }

  /**
   * Scan a QR code
   *
   * On Tauri mobile: Opens the native camera scanner
   * On web: Starts continuous scanning (use stop() to end)
   *
   * @returns Promise that resolves with the scan result
   */
  async scan(): Promise<ScanResult> {
    if (this.isScanning) {
      throw new Error('Scanner is already active');
    }

    this.isScanning = true;
    this.abortController = new AbortController();

    try {
      if (this.platform === 'tauri-mobile') {
        return await this.scanTauri();
      } else {
        return await this.scanWeb();
      }
    } finally {
      this.isScanning = false;
      this.abortController = null;
    }
  }

  /**
   * Scan using Tauri barcode scanner plugin
   */
  private async scanTauri(): Promise<ScanResult> {
    this.setState('scanning');

    try {
      const scanner = getTauriBarcodeScanner();
      const result = await scanner.scan({
        windowed: this.config.windowedMode,
      });

      this.setState('idle');

      return this.processResult(result.content);
    } catch (error) {
      this.setState('error');
      throw error;
    }
  }

  /**
   * Scan using web camera scanner
   */
  private async scanWeb(): Promise<ScanResult> {
    return new Promise<ScanResult>((resolve, reject) => {
      if (!this.webScanner) {
        this.webScanner = new WebCameraScanner();
      }

      // Forward events to our subscribers
      const unsubscribe = this.webScanner.subscribe((event) => {
        this.emit(event);

        if (event.type === 'state_change') {
          this.state = event.state;
        }

        if (event.type === 'code_detected') {
          unsubscribe();
          this.webScanner?.stop();
          resolve(this.processResult(event.result.data));
        }

        if (event.type === 'error') {
          unsubscribe();
          this.webScanner?.stop();
          reject(new Error(event.error.message));
        }
      });

      // Handle abort
      this.abortController?.signal.addEventListener('abort', () => {
        unsubscribe();
        this.webScanner?.stop();
        reject(new Error('Scan cancelled'));
      });

      // Start scanning
      this.webScanner.start().catch((error) => {
        unsubscribe();
        reject(error);
      });
    });
  }

  /**
   * Process a raw scan result
   */
  private processResult(content: string): ScanResult {
    const result: ScanResult = { content };

    if (this.config.autoParsePairing) {
      const parseResult = parsePairingData(content);
      if (parseResult.success) {
        result.pairingData = parseResult.data;
      } else {
        result.parseError = parseResult.error;
      }
    }

    return result;
  }

  /**
   * Stop scanning (web mode only)
   * On Tauri mobile, use the native cancel button
   */
  stop(): void {
    if (this.abortController) {
      this.abortController.abort();
    }

    if (this.platform === 'tauri-mobile') {
      // Try to cancel the Tauri scanner
      try {
        const scanner = getTauriBarcodeScanner();
        scanner.cancel().catch(() => {
          // Ignore cancel errors
        });
      } catch {
        // Ignore if Tauri is not available
      }
    } else if (this.webScanner) {
      this.webScanner.stop();
    }

    this.isScanning = false;
    this.setState('idle');
  }

  /**
   * Get the web scanner instance for binding video elements
   * Only available in web mode
   */
  getWebScanner(): WebCameraScanner | null {
    if (this.platform === 'web') {
      if (!this.webScanner) {
        this.webScanner = new WebCameraScanner();
      }
      return this.webScanner;
    }
    return null;
  }

  /**
   * Destroy the scanner and clean up resources
   */
  destroy(): void {
    this.stop();
    if (this.webScanner) {
      this.webScanner.destroy();
      this.webScanner = null;
    }
    this.subscribers.clear();
  }
}

// ============================================================================
// Singleton Instance
// ============================================================================

let scannerInstance: BarcodeScanner | null = null;

/**
 * Get or create a singleton BarcodeScanner instance
 */
export function getBarcodeScanner(config?: BarcodeScannerConfig): BarcodeScanner {
  if (!scannerInstance) {
    scannerInstance = new BarcodeScanner(config);
  }
  return scannerInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetBarcodeScanner(): void {
  if (scannerInstance) {
    scannerInstance.destroy();
  }
  scannerInstance = null;
}

// ============================================================================
// Re-exports
// ============================================================================

export type {
  QRCodeResult,
  ScannerState,
  ScannerError,
  ScannerErrorType,
  AnyScannerEvent,
  ScannerEventSubscriber,
};
