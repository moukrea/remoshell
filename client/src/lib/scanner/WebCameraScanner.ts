/**
 * WebCameraScanner - Camera-based QR code scanning for web browsers
 * Uses navigator.mediaDevices.getUserMedia for camera access
 * and jsQR for QR code detection
 */

import jsQR from 'jsqr';

/**
 * Camera facing mode for front/back camera selection
 */
export type CameraFacingMode = 'user' | 'environment';

/**
 * Scanner state
 */
export type ScannerState = 'idle' | 'requesting' | 'scanning' | 'paused' | 'error';

/**
 * Error types for scanner operations
 */
export type ScannerErrorType =
  | 'permission_denied'
  | 'not_found'
  | 'not_readable'
  | 'overconstrained'
  | 'security'
  | 'abort'
  | 'unknown';

/**
 * Scanner error with type and original error
 */
export interface ScannerError {
  type: ScannerErrorType;
  message: string;
  originalError?: Error;
}

/**
 * Detected QR code result
 */
export interface QRCodeResult {
  /** The decoded text content of the QR code */
  data: string;
  /** Location of the QR code in the frame */
  location: {
    topLeftCorner: { x: number; y: number };
    topRightCorner: { x: number; y: number };
    bottomLeftCorner: { x: number; y: number };
    bottomRightCorner: { x: number; y: number };
  };
}

/**
 * Event types emitted by WebCameraScanner
 */
export type ScannerEventType =
  | 'state_change'
  | 'code_detected'
  | 'error';

/**
 * Base scanner event
 */
export interface ScannerEvent {
  type: ScannerEventType;
}

/**
 * State change event
 */
export interface StateChangeEvent extends ScannerEvent {
  type: 'state_change';
  state: ScannerState;
  previousState: ScannerState;
}

/**
 * Code detected event
 */
export interface CodeDetectedEvent extends ScannerEvent {
  type: 'code_detected';
  result: QRCodeResult;
}

/**
 * Error event
 */
export interface ScannerErrorEvent extends ScannerEvent {
  type: 'error';
  error: ScannerError;
}

/**
 * Union of all scanner events
 */
export type AnyScannerEvent =
  | StateChangeEvent
  | CodeDetectedEvent
  | ScannerErrorEvent;

/**
 * Event subscriber callback
 */
export type ScannerEventSubscriber = (event: AnyScannerEvent) => void;

/**
 * Callback for detected QR codes
 */
export type CodeDetectedCallback = (result: QRCodeResult) => void;

/**
 * Configuration for WebCameraScanner
 */
export interface WebCameraScannerConfig {
  /** Preferred camera facing mode (default: 'environment') */
  facingMode?: CameraFacingMode;
  /** Scan interval in milliseconds (default: 100ms) */
  scanInterval?: number;
  /** Video width constraint (default: 1280) */
  videoWidth?: number;
  /** Video height constraint (default: 720) */
  videoHeight?: number;
}

/**
 * Default configuration values
 */
const DEFAULT_CONFIG: Required<WebCameraScannerConfig> = {
  facingMode: 'environment',
  scanInterval: 100,
  videoWidth: 1280,
  videoHeight: 720,
};

/**
 * Map DOMException names to ScannerErrorType
 */
function mapErrorType(error: unknown): ScannerErrorType {
  if (error instanceof DOMException) {
    switch (error.name) {
      case 'NotAllowedError':
      case 'PermissionDeniedError':
        return 'permission_denied';
      case 'NotFoundError':
      case 'DevicesNotFoundError':
        return 'not_found';
      case 'NotReadableError':
      case 'TrackStartError':
        return 'not_readable';
      case 'OverconstrainedError':
      case 'ConstraintNotSatisfiedError':
        return 'overconstrained';
      case 'SecurityError':
        return 'security';
      case 'AbortError':
        return 'abort';
      default:
        return 'unknown';
    }
  }
  return 'unknown';
}

/**
 * WebCameraScanner manages camera access and QR code scanning
 */
export class WebCameraScanner {
  private config: Required<WebCameraScannerConfig>;
  private state: ScannerState = 'idle';
  private subscribers: Set<ScannerEventSubscriber> = new Set();
  private onCodeDetected: CodeDetectedCallback | null = null;

  private stream: MediaStream | null = null;
  private videoElement: HTMLVideoElement | null = null;
  private canvasElement: HTMLCanvasElement | null = null;
  private canvasContext: CanvasRenderingContext2D | null = null;
  private animationFrameId: number | null = null;
  private lastScanTime: number = 0;
  private currentFacingMode: CameraFacingMode;
  private lastDetectedCode: string | null = null;
  private codeDebounceTimeout: ReturnType<typeof setTimeout> | null = null;

  constructor(config: WebCameraScannerConfig = {}) {
    this.config = {
      ...DEFAULT_CONFIG,
      ...config,
    };
    this.currentFacingMode = this.config.facingMode;
  }

  /**
   * Subscribe to scanner events
   */
  subscribe(callback: ScannerEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Set callback for detected codes (alternative to event subscription)
   */
  setOnCodeDetected(callback: CodeDetectedCallback | null): void {
    this.onCodeDetected = callback;
  }

  /**
   * Emit an event to all subscribers
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
   * Update scanner state and emit state change event
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
   * Get current scanner state
   */
  getState(): ScannerState {
    return this.state;
  }

  /**
   * Get current camera facing mode
   */
  getFacingMode(): CameraFacingMode {
    return this.currentFacingMode;
  }

  /**
   * Check if scanner is actively scanning
   */
  isScanning(): boolean {
    return this.state === 'scanning';
  }

  /**
   * Bind a video element for preview display
   */
  bindVideoElement(video: HTMLVideoElement): void {
    this.videoElement = video;

    // Configure video element
    this.videoElement.setAttribute('playsinline', 'true');
    this.videoElement.setAttribute('autoplay', 'true');
    this.videoElement.muted = true;

    // If we already have a stream, attach it
    if (this.stream) {
      this.videoElement.srcObject = this.stream;
    }
  }

  /**
   * Unbind the video element
   */
  unbindVideoElement(): void {
    if (this.videoElement) {
      this.videoElement.srcObject = null;
    }
    this.videoElement = null;
  }

  /**
   * Get video constraints for getUserMedia
   */
  private getVideoConstraints(): MediaStreamConstraints {
    return {
      audio: false,
      video: {
        facingMode: { ideal: this.currentFacingMode },
        width: { ideal: this.config.videoWidth },
        height: { ideal: this.config.videoHeight },
      },
    };
  }

  /**
   * Request camera permission and start the camera stream
   */
  async start(): Promise<void> {
    if (this.state === 'scanning') {
      return;
    }

    this.setState('requesting');

    try {
      // Check if getUserMedia is available
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new DOMException(
          'Camera API not available',
          'NotFoundError'
        );
      }

      // Request camera access
      this.stream = await navigator.mediaDevices.getUserMedia(
        this.getVideoConstraints()
      );

      // Create canvas for frame analysis
      this.canvasElement = document.createElement('canvas');
      this.canvasContext = this.canvasElement.getContext('2d', {
        willReadFrequently: true,
      });

      // Attach stream to video element if bound
      if (this.videoElement) {
        this.videoElement.srcObject = this.stream;

        // Wait for video to be ready
        await new Promise<void>((resolve, reject) => {
          if (!this.videoElement) {
            resolve();
            return;
          }

          const onLoadedMetadata = () => {
            this.videoElement?.removeEventListener('loadedmetadata', onLoadedMetadata);
            this.videoElement?.play()
              .then(() => resolve())
              .catch(reject);
          };

          this.videoElement.addEventListener('loadedmetadata', onLoadedMetadata);
        });
      }

      this.setState('scanning');
      this.startScanLoop();
    } catch (error) {
      this.handleError(error);
      throw error;
    }
  }

  /**
   * Stop the camera stream and scanning
   */
  stop(): void {
    this.stopScanLoop();
    this.stopStream();
    this.cleanup();
    this.setState('idle');
  }

  /**
   * Pause scanning without stopping the camera
   */
  pause(): void {
    if (this.state === 'scanning') {
      this.stopScanLoop();
      this.setState('paused');
    }
  }

  /**
   * Resume scanning after pause
   */
  resume(): void {
    if (this.state === 'paused' && this.stream) {
      this.setState('scanning');
      this.startScanLoop();
    }
  }

  /**
   * Toggle between front and back camera
   */
  async toggleCamera(): Promise<void> {
    const newFacingMode: CameraFacingMode =
      this.currentFacingMode === 'environment' ? 'user' : 'environment';

    await this.setFacingMode(newFacingMode);
  }

  /**
   * Set specific camera facing mode
   */
  async setFacingMode(facingMode: CameraFacingMode): Promise<void> {
    if (facingMode === this.currentFacingMode && this.stream) {
      return;
    }

    this.currentFacingMode = facingMode;

    // If currently scanning or paused, restart with new camera
    if (this.state === 'scanning' || this.state === 'paused') {
      this.stopScanLoop();
      this.stopStream();
      // Reset state to idle so start() will proceed
      this.setState('idle');
      await this.start();
    }
  }

  /**
   * Start the scan loop using requestAnimationFrame
   */
  private startScanLoop(): void {
    if (this.animationFrameId !== null) {
      return;
    }

    const scanFrame = (timestamp: number) => {
      if (this.state !== 'scanning') {
        return;
      }

      // Throttle scanning based on scanInterval
      if (timestamp - this.lastScanTime >= this.config.scanInterval) {
        this.lastScanTime = timestamp;
        this.analyzeFrame();
      }

      this.animationFrameId = requestAnimationFrame(scanFrame);
    };

    this.animationFrameId = requestAnimationFrame(scanFrame);
  }

  /**
   * Stop the scan loop
   */
  private stopScanLoop(): void {
    if (this.animationFrameId !== null) {
      cancelAnimationFrame(this.animationFrameId);
      this.animationFrameId = null;
    }
  }

  /**
   * Analyze current video frame for QR codes
   */
  private analyzeFrame(): void {
    if (!this.videoElement || !this.canvasElement || !this.canvasContext) {
      return;
    }

    const video = this.videoElement;

    // Check if video is ready
    if (video.readyState !== video.HAVE_ENOUGH_DATA) {
      return;
    }

    const width = video.videoWidth;
    const height = video.videoHeight;

    if (width === 0 || height === 0) {
      return;
    }

    // Set canvas size to match video
    this.canvasElement.width = width;
    this.canvasElement.height = height;

    // Draw current video frame to canvas
    this.canvasContext.drawImage(video, 0, 0, width, height);

    // Get image data for QR analysis
    const imageData = this.canvasContext.getImageData(0, 0, width, height);

    // Use jsQR to detect QR code
    const code = jsQR(imageData.data, width, height, {
      inversionAttempts: 'dontInvert',
    });

    if (code) {
      this.handleDetectedCode(code);
    }
  }

  /**
   * Handle a detected QR code
   */
  private handleDetectedCode(code: ReturnType<typeof jsQR>): void {
    if (!code) return;

    // Debounce repeated detections of the same code
    if (code.data === this.lastDetectedCode) {
      return;
    }

    this.lastDetectedCode = code.data;

    // Clear the debounce after a short delay to allow re-detection
    if (this.codeDebounceTimeout) {
      clearTimeout(this.codeDebounceTimeout);
    }
    this.codeDebounceTimeout = setTimeout(() => {
      this.lastDetectedCode = null;
      this.codeDebounceTimeout = null;
    }, 1000);

    const result: QRCodeResult = {
      data: code.data,
      location: {
        topLeftCorner: code.location.topLeftCorner,
        topRightCorner: code.location.topRightCorner,
        bottomLeftCorner: code.location.bottomLeftCorner,
        bottomRightCorner: code.location.bottomRightCorner,
      },
    };

    // Emit event
    this.emit({
      type: 'code_detected',
      result,
    });

    // Call callback if set
    if (this.onCodeDetected) {
      try {
        this.onCodeDetected(result);
      } catch (error) {
        console.error('Error in onCodeDetected callback:', error);
      }
    }
  }

  /**
   * Handle errors during camera operations
   */
  private handleError(error: unknown): void {
    const errorType = mapErrorType(error);
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';

    const scannerError: ScannerError = {
      type: errorType,
      message: errorMessage,
      originalError: error instanceof Error ? error : undefined,
    };

    this.setState('error');
    this.emit({
      type: 'error',
      error: scannerError,
    });
  }

  /**
   * Stop the media stream
   */
  private stopStream(): void {
    if (this.stream) {
      this.stream.getTracks().forEach((track) => {
        track.stop();
      });
      this.stream = null;
    }

    if (this.videoElement) {
      this.videoElement.srcObject = null;
    }
  }

  /**
   * Clean up resources
   */
  private cleanup(): void {
    if (this.codeDebounceTimeout) {
      clearTimeout(this.codeDebounceTimeout);
      this.codeDebounceTimeout = null;
    }

    this.canvasElement = null;
    this.canvasContext = null;
    this.lastDetectedCode = null;
    this.lastScanTime = 0;
  }

  /**
   * Destroy the scanner and clean up all resources
   */
  destroy(): void {
    this.stop();
    this.unbindVideoElement();
    this.subscribers.clear();
    this.onCodeDetected = null;
  }

  /**
   * Check if camera permission has been granted
   * Note: This only works in browsers that support the Permissions API
   */
  static async checkPermission(): Promise<PermissionState | null> {
    try {
      if (!navigator.permissions?.query) {
        return null;
      }
      const result = await navigator.permissions.query({ name: 'camera' as PermissionName });
      return result.state;
    } catch {
      return null;
    }
  }

  /**
   * Check if camera is available on this device
   */
  static async isCameraAvailable(): Promise<boolean> {
    try {
      if (!navigator.mediaDevices?.enumerateDevices) {
        return false;
      }
      const devices = await navigator.mediaDevices.enumerateDevices();
      return devices.some((device) => device.kind === 'videoinput');
    } catch {
      return false;
    }
  }
}

/**
 * Singleton instance of WebCameraScanner
 */
let scannerInstance: WebCameraScanner | null = null;

/**
 * Get or create a singleton WebCameraScanner instance
 */
export function getWebCameraScanner(config?: WebCameraScannerConfig): WebCameraScanner {
  if (!scannerInstance) {
    scannerInstance = new WebCameraScanner(config);
  }
  return scannerInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetWebCameraScanner(): void {
  if (scannerInstance) {
    scannerInstance.destroy();
  }
  scannerInstance = null;
}
