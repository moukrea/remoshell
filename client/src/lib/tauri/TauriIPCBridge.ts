/**
 * Tauri IPC Bridge
 *
 * This module provides a TypeScript wrapper for Tauri IPC commands,
 * allowing the frontend to communicate with the Rust backend.
 *
 * It detects if running in a Tauri environment (via window.__TAURI__)
 * and provides typed methods matching the Rust commands.
 */

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * Channel type for QUIC data transfer.
 */
export type ChannelType = 'Control' | 'Terminal' | 'Files';

/**
 * Connection state for QUIC connections.
 */
export type ConnectionState =
  | 'Disconnected'
  | 'Connecting'
  | 'Connected'
  | 'Reconnecting'
  | 'Failed';

/**
 * Error returned from Tauri commands.
 */
export interface CommandError {
  /** Error code for programmatic handling. */
  code: string;
  /** Human-readable error message. */
  message: string;
}

/**
 * Request payload for connecting to a peer.
 */
export interface ConnectRequest {
  /** The node ID (public key) of the peer to connect to. */
  node_id: string;
  /** Optional relay URL for NAT traversal. */
  relay_url?: string;
  /** Optional direct addresses for the peer. */
  direct_addresses?: string[];
}

/**
 * Response from a successful connection.
 */
export interface ConnectResponse {
  /** Whether the connection was successful. */
  connected: boolean;
  /** The connected peer's node ID. */
  peer_node_id: string;
}

/**
 * Response from disconnection.
 */
export interface DisconnectResponse {
  /** Whether the disconnection was successful. */
  disconnected: boolean;
}

/**
 * Request payload for sending data.
 */
export interface SendDataRequest {
  /** The channel type to send on (Control, Terminal, or Files). */
  channel: ChannelType;
  /** The data to send (base64-encoded). */
  data: string;
}

/**
 * Response from sending data.
 */
export interface SendDataResponse {
  /** Whether the data was sent successfully. */
  sent: boolean;
  /** Number of bytes sent. */
  bytes_sent: number;
}

/**
 * Response for connection status.
 */
export interface ConnectionStatusResponse {
  /** Current connection state. */
  state: ConnectionState;
  /** Connected peer's node ID, if connected. */
  peer_node_id?: string;
  /** This client's node ID. */
  local_node_id?: string;
}

/**
 * Response for device keys.
 */
export interface DeviceKeysResponse {
  /** The device's secret key (base64-encoded). */
  secret_key: string;
  /** Whether the key was newly generated. */
  newly_generated: boolean;
}

/**
 * A paired device stored in the database.
 */
export interface PairedDevice {
  /** Unique identifier for the device. */
  id: string;
  /** Human-readable name of the device. */
  name: string;
  /** Public key of the device for authentication. */
  public_key: string;
  /** Unix timestamp when the device was first paired. */
  created_at: number;
  /** Unix timestamp when the device was last seen (connected). */
  last_seen?: number;
}

/**
 * Request payload for storing a paired device.
 */
export interface StorePairedDeviceRequest {
  /** Unique identifier for the device. */
  id: string;
  /** Human-readable name for the device. */
  name: string;
  /** Public key of the device. */
  public_key: string;
}

/**
 * Response from removing a paired device.
 */
export interface RemoveDeviceResponse {
  /** Whether the device was successfully removed. */
  removed: boolean;
}

/**
 * Request payload for showing a notification.
 */
export interface NotificationRequest {
  /** The notification title. */
  title: string;
  /** The notification body text. */
  body: string;
  /** Optional icon name or path. */
  icon?: string;
}

/**
 * Response from showing a notification.
 */
export interface NotificationResponse {
  /** Whether the notification was shown successfully. */
  shown: boolean;
}

/**
 * Request payload for initializing the application.
 */
export interface InitRequest {
  /** Path to the database file. */
  database_path: string;
  /** Optional relay URL for QUIC connections. */
  relay_url?: string;
}

/**
 * Response from initialization.
 */
export interface InitResponse {
  /** Whether initialization was successful. */
  initialized: boolean;
  /** The local node ID for QUIC connections. */
  node_id: string;
}

/**
 * Connection events emitted from the Rust backend.
 */
export type ConnectionEvent =
  | { type: 'StateChanged'; payload: ConnectionState }
  | { type: 'DataReceived'; payload: { channel: ChannelType; data: string } }
  | { type: 'Error'; payload: string }
  | { type: 'PeerInfo'; payload: { node_id: string } };

/**
 * Event subscriber callback type.
 */
export type TauriEventSubscriber = (event: ConnectionEvent) => void;

// ============================================================================
// Tauri API Types
// ============================================================================

/**
 * Tauri invoke function type.
 */
type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * Tauri event listener function type.
 */
type ListenFn = <T>(
  event: string,
  handler: (event: { payload: T }) => void
) => Promise<() => void>;

/**
 * Tauri API interface (subset of window.__TAURI__).
 */
interface TauriAPI {
  core: {
    invoke: InvokeFn;
  };
  event: {
    listen: ListenFn;
  };
}

/**
 * Extend Window interface for Tauri.
 */
declare global {
  interface Window {
    __TAURI__?: TauriAPI;
  }
}

// ============================================================================
// Tauri IPC Bridge
// ============================================================================

/**
 * TauriIPCBridge provides typed methods for invoking Tauri commands
 * and subscribing to events from the Rust backend.
 */
export class TauriIPCBridge {
  private subscribers: Set<TauriEventSubscriber> = new Set();
  private unlistenFns: (() => void)[] = [];
  private initialized = false;

  /**
   * Check if running in a Tauri environment.
   */
  static isAvailable(): boolean {
    return typeof window !== 'undefined' && window.__TAURI__ !== undefined;
  }

  /**
   * Get the Tauri API, throwing if not available.
   */
  private getTauri(): TauriAPI {
    if (!TauriIPCBridge.isAvailable()) {
      throw new TauriNotAvailableError();
    }
    return window.__TAURI__!;
  }

  /**
   * Invoke a Tauri command.
   */
  private async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const tauri = this.getTauri();
    try {
      return await tauri.core.invoke<T>(cmd, args);
    } catch (error) {
      throw this.wrapError(error);
    }
  }

  /**
   * Wrap an error from Tauri into a TauriCommandError.
   */
  private wrapError(error: unknown): TauriCommandError {
    if (error && typeof error === 'object') {
      const err = error as Record<string, unknown>;
      if (typeof err.code === 'string' && typeof err.message === 'string') {
        return new TauriCommandError(err.code, err.message);
      }
    }
    return new TauriCommandError(
      'UNKNOWN_ERROR',
      error instanceof Error ? error.message : String(error)
    );
  }

  /**
   * Subscribe to connection events from the Rust backend.
   */
  subscribe(callback: TauriEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Emit an event to all subscribers.
   */
  private emit(event: ConnectionEvent): void {
    this.subscribers.forEach((subscriber) => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in Tauri event subscriber:', error);
      }
    });
  }

  /**
   * Start listening to Rust events.
   *
   * This should be called once after initialization.
   */
  async startListening(): Promise<void> {
    if (this.initialized) {
      return;
    }

    const tauri = this.getTauri();

    // Listen for connection state changes
    const unlistenState = await tauri.event.listen<ConnectionState>(
      'quic_state_changed',
      (event) => {
        this.emit({ type: 'StateChanged', payload: event.payload });
      }
    );
    this.unlistenFns.push(unlistenState);

    // Listen for received data
    const unlistenData = await tauri.event.listen<{ channel: ChannelType; data: string }>(
      'quic_data_received',
      (event) => {
        this.emit({ type: 'DataReceived', payload: event.payload });
      }
    );
    this.unlistenFns.push(unlistenData);

    // Listen for errors
    const unlistenError = await tauri.event.listen<string>('quic_error', (event) => {
      this.emit({ type: 'Error', payload: event.payload });
    });
    this.unlistenFns.push(unlistenError);

    // Listen for peer info
    const unlistenPeer = await tauri.event.listen<{ node_id: string }>(
      'quic_peer_info',
      (event) => {
        this.emit({ type: 'PeerInfo', payload: event.payload });
      }
    );
    this.unlistenFns.push(unlistenPeer);

    this.initialized = true;
  }

  /**
   * Stop listening to Rust events and clean up.
   */
  stopListening(): void {
    this.unlistenFns.forEach((unlisten) => unlisten());
    this.unlistenFns = [];
    this.initialized = false;
  }

  /**
   * Destroy the bridge and clean up all resources.
   */
  destroy(): void {
    this.stopListening();
    this.subscribers.clear();
  }

  // ==========================================================================
  // Initialization Commands
  // ==========================================================================

  /**
   * Initialize the application state.
   *
   * This initializes the database and QUIC manager. Should be called once
   * when the application starts.
   */
  async initializeApp(request: InitRequest): Promise<InitResponse> {
    return this.invoke<InitResponse>('initialize_app', { request });
  }

  // ==========================================================================
  // QUIC Connection Commands
  // ==========================================================================

  /**
   * Connect to a peer via QUIC.
   *
   * This establishes a QUIC connection to a remote peer using their
   * node address. Supports both direct connections and relay-assisted
   * connections for NAT traversal.
   */
  async connectQuic(request: ConnectRequest): Promise<ConnectResponse> {
    return this.invoke<ConnectResponse>('connect_quic', { request });
  }

  /**
   * Disconnect from the current peer.
   *
   * This gracefully closes the QUIC connection to the remote peer,
   * closing all streams and releasing resources.
   */
  async disconnectQuic(): Promise<DisconnectResponse> {
    return this.invoke<DisconnectResponse>('disconnect_quic');
  }

  /**
   * Send data over the QUIC connection.
   *
   * The data should be provided as a Uint8Array, which will be
   * base64-encoded before sending to the Rust backend.
   */
  async sendQuicData(channel: ChannelType, data: Uint8Array): Promise<SendDataResponse> {
    const base64Data = uint8ArrayToBase64(data);
    return this.invoke<SendDataResponse>('send_quic_data', {
      request: { channel, data: base64Data },
    });
  }

  /**
   * Get the current QUIC connection status.
   */
  async getConnectionStatus(): Promise<ConnectionStatusResponse> {
    return this.invoke<ConnectionStatusResponse>('get_connection_status');
  }

  // ==========================================================================
  // Device Key Commands
  // ==========================================================================

  /**
   * Get or create the device's secret key from the keychain.
   *
   * If no key exists, a new one is generated and stored.
   */
  async getDeviceKeys(): Promise<DeviceKeysResponse> {
    return this.invoke<DeviceKeysResponse>('get_device_keys');
  }

  /**
   * Check if device keys exist in the keychain.
   */
  async hasDeviceKeys(): Promise<boolean> {
    return this.invoke<boolean>('has_device_keys');
  }

  // ==========================================================================
  // Paired Device Commands
  // ==========================================================================

  /**
   * Get all paired devices from the database.
   */
  async getPairedDevices(): Promise<PairedDevice[]> {
    return this.invoke<PairedDevice[]>('get_paired_devices');
  }

  /**
   * Get a specific paired device by ID.
   */
  async getPairedDevice(deviceId: string): Promise<PairedDevice | null> {
    return this.invoke<PairedDevice | null>('get_paired_device', { device_id: deviceId });
  }

  /**
   * Store a new paired device in the database.
   */
  async storePairedDevice(request: StorePairedDeviceRequest): Promise<PairedDevice> {
    return this.invoke<PairedDevice>('store_paired_device', { request });
  }

  /**
   * Remove a paired device from the database.
   *
   * This also removes any connection history for the device.
   */
  async removePairedDevice(deviceId: string): Promise<RemoveDeviceResponse> {
    return this.invoke<RemoveDeviceResponse>('remove_paired_device', { device_id: deviceId });
  }

  /**
   * Update the last_seen timestamp for a device.
   */
  async updateDeviceLastSeen(deviceId: string): Promise<boolean> {
    return this.invoke<boolean>('update_device_last_seen', { device_id: deviceId });
  }

  // ==========================================================================
  // Notification Commands
  // ==========================================================================

  /**
   * Show a native notification.
   *
   * On platforms that support it, notifications may include actions
   * and rich content.
   */
  async showNotification(
    title: string,
    body: string,
    icon?: string
  ): Promise<NotificationResponse> {
    const request: NotificationRequest = { title, body };
    if (icon !== undefined) {
      request.icon = icon;
    }
    return this.invoke<NotificationResponse>('show_native_notification', { request });
  }
}

// ============================================================================
// Error Classes
// ============================================================================

/**
 * Error thrown when Tauri is not available.
 */
export class TauriNotAvailableError extends Error {
  constructor() {
    super('Tauri is not available. Are you running in a Tauri environment?');
    this.name = 'TauriNotAvailableError';
  }
}

/**
 * Error thrown when a Tauri command fails.
 */
export class TauriCommandError extends Error {
  /** Error code for programmatic handling. */
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'TauriCommandError';
    this.code = code;
  }
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Convert a Uint8Array to a base64 string.
 */
export function uint8ArrayToBase64(data: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < data.length; i++) {
    binary += String.fromCharCode(data[i]);
  }
  return btoa(binary);
}

/**
 * Convert a base64 string to a Uint8Array.
 */
export function base64ToUint8Array(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

// ============================================================================
// Singleton Instance
// ============================================================================

/**
 * Singleton instance of TauriIPCBridge.
 */
let tauriIPCBridgeInstance: TauriIPCBridge | null = null;

/**
 * Get or create the singleton TauriIPCBridge instance.
 */
export function getTauriIPCBridge(): TauriIPCBridge {
  if (!tauriIPCBridgeInstance) {
    tauriIPCBridgeInstance = new TauriIPCBridge();
  }
  return tauriIPCBridgeInstance;
}

/**
 * Reset the singleton instance (useful for testing).
 */
export function resetTauriIPCBridge(): void {
  if (tauriIPCBridgeInstance) {
    tauriIPCBridgeInstance.destroy();
  }
  tauriIPCBridgeInstance = null;
}
