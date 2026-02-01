import { Component, createSignal, onMount, onCleanup, Show } from 'solid-js';
import { getBarcodeScanner, type ScannerState, type AnyScannerEvent } from '../../lib/scanner/BarcodeScanner';

export interface QRScannerProps {
  /** Called when a QR code is successfully scanned */
  onScan: (data: string) => void;
  /** Called when an error occurs */
  onError?: (error: string) => void;
  /** Additional CSS class for the container */
  class?: string;
}

/**
 * QR Scanner component for pairing flow.
 *
 * Uses the WebCameraScanner for browser-based QR code scanning,
 * or the native Tauri barcode scanner plugin on mobile.
 */
const QRScanner: Component<QRScannerProps> = (props) => {
  const [state, setState] = createSignal<ScannerState>('idle');
  const [error, setError] = createSignal<string | null>(null);
  const [isStarting, setIsStarting] = createSignal(false);
  let videoRef: HTMLVideoElement | undefined;

  const scanner = getBarcodeScanner();

  const handleEvent = (event: AnyScannerEvent) => {
    switch (event.type) {
      case 'state_change':
        setState(event.state);
        if (event.state === 'error') {
          setError('Scanner error occurred');
        }
        break;
      case 'code_detected':
        // Stop scanning and emit the result
        scanner.stop();
        props.onScan(event.result.data);
        break;
      case 'error':
        setError(event.error.message);
        props.onError?.(event.error.message);
        break;
    }
  };

  const startScanning = async () => {
    if (isStarting()) return;

    setIsStarting(true);
    setError(null);

    try {
      // Check/request permission first
      const permission = await scanner.checkPermission();
      if (permission === 'denied') {
        setError('Camera permission denied. Please allow camera access to scan QR codes.');
        props.onError?.('Camera permission denied');
        setIsStarting(false);
        return;
      }

      if (permission === 'prompt' || permission === null) {
        const granted = await scanner.requestPermission();
        if (!granted) {
          setError('Camera permission denied. Please allow camera access to scan QR codes.');
          props.onError?.('Camera permission denied');
          setIsStarting(false);
          return;
        }
      }

      // Bind video element for web scanner
      const webScanner = scanner.getWebScanner();
      if (webScanner && videoRef) {
        webScanner.bindVideoElement(videoRef);
      }

      // Start scanning
      await scanner.scan();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to start scanner';
      setError(errorMessage);
      props.onError?.(errorMessage);
    } finally {
      setIsStarting(false);
    }
  };

  const stopScanning = () => {
    scanner.stop();
    setState('idle');
  };

  onMount(() => {
    // Subscribe to scanner events
    const unsubscribe = scanner.subscribe(handleEvent);

    // Bind video element to web scanner
    const webScanner = scanner.getWebScanner();
    if (webScanner && videoRef) {
      webScanner.bindVideoElement(videoRef);
    }

    onCleanup(() => {
      unsubscribe();
      scanner.stop();
    });
  });

  const isScanning = () => state() === 'scanning' || state() === 'requesting';

  return (
    <div class={`qr-scanner ${props.class ?? ''}`} data-testid="qr-scanner">
      <div class="qr-scanner__viewport">
        <video
          ref={videoRef}
          class="qr-scanner__video"
          playsinline
          autoplay
          muted
        />
        <Show when={isScanning()}>
          <div class="qr-scanner__overlay">
            <div class="qr-scanner__frame" />
          </div>
        </Show>
        <Show when={!isScanning() && !error()}>
          <div class="qr-scanner__placeholder">
            <p>Camera preview will appear here</p>
          </div>
        </Show>
      </div>

      <Show when={error()}>
        <div class="qr-scanner__error" role="alert">
          {error()}
        </div>
      </Show>

      <div class="qr-scanner__controls">
        <Show
          when={isScanning()}
          fallback={
            <button
              class="qr-scanner__button qr-scanner__button--start"
              onClick={startScanning}
              disabled={isStarting()}
            >
              {isStarting() ? 'Starting...' : 'Start Camera'}
            </button>
          }
        >
          <button
            class="qr-scanner__button qr-scanner__button--stop"
            onClick={stopScanning}
          >
            Stop Camera
          </button>
        </Show>
      </div>

      <p class="qr-scanner__hint">
        Point your camera at the QR code displayed on the daemon
      </p>
    </div>
  );
};

export default QRScanner;
