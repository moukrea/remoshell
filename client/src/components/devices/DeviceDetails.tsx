import { Component, Show, For, createMemo } from 'solid-js';
import {
  type ConnectionHistoryEntry,
  type DevicePlatform,
  getDeviceStore,
} from '../../stores/devices';

/**
 * Format a timestamp to a readable date/time string
 */
const formatDateTime = (timestamp: number): string => {
  const date = new Date(timestamp);
  return date.toLocaleString();
};

/**
 * Format a duration in milliseconds to a readable string
 */
const formatDuration = (ms: number): string => {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) {
    const remainingHours = hours % 24;
    return `${days}d ${remainingHours}h`;
  } else if (hours > 0) {
    const remainingMinutes = minutes % 60;
    return `${hours}h ${remainingMinutes}m`;
  } else if (minutes > 0) {
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds}s`;
  } else {
    return `${seconds}s`;
  }
};

/**
 * Get a display name for the platform
 */
const getPlatformDisplayName = (platform: DevicePlatform): string => {
  const names: Record<DevicePlatform, string> = {
    windows: 'Windows',
    macos: 'macOS',
    linux: 'Linux',
    android: 'Android',
    ios: 'iOS',
    unknown: 'Unknown',
  };
  return names[platform];
};

export interface DeviceDetailsProps {
  /** Device ID to display details for */
  deviceId: string;
  /** Called when connect action is triggered */
  onConnect?: (deviceId: string) => void;
  /** Called when back/close is triggered */
  onBack?: () => void;
  /** Additional CSS class for the container */
  class?: string;
}

export interface ConnectionHistoryItemProps {
  entry: ConnectionHistoryEntry;
  index: number;
}

/**
 * Individual connection history entry component
 */
const ConnectionHistoryItem: Component<ConnectionHistoryItemProps> = (props) => {
  const isActive = () => !props.entry.disconnectedAt;

  return (
    <div
      class={`connection-history-item ${isActive() ? 'connection-history-item--active' : ''} ${props.entry.error ? 'connection-history-item--error' : ''}`}
      data-testid={`connection-history-${props.index}`}
    >
      <div class="connection-history-item__time">
        <span class="connection-history-item__label">Connected:</span>
        <span class="connection-history-item__value" data-testid={`connection-connected-${props.index}`}>
          {formatDateTime(props.entry.connectedAt)}
        </span>
      </div>

      <Show when={props.entry.disconnectedAt}>
        <div class="connection-history-item__time">
          <span class="connection-history-item__label">Disconnected:</span>
          <span class="connection-history-item__value" data-testid={`connection-disconnected-${props.index}`}>
            {formatDateTime(props.entry.disconnectedAt!)}
          </span>
        </div>
      </Show>

      <Show when={props.entry.duration}>
        <div class="connection-history-item__duration">
          <span class="connection-history-item__label">Duration:</span>
          <span class="connection-history-item__value" data-testid={`connection-duration-${props.index}`}>
            {formatDuration(props.entry.duration!)}
          </span>
        </div>
      </Show>

      <Show when={isActive()}>
        <div class="connection-history-item__active-badge" data-testid={`connection-active-${props.index}`}>
          Currently connected
        </div>
      </Show>

      <Show when={props.entry.error}>
        <div class="connection-history-item__error" data-testid={`connection-error-${props.index}`}>
          Error: {props.entry.error}
        </div>
      </Show>
    </div>
  );
};

/**
 * Device details component showing device information and connection history
 */
const DeviceDetails: Component<DeviceDetailsProps> = (props) => {
  const store = getDeviceStore();

  const device = createMemo(() => store.getDevice(props.deviceId));

  const connectionHistory = createMemo(() => {
    const dev = device();
    if (!dev) return [];
    // Return history in reverse chronological order (most recent first)
    return [...dev.connectionHistory].reverse();
  });

  const totalConnections = createMemo(() => connectionHistory().length);

  const totalConnectionTime = createMemo(() => {
    return connectionHistory().reduce((total, entry) => {
      return total + (entry.duration ?? 0);
    }, 0);
  });

  const handleConnect = () => {
    props.onConnect?.(props.deviceId);
  };

  const handleBack = () => {
    props.onBack?.();
  };

  return (
    <div
      class={`device-details ${props.class ?? ''}`}
      data-testid="device-details"
    >
      <Show
        when={device()}
        fallback={
          <div class="device-details__not-found" data-testid="device-not-found">
            <p>Device not found</p>
            <button
              class="device-details__back-button"
              onClick={handleBack}
            >
              Back to device list
            </button>
          </div>
        }
      >
        {(dev) => (
          <>
            {/* Header with back button */}
            <div class="device-details__header">
              <button
                class="device-details__back-button"
                onClick={handleBack}
                data-testid="device-details-back"
              >
                Back
              </button>
              <h2 class="device-details__title" data-testid="device-details-name">
                {dev().name}
              </h2>
            </div>

            {/* Device Information */}
            <div class="device-details__info">
              <div class="device-details__info-row">
                <span class="device-details__label">Status:</span>
                <span
                  class={`device-details__status ${dev().status === 'online' ? 'device-details__status--online' : 'device-details__status--offline'}`}
                  data-testid="device-details-status"
                >
                  {dev().status === 'online' ? 'Online' : 'Offline'}
                </span>
              </div>

              <div class="device-details__info-row">
                <span class="device-details__label">Platform:</span>
                <span class="device-details__value" data-testid="device-details-platform">
                  {getPlatformDisplayName(dev().platform)}
                </span>
              </div>

              <div class="device-details__info-row">
                <span class="device-details__label">Paired:</span>
                <span class="device-details__value" data-testid="device-details-paired">
                  {formatDateTime(dev().pairedAt)}
                </span>
              </div>

              <div class="device-details__info-row">
                <span class="device-details__label">Last Seen:</span>
                <span class="device-details__value" data-testid="device-details-last-seen">
                  {formatDateTime(dev().lastSeen)}
                </span>
              </div>

              <div class="device-details__info-row">
                <span class="device-details__label">Total Connections:</span>
                <span class="device-details__value" data-testid="device-details-total-connections">
                  {totalConnections()}
                </span>
              </div>

              <div class="device-details__info-row">
                <span class="device-details__label">Total Connection Time:</span>
                <span class="device-details__value" data-testid="device-details-total-time">
                  {totalConnectionTime() > 0 ? formatDuration(totalConnectionTime()) : 'N/A'}
                </span>
              </div>
            </div>

            {/* Connect Button */}
            <div class="device-details__actions">
              <button
                class="device-details__connect-button"
                onClick={handleConnect}
                disabled={dev().status === 'online'}
                data-testid="device-details-connect"
              >
                {dev().status === 'online' ? 'Connected' : 'Connect'}
              </button>
            </div>

            {/* Connection History */}
            <div class="device-details__history">
              <h3 class="device-details__history-title">Connection History</h3>
              <Show
                when={connectionHistory().length > 0}
                fallback={
                  <div class="device-details__history-empty" data-testid="connection-history-empty">
                    No connection history
                  </div>
                }
              >
                <div class="device-details__history-list" data-testid="connection-history-list">
                  <For each={connectionHistory()}>
                    {(entry, index) => (
                      <ConnectionHistoryItem entry={entry} index={index()} />
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

export default DeviceDetails;
export { ConnectionHistoryItem, formatDateTime, formatDuration };
