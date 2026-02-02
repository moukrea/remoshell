import { Component, createSignal, Show, createEffect, onMount, onCleanup, For, lazy, Suspense, ErrorBoundary } from 'solid-js';
import { ErrorFallback } from './components/error';
import { AppShell, type AppView } from './components/layout';
import { getConnectionStore, type ConnectionEvent } from './stores/connection';
import { getSessionStore, type SessionEvent } from './stores/sessions';
import { getDeviceStore, type DeviceEvent } from './stores/devices';
import { getFileStore, type FileEvent, type FileEntry } from './stores/files';
import { getNotificationStore } from './stores/notifications';
import { OfflineIndicator } from './components/offline';
import { ToastContainer } from './components/notifications';
import { getOrchestrator } from './lib/orchestration/ConnectionOrchestrator';
import { initializeAppLifecycle } from './lib/lifecycle/AppLifecycle';
import { parsePairingData, isPairingExpired } from './lib/scanner/BarcodeScanner';
import { setSignalingUrl } from './config';

// Lazy-loaded components for better initial load performance
// XTermWrapper is a heavy dependency (xterm.js + WebGL addon)
const XTermWrapper = lazy(() => import('./components/terminal/XTermWrapper'));
import type { XTermWrapperHandle } from './components/terminal/XTermWrapper';

// FileBrowser is only needed when viewing files
const FileBrowser = lazy(() => import('./components/files/FileBrowser'));

// FileTransferProgress shows download/upload progress
const FileTransferProgress = lazy(() => import('./components/files/FileTransferProgress'));

// DeviceList and PairingCodeInput are lighter but still lazy-loaded
const DeviceList = lazy(() => import('./components/devices/DeviceList'));
const PairingCodeInput = lazy(() => import('./components/pairing/PairingCodeInput'));
const QRScanner = lazy(() => import('./components/pairing/QRScanner'));

/**
 * Loading fallback component for lazy-loaded views
 */
const LoadingFallback: Component = () => (
  <div class="loading-fallback" style={{ display: 'flex', 'align-items': 'center', 'justify-content': 'center', height: '100%' }}>
    <span>Loading...</span>
  </div>
);

/**
 * Map signaling status to connection status for header display
 */
const mapConnectionStatus = (signalingStatus: 'disconnected' | 'connecting' | 'connected'): 'connected' | 'connecting' | 'disconnected' => {
  return signalingStatus;
};

/**
 * Terminal view component
 */
const TerminalView: Component = () => {
  const sessionStore = getSessionStore();
  const connectionStore = getConnectionStore();
  const [terminalHandle, setTerminalHandle] = createSignal<XTermWrapperHandle | undefined>();

  // Get active session
  const activeSession = () => sessionStore.getActiveSession();

  // Subscribe to session output events
  createEffect(() => {
    const session = activeSession();
    if (!session) return;

    const unsub = sessionStore.subscribe((event) => {
      if (event.type === 'session:output' && event.sessionId === session.id) {
        const data = event.data as { output: string } | undefined;
        const handle = terminalHandle();
        if (data?.output && handle) {
          handle.write(data.output);
        }
      }
    });

    onCleanup(unsub);
  });

  // Create a new session when connected
  const handleNewSession = () => {
    const activePeer = connectionStore.getActivePeer();
    if (activePeer && activePeer.status === 'connected') {
      const sessionId = sessionStore.createSession({
        peerId: activePeer.id,
        title: 'Terminal',
      });
      sessionStore.setSessionStatus(sessionId, 'connected');
    }
  };

  // Handle terminal input
  const handleTerminalData = (data: string) => {
    const session = activeSession();
    if (session) {
      sessionStore.sendInput(session.id, data);
    }
  };

  // Handle terminal resize
  const handleTerminalResize = (cols: number, rows: number) => {
    const session = activeSession();
    if (session) {
      sessionStore.resizeTerminal(session.id, cols, rows);
    }
  };

  // Get sessions in order for tabs
  const sessions = () => sessionStore.getSessionsInOrder();

  return (
    <div class="terminal-view" data-testid="terminal-view">
      {/* Session tabs */}
      <div class="terminal-tabs">
        <For each={sessions()}>
          {(session) => (
            <button
              class={`terminal-tab ${session.id === sessionStore.state.activeSessionId ? 'terminal-tab--active' : ''}`}
              onClick={() => sessionStore.setActiveSession(session.id)}
            >
              <span class="terminal-tab__title">{session.title}</span>
              <button
                class="terminal-tab__close"
                onClick={(e) => {
                  e.stopPropagation();
                  sessionStore.closeSession(session.id);
                }}
              >
                x
              </button>
            </button>
          )}
        </For>
        <button
          class="terminal-tab terminal-tab--new"
          onClick={handleNewSession}
          disabled={connectionStore.state.signalingStatus !== 'connected'}
        >
          +
        </button>
      </div>

      {/* Terminal content */}
      <Show
        when={activeSession()}
        fallback={
          <div class="terminal-placeholder">
            <Show
              when={connectionStore.state.signalingStatus === 'connected'}
              fallback={
                <div class="terminal-placeholder__message">
                  <h2>Not Connected</h2>
                  <p>Connect to a device to start a terminal session</p>
                </div>
              }
            >
              <div class="terminal-placeholder__message">
                <h2>No Active Session</h2>
                <p>Click + to create a new terminal session</p>
              </div>
            </Show>
          </div>
        }
      >
        <div class="terminal-container">
          <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
            <Suspense fallback={<LoadingFallback />}>
              <XTermWrapper
                ref={setTerminalHandle}
                onData={handleTerminalData}
                onResize={handleTerminalResize}
                class="terminal-wrapper"
              />
            </Suspense>
          </ErrorBoundary>
        </div>
      </Show>
    </div>
  );
};

/**
 * Files view component
 */
const FilesView: Component = () => {
  const fileStore = getFileStore();
  const connectionStore = getConnectionStore();

  // Navigate to home directory on mount if connected
  onMount(() => {
    if (connectionStore.state.signalingStatus === 'connected') {
      fileStore.navigate('/');
    }
  });

  const handleDownload = (entry: FileEntry) => {
    // Start a download transfer - this emits 'files:download' event
    // which the orchestrator listens to and initiates the actual download
    const transferId = fileStore.startDownload(entry.path, entry.name, entry.size);
    // Mark transfer as started (in_progress)
    fileStore.setTransferStarted(transferId);
    // The orchestrator handles:
    // 1. Sending FileDownloadRequest to peer
    // 2. Receiving FileDownloadChunk responses
    // 3. Calling fileStore.receiveChunk() to track progress and assemble file
    // 4. Triggering browser download when complete
  };

  const handleUpload = async (files: File[]) => {
    const orchestrator = getOrchestrator();

    for (const file of files) {
      // Start tracking the upload in the store
      const transferId = fileStore.startUpload(file, fileStore.state.currentPath);

      // Calculate destination path
      const destPath = `${fileStore.state.currentPath}/${file.name}`.replace(/\/+/g, '/');

      // Mark transfer as started
      fileStore.setTransferStarted(transferId);

      // Start the actual upload
      try {
        await orchestrator.uploadFile(file, destPath, (progress) => {
          fileStore.updateTransferProgress(transferId, progress.bytesSent);
        });

        fileStore.completeTransfer(transferId);
        // Refresh directory listing to show the uploaded file
        fileStore.refresh();
      } catch (error) {
        fileStore.failTransfer(transferId, error instanceof Error ? error.message : 'Upload failed');
      }
    }
  };

  return (
    <div class="files-view" data-testid="files-view">
      <Show
        when={connectionStore.state.signalingStatus === 'connected'}
        fallback={
          <div class="files-placeholder">
            <h2>Not Connected</h2>
            <p>Connect to a device to browse files</p>
          </div>
        }
      >
        <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
          <Suspense fallback={<LoadingFallback />}>
            <FileBrowser
              onDownload={handleDownload}
              onUpload={handleUpload}
              class="files-browser"
            />
          </Suspense>
        </ErrorBoundary>
        <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
          <Suspense fallback={<LoadingFallback />}>
            <FileTransferProgress showOnlyActive={false} maxItems={10} />
          </Suspense>
        </ErrorBoundary>
      </Show>
    </div>
  );
};

/**
 * Devices view component with pairing
 */
const DevicesView: Component = () => {
  const deviceStore = getDeviceStore();
  const connectionStore = getConnectionStore();
  const [showPairing, setShowPairing] = createSignal(false);
  const [pairingError, setPairingError] = createSignal<string | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = createSignal<string | null>(null);
  const [scannerKey, setScannerKey] = createSignal(0);

  const handleConnect = (deviceId: string) => {
    // Start connection process
    connectionStore.connectToPeer(deviceId);
    deviceStore.recordConnection(deviceId);
  };

  const handleSelectDevice = (deviceId: string) => {
    setSelectedDeviceId(deviceId);
  };

  const handlePairingComplete = async (code: string) => {
    try {
      // Clear any previous error
      setPairingError(null);

      // Parse the pairing code (can be raw JSON or base58 encoded)
      const parseResult = parsePairingData(code);
      if (!parseResult.success) {
        setPairingError(parseResult.error);
        console.error('[Pairing] Parse failed:', parseResult.error);
        return;
      }

      const pairingData = parseResult.data;

      // Validate expiry
      if (isPairingExpired(pairingData)) {
        setPairingError('Pairing code has expired');
        console.error('[Pairing] Code expired');
        return;
      }

      // Update signaling URL from pairing data
      if (pairingData.relay_url) {
        setSignalingUrl(pairingData.relay_url);
        console.log('[Pairing] Using relay URL:', pairingData.relay_url);
      }

      // Add device to store
      deviceStore.addDevice({
        id: pairingData.device_id,
        name: `Device ${pairingData.device_id.substring(0, 8)}`,
        platform: 'unknown',
      });

      // Connect via orchestrator using device_id as room ID
      const orchestrator = getOrchestrator();
      await orchestrator.connect(pairingData.device_id);

      setShowPairing(false);
      console.log('[Pairing] Successfully initiated connection to device:', pairingData.device_id);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error during pairing';
      setPairingError(errorMessage);
      console.error('[Pairing] Failed:', error);
    }
  };

  const selectedDevice = () => {
    const id = selectedDeviceId();
    return id ? deviceStore.getDevice(id) : null;
  };

  return (
    <div class="devices-view" data-testid="devices-view">
      <div class="devices-header">
        <h2>Devices</h2>
        <button
          class="devices-add-btn"
          onClick={() => setShowPairing(!showPairing())}
        >
          {showPairing() ? 'Cancel' : 'Add Device'}
        </button>
      </div>

      {/* Pairing section */}
      <Show when={showPairing()}>
        <div class="devices-pairing" data-testid="pairing-section">
          <h3>Pair a Device</h3>

          {/* QR Scanner - keyed by scannerKey to force re-mount on rescan */}
          <div class="pairing-scanner" data-scanner-key={scannerKey()}>
            <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
              <Suspense fallback={<LoadingFallback />}>
                <Show when={scannerKey() >= 0} keyed>
                  <QRScanner
                    onScan={(data) => handlePairingComplete(data)}
                    onError={(err) => {
                      console.error('[Pairing] Scan error:', err);
                      setPairingError(err);
                    }}
                  />
                </Show>
              </Suspense>
            </ErrorBoundary>
          </div>

          {/* Pairing error with scan again option */}
          <Show when={pairingError()}>
            <div class="pairing-error" data-testid="pairing-error">
              <span class="pairing-error__message">{pairingError()}</span>
              <Show when={pairingError()?.toLowerCase().includes('expired')}>
                <button
                  class="pairing-error__rescan-btn"
                  data-testid="pairing-rescan"
                  onClick={() => {
                    setPairingError(null);
                    // Increment key to force QRScanner re-mount and restart
                    setScannerKey(k => k + 1);
                  }}
                >
                  Scan again
                </button>
              </Show>
            </div>
          </Show>

          <div class="pairing-divider">
            <span>or enter code manually</span>
          </div>

          <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
            <Suspense fallback={<LoadingFallback />}>
              <PairingCodeInput
                autoFocus={false}
                onComplete={handlePairingComplete}
                error={!!pairingError()}
                errorMessage={pairingError() ?? undefined}
              />
            </Suspense>
          </ErrorBoundary>
        </div>
      </Show>

      {/* Device list */}
      <div class="devices-content">
        <ErrorBoundary fallback={(err, reset) => <ErrorFallback error={err} reset={reset} />}>
          <Suspense fallback={<LoadingFallback />}>
            <DeviceList
              onConnect={handleConnect}
              onSelectDevice={handleSelectDevice}
            />
          </Suspense>
        </ErrorBoundary>
      </div>

      {/* Device details panel */}
      <Show when={selectedDevice()}>
        <div class="device-details" data-testid="device-details">
          <h3>{selectedDevice()!.name}</h3>
          <dl class="device-details__info">
            <dt>Status</dt>
            <dd>{selectedDevice()!.status}</dd>
            <dt>Platform</dt>
            <dd>{selectedDevice()!.platform}</dd>
            <dt>Paired</dt>
            <dd>{new Date(selectedDevice()!.pairedAt).toLocaleDateString()}</dd>
            <dt>Last Seen</dt>
            <dd>{new Date(selectedDevice()!.lastSeen).toLocaleString()}</dd>
          </dl>
          <div class="device-details__actions">
            <button
              onClick={() => handleConnect(selectedDevice()!.id)}
              disabled={selectedDevice()!.status === 'online'}
            >
              Connect
            </button>
            <button onClick={() => setSelectedDeviceId(null)}>
              Close
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

/**
 * Main App component with routing and store integration
 */
const App: Component = () => {
  const connectionStore = getConnectionStore();
  const sessionStore = getSessionStore();
  const deviceStore = getDeviceStore();
  const fileStore = getFileStore();

  const [activeView, setActiveView] = createSignal<AppView>('terminal');

  // Load theme from localStorage, default to dark if not set
  const savedTheme = localStorage.getItem('remoshell-theme');
  const [isDarkTheme, setIsDarkTheme] = createSignal(savedTheme !== 'light');

  // Update document class for theme and persist to localStorage
  createEffect(() => {
    const isDark = isDarkTheme();
    document.documentElement.classList.toggle('dark', isDark);
    document.documentElement.classList.toggle('light', !isDark);
    localStorage.setItem('remoshell-theme', isDark ? 'dark' : 'light');
  });

  // Subscribe to store events for logging/debugging
  onMount(async () => {
    // Initialize app lifecycle and connection orchestrator
    try {
      // Initialize lifecycle management (handles foreground/background)
      await initializeAppLifecycle();
      console.log('[App] Lifecycle manager initialized');

      // Initialize connection orchestrator
      const orchestrator = getOrchestrator();
      await orchestrator.initialize();
      console.log('[App] Connection orchestrator initialized');
    } catch (error) {
      console.error('[App] Initialization failed:', error);
      const notifications = getNotificationStore();
      notifications.show({
        type: 'error',
        title: 'Initialization Failed',
        message: 'Failed to initialize the app. Some features may not work correctly.',
        duration: 10000, // 10 seconds - longer since this is important
      });
    }

    // Connection store events
    const unsubConnection = connectionStore.subscribe((event: ConnectionEvent) => {
      console.log('[Connection]', event.type, event);

      // Handle peer connection changes
      if (event.type === 'peer:connected' && event.peerId) {
        deviceStore.setDeviceStatus(event.peerId, 'online');
      } else if ((event.type === 'peer:disconnected' || event.type === 'peer:error') && event.peerId) {
        deviceStore.recordDisconnection(event.peerId, event.error);
        sessionStore.closeSessionsByPeer(event.peerId);
      }
    });

    // Session store events
    const unsubSession = sessionStore.subscribe((event: SessionEvent) => {
      console.log('[Session]', event.type, event);
    });

    // Device store events
    const unsubDevice = deviceStore.subscribe((event: DeviceEvent) => {
      console.log('[Device]', event.type, event);
    });

    // File store events
    const unsubFile = fileStore.subscribe((event: FileEvent) => {
      console.log('[File]', event.type, event);
    });

    // Cleanup subscriptions
    onCleanup(() => {
      unsubConnection();
      unsubSession();
      unsubDevice();
      unsubFile();
    });
  });

  const handleViewChange = (view: AppView) => {
    setActiveView(view);
  };

  const handleThemeChange = (isDark: boolean) => {
    setIsDarkTheme(isDark);
  };

  // Get connection status from store
  const connectionStatus = () => mapConnectionStatus(connectionStore.state.signalingStatus);

  return (
    <>
      <OfflineIndicator />
      <ToastContainer position="top-right" />
      <AppShell
        activeView={activeView()}
        onViewChange={handleViewChange}
        isDarkTheme={isDarkTheme()}
        onThemeChange={handleThemeChange}
        connectionStatus={connectionStatus()}
      >
        {/* Route to appropriate view */}
        <Show when={activeView() === 'terminal'}>
          <TerminalView />
        </Show>

        <Show when={activeView() === 'files'}>
          <FilesView />
        </Show>

        <Show when={activeView() === 'devices'}>
          <DevicesView />
        </Show>
      </AppShell>
    </>
  );
};

export default App;
