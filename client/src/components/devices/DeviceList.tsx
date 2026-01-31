import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import {
  type Device,
  type DeviceStatus,
  type DevicePlatform,
  getDeviceStore,
} from '../../stores/devices';

/**
 * Format a timestamp to a human-readable "last seen" string
 */
const formatLastSeen = (timestamp: number): string => {
  const now = Date.now();
  const diff = now - timestamp;

  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (seconds < 60) {
    return 'Just now';
  } else if (minutes < 60) {
    return `${minutes} minute${minutes !== 1 ? 's' : ''} ago`;
  } else if (hours < 24) {
    return `${hours} hour${hours !== 1 ? 's' : ''} ago`;
  } else if (days < 7) {
    return `${days} day${days !== 1 ? 's' : ''} ago`;
  } else {
    const date = new Date(timestamp);
    return date.toLocaleDateString();
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

/**
 * Get an icon/emoji for the platform (can be replaced with actual icons)
 */
const getPlatformIcon = (platform: DevicePlatform): string => {
  const icons: Record<DevicePlatform, string> = {
    windows: 'W',
    macos: 'M',
    linux: 'L',
    android: 'A',
    ios: 'i',
    unknown: '?',
  };
  return icons[platform];
};

export interface DeviceListProps {
  /** Called when connect action is triggered */
  onConnect?: (deviceId: string) => void;
  /** Called when a device is selected for details */
  onSelectDevice?: (deviceId: string) => void;
  /** Additional CSS class for the container */
  class?: string;
}

export interface DeviceListItemProps {
  device: Device;
  onConnect?: (deviceId: string) => void;
  onRename?: (deviceId: string, newName: string) => void;
  onRemove?: (deviceId: string) => void;
  onSelect?: (deviceId: string) => void;
}

/**
 * Individual device list item component
 */
const DeviceListItem: Component<DeviceListItemProps> = (props) => {
  const [isEditing, setIsEditing] = createSignal(false);
  const [editValue, setEditValue] = createSignal('');
  const [showConfirmRemove, setShowConfirmRemove] = createSignal(false);

  let inputRef: HTMLInputElement | undefined;

  const startEditing = () => {
    setEditValue(props.device.name);
    setIsEditing(true);
    // Focus the input on next tick
    setTimeout(() => inputRef?.focus(), 0);
  };

  const cancelEditing = () => {
    setIsEditing(false);
    setEditValue('');
  };

  const saveEdit = () => {
    const newName = editValue().trim();
    if (newName && newName !== props.device.name) {
      props.onRename?.(props.device.id, newName);
    }
    setIsEditing(false);
    setEditValue('');
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter') {
      saveEdit();
    } else if (e.key === 'Escape') {
      cancelEditing();
    }
  };

  const handleConnect = (e: MouseEvent) => {
    e.stopPropagation();
    props.onConnect?.(props.device.id);
  };

  const handleRemoveClick = (e: MouseEvent) => {
    e.stopPropagation();
    setShowConfirmRemove(true);
  };

  const confirmRemove = (e: MouseEvent) => {
    e.stopPropagation();
    props.onRemove?.(props.device.id);
    setShowConfirmRemove(false);
  };

  const cancelRemove = (e: MouseEvent) => {
    e.stopPropagation();
    setShowConfirmRemove(false);
  };

  const handleItemClick = () => {
    if (!isEditing() && !showConfirmRemove()) {
      props.onSelect?.(props.device.id);
    }
  };

  return (
    <div
      class={`device-list-item ${props.device.status === 'online' ? 'device-list-item--online' : 'device-list-item--offline'}`}
      data-testid={`device-item-${props.device.id}`}
      onClick={handleItemClick}
      role="listitem"
      aria-label={`${props.device.name}, ${props.device.status === 'online' ? 'online' : 'offline'}, ${getPlatformDisplayName(props.device.platform)}`}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleItemClick();
        }
      }}
    >
      {/* Status Indicator */}
      <div
        class={`device-list-item__status ${props.device.status === 'online' ? 'device-list-item__status--online' : 'device-list-item__status--offline'}`}
        data-testid={`device-status-${props.device.id}`}
        title={props.device.status === 'online' ? 'Online' : 'Offline'}
        aria-label={props.device.status === 'online' ? 'Online' : 'Offline'}
      />

      {/* Platform Icon */}
      <div
        class="device-list-item__platform"
        title={getPlatformDisplayName(props.device.platform)}
        data-testid={`device-platform-${props.device.id}`}
      >
        {getPlatformIcon(props.device.platform)}
      </div>

      {/* Device Info */}
      <div class="device-list-item__info">
        <Show
          when={!isEditing()}
          fallback={
            <input
              ref={inputRef}
              type="text"
              class="device-list-item__name-input"
              value={editValue()}
              onInput={(e) => setEditValue(e.currentTarget.value)}
              onKeyDown={handleKeyDown}
              onBlur={saveEdit}
              data-testid={`device-name-input-${props.device.id}`}
              onClick={(e) => e.stopPropagation()}
              aria-label="Device name"
            />
          }
        >
          <span
            class="device-list-item__name"
            data-testid={`device-name-${props.device.id}`}
            onDblClick={(e) => { e.stopPropagation(); startEditing(); }}
          >
            {props.device.name}
          </span>
        </Show>
        <span
          class="device-list-item__last-seen"
          data-testid={`device-last-seen-${props.device.id}`}
        >
          {props.device.status === 'online' ? 'Connected' : formatLastSeen(props.device.lastSeen)}
        </span>
      </div>

      {/* Actions */}
      <div class="device-list-item__actions">
        <Show when={!showConfirmRemove()}>
          <button
            class="device-list-item__action device-list-item__action--connect"
            onClick={handleConnect}
            disabled={props.device.status === 'online'}
            title={props.device.status === 'online' ? 'Already connected' : 'Connect'}
            data-testid={`device-connect-${props.device.id}`}
          >
            Connect
          </button>
          <button
            class="device-list-item__action device-list-item__action--rename"
            onClick={(e) => { e.stopPropagation(); startEditing(); }}
            title="Rename device"
            data-testid={`device-rename-${props.device.id}`}
          >
            Rename
          </button>
          <button
            class="device-list-item__action device-list-item__action--remove"
            onClick={handleRemoveClick}
            title="Remove device"
            data-testid={`device-remove-${props.device.id}`}
          >
            Remove
          </button>
        </Show>

        {/* Confirmation Dialog (inline) */}
        <Show when={showConfirmRemove()}>
          <div class="device-list-item__confirm" data-testid={`device-confirm-${props.device.id}`}>
            <span class="device-list-item__confirm-text">Remove?</span>
            <button
              class="device-list-item__action device-list-item__action--confirm-yes"
              onClick={confirmRemove}
              data-testid={`device-confirm-yes-${props.device.id}`}
            >
              Yes
            </button>
            <button
              class="device-list-item__action device-list-item__action--confirm-no"
              onClick={cancelRemove}
              data-testid={`device-confirm-no-${props.device.id}`}
            >
              No
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
};

/**
 * Device list component displaying all paired devices
 */
const DeviceList: Component<DeviceListProps> = (props) => {
  const store = getDeviceStore();

  // Get devices sorted by status (online first) then by last seen
  const sortedDevices = createMemo(() => {
    const devices = store.getAllDevices();
    return devices.sort((a, b) => {
      // Online devices first
      if (a.status === 'online' && b.status === 'offline') return -1;
      if (a.status === 'offline' && b.status === 'online') return 1;
      // Then by last seen (most recent first)
      return b.lastSeen - a.lastSeen;
    });
  });

  const handleConnect = (deviceId: string) => {
    props.onConnect?.(deviceId);
  };

  const handleRename = (deviceId: string, newName: string) => {
    store.renameDevice(deviceId, newName);
  };

  const handleRemove = (deviceId: string) => {
    store.removeDevice(deviceId);
  };

  const handleSelect = (deviceId: string) => {
    props.onSelectDevice?.(deviceId);
  };

  return (
    <div
      class={`device-list ${props.class ?? ''}`}
      data-testid="device-list"
      role="list"
      aria-label="Paired devices"
    >
      <Show
        when={sortedDevices().length > 0}
        fallback={
          <div class="device-list__empty" data-testid="device-list-empty">
            <p>No paired devices</p>
            <p class="device-list__empty-hint">
              Scan a QR code to pair a new device
            </p>
          </div>
        }
      >
        <div class="device-list__header">
          <span class="device-list__count" data-testid="device-count">
            {sortedDevices().length} device{sortedDevices().length !== 1 ? 's' : ''}
          </span>
          <span class="device-list__online-count" data-testid="device-online-count">
            {store.getOnlineDevices().length} online
          </span>
        </div>
        <div class="device-list__items">
          <For each={sortedDevices()}>
            {(device) => (
              <DeviceListItem
                device={device}
                onConnect={handleConnect}
                onRename={handleRename}
                onRemove={handleRemove}
                onSelect={handleSelect}
              />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default DeviceList;
export { DeviceListItem, formatLastSeen, getPlatformDisplayName, getPlatformIcon };
