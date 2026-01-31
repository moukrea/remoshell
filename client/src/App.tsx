import { Component, createSignal, Show, createEffect, onMount, onCleanup, For, lazy, Suspense } from 'solid-js';
import { AppShell, type AppView } from './components/layout';
import { getConnectionStore, type ConnectionEvent } from './stores/connection';
import { getSessionStore, type SessionEvent } from './stores/sessions';
import { getDeviceStore, type DeviceEvent } from './stores/devices';
import { getFileStore, type FileEvent, type FileEntry } from './stores/files';
import { OfflineIndicator } from './components/offline';

// Lazy-loaded components for better initial load performance
// XTermWrapper is a heavy dependency (xterm.js + WebGL addon)
const XTermWrapper = lazy(() => import('./components/terminal/XTermWrapper'));

// FileBrowser is only needed when viewing files
const FileBrowser = lazy(() => import('./components/files/FileBrowser'));

// DeviceList and PairingCodeInput are lighter but still lazy-loaded
const DeviceList = lazy(() => import('./components/devices/DeviceList'));
const PairingCodeInput = lazy(() => import('./components/pairing/PairingCodeInput'));

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
  let terminalRef: any;

  // Get active session
  const activeSession = () => sessionStore.getActiveSession();

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
          <Suspense fallback={<LoadingFallback />}>
            <XTermWrapper
              ref={terminalRef}
              onData={handleTerminalData}
              onResize={handleTerminalResize}
              class="terminal-wrapper"
            />
          </Suspense>
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
    // Start a download transfer
    fileStore.startDownload(entry.path, entry.name, entry.size);
    // In a real implementation, this would trigger the actual download via the connection
  };

  const handleUpload = (files: File[]) => {
    // Start upload transfers for each file
    files.forEach(file => {
      fileStore.startUpload(file, fileStore.state.currentPath);
    });
    // In a real implementation, this would trigger the actual upload via the connection
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
        <Suspense fallback={<LoadingFallback />}>
          <FileBrowser
            onDownload={handleDownload}
            onUpload={handleUpload}
            class="files-browser"
          />
        </Suspense>
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

  const handleConnect = (deviceId: string) => {
    // Start connection process
    connectionStore.connectToPeer(deviceId);
    deviceStore.recordConnection(deviceId);
  };

  const handleSelectDevice = (deviceId: string) => {
    setSelectedDeviceId(deviceId);
  };

  const handlePairingComplete = (code: string) => {
    // Process pairing code
    // In a real implementation, this would:
    // 1. Parse the QR code data
    // 2. Extract device info and signaling room
    // 3. Connect to the signaling server
    // 4. Initiate WebRTC connection
    console.log('Pairing code entered:', code);

    // For demo purposes, add a fake device
    const newDeviceId = `device-${Date.now()}`;
    deviceStore.addDevice({
      id: newDeviceId,
      name: `Device ${code.substring(0, 3)}`,
      platform: 'linux',
    });

    setShowPairing(false);
    setPairingError(null);
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
          <h3>Enter Pairing Code</h3>
          <p>Enter the code shown on the remote device</p>
          <Suspense fallback={<LoadingFallback />}>
            <PairingCodeInput
              autoFocus
              onComplete={handlePairingComplete}
              error={!!pairingError()}
              errorMessage={pairingError() ?? undefined}
            />
          </Suspense>
          <p class="devices-pairing__hint">
            Or scan the QR code with your camera (coming soon)
          </p>
        </div>
      </Show>

      {/* Device list */}
      <div class="devices-content">
        <Suspense fallback={<LoadingFallback />}>
          <DeviceList
            onConnect={handleConnect}
            onSelectDevice={handleSelectDevice}
          />
        </Suspense>
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
  const [isDarkTheme, setIsDarkTheme] = createSignal(true);

  // Update document class for theme
  createEffect(() => {
    document.documentElement.classList.toggle('dark', isDarkTheme());
    document.documentElement.classList.toggle('light', !isDarkTheme());
  });

  // Subscribe to store events for logging/debugging
  onMount(() => {
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
