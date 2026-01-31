/**
 * Scanner module exports
 * Provides camera-based QR code scanning functionality
 */

// Web camera scanner (low-level)
export {
  WebCameraScanner,
  getWebCameraScanner,
  resetWebCameraScanner,
  type CameraFacingMode,
  type ScannerState,
  type ScannerErrorType,
  type ScannerError,
  type QRCodeResult,
  type ScannerEventType,
  type ScannerEvent,
  type StateChangeEvent,
  type CodeDetectedEvent,
  type ScannerErrorEvent,
  type AnyScannerEvent,
  type ScannerEventSubscriber,
  type CodeDetectedCallback,
  type WebCameraScannerConfig,
} from './WebCameraScanner';

// Cross-platform barcode scanner (high-level abstraction)
export {
  BarcodeScanner,
  getBarcodeScanner,
  resetBarcodeScanner,
  // Platform detection
  isTauriMobile,
  getScannerPlatform,
  type ScannerPlatform,
  // Pairing data
  parsePairingData,
  createPairingQRContent,
  isValidBase58,
  decodeBase58,
  encodeBase58,
  type PairingData,
  type PairingParseResult,
  // Scanner types
  type ScanResult,
  type BarcodeScannerConfig,
} from './BarcodeScanner';
