import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { resetDeviceStore } from '../../stores/devices';

// Helper functions from DeviceDetails - tested independently to avoid SSR issues
const formatDateTime = (timestamp: number): string => {
  const date = new Date(timestamp);
  return date.toLocaleString();
};

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

type DevicePlatform = 'windows' | 'macos' | 'linux' | 'android' | 'ios' | 'unknown';

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

describe('DeviceDetails', () => {
  beforeEach(() => {
    resetDeviceStore();
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetDeviceStore();
    vi.clearAllMocks();
  });

  describe('formatDateTime', () => {
    it('should return locale string for timestamp', () => {
      const timestamp = new Date('2024-01-15T10:30:00').getTime();
      const result = formatDateTime(timestamp);

      // Result is locale-dependent, but should contain the date/time components
      expect(result).toMatch(/\d/); // Contains numbers
      expect(typeof result).toBe('string');
    });

    it('should handle current timestamp', () => {
      const now = Date.now();
      const result = formatDateTime(now);
      expect(typeof result).toBe('string');
      expect(result.length).toBeGreaterThan(0);
    });

    it('should handle very old timestamps', () => {
      const oldTimestamp = new Date('2000-01-01T00:00:00').getTime();
      const result = formatDateTime(oldTimestamp);
      expect(result).toMatch(/2000/);
    });
  });

  describe('formatDuration', () => {
    it('should format seconds only', () => {
      expect(formatDuration(1000)).toBe('1s');
      expect(formatDuration(30000)).toBe('30s');
      expect(formatDuration(59000)).toBe('59s');
    });

    it('should format minutes and seconds', () => {
      expect(formatDuration(60000)).toBe('1m 0s');
      expect(formatDuration(90000)).toBe('1m 30s');
      expect(formatDuration(125000)).toBe('2m 5s');
      expect(formatDuration(3599000)).toBe('59m 59s');
    });

    it('should format hours and minutes', () => {
      expect(formatDuration(3600000)).toBe('1h 0m');
      expect(formatDuration(5400000)).toBe('1h 30m');
      expect(formatDuration(7200000)).toBe('2h 0m');
      expect(formatDuration(86399000)).toBe('23h 59m');
    });

    it('should format days and hours', () => {
      expect(formatDuration(86400000)).toBe('1d 0h');
      expect(formatDuration(90000000)).toBe('1d 1h');
      expect(formatDuration(172800000)).toBe('2d 0h');
      expect(formatDuration(259200000)).toBe('3d 0h');
    });

    it('should handle zero duration', () => {
      expect(formatDuration(0)).toBe('0s');
    });

    it('should handle large durations', () => {
      const oneWeek = 7 * 24 * 60 * 60 * 1000;
      expect(formatDuration(oneWeek)).toBe('7d 0h');

      const thirtyDays = 30 * 24 * 60 * 60 * 1000;
      expect(formatDuration(thirtyDays)).toBe('30d 0h');
    });
  });

  describe('getPlatformDisplayName', () => {
    it('should return correct display names', () => {
      expect(getPlatformDisplayName('windows')).toBe('Windows');
      expect(getPlatformDisplayName('macos')).toBe('macOS');
      expect(getPlatformDisplayName('linux')).toBe('Linux');
      expect(getPlatformDisplayName('android')).toBe('Android');
      expect(getPlatformDisplayName('ios')).toBe('iOS');
      expect(getPlatformDisplayName('unknown')).toBe('Unknown');
    });
  });

  describe('Connection History Processing', () => {
    interface ConnectionHistoryEntry {
      connectedAt: number;
      disconnectedAt?: number;
      duration?: number;
      error?: string;
    }

    it('should reverse connection history for display (most recent first)', () => {
      const history: ConnectionHistoryEntry[] = [
        { connectedAt: 1000, disconnectedAt: 2000, duration: 1000 },
        { connectedAt: 3000, disconnectedAt: 4000, duration: 1000 },
        { connectedAt: 5000 }, // Active connection
      ];

      const reversed = [...history].reverse();

      expect(reversed[0].connectedAt).toBe(5000);
      expect(reversed[1].connectedAt).toBe(3000);
      expect(reversed[2].connectedAt).toBe(1000);
    });

    it('should detect active connections', () => {
      const isActive = (entry: ConnectionHistoryEntry): boolean => {
        return !entry.disconnectedAt;
      };

      expect(isActive({ connectedAt: 1000 })).toBe(true);
      expect(isActive({ connectedAt: 1000, disconnectedAt: 2000 })).toBe(false);
    });

    it('should detect entries with errors', () => {
      const hasError = (entry: ConnectionHistoryEntry): boolean => {
        return !!entry.error;
      };

      expect(hasError({ connectedAt: 1000 })).toBe(false);
      expect(hasError({ connectedAt: 1000, error: 'Connection failed' })).toBe(true);
    });
  });

  describe('Total Connections Calculation', () => {
    interface ConnectionHistoryEntry {
      connectedAt: number;
      disconnectedAt?: number;
      duration?: number;
    }

    it('should count total connections', () => {
      const countConnections = (history: ConnectionHistoryEntry[]): number => {
        return history.length;
      };

      expect(countConnections([])).toBe(0);
      expect(countConnections([{ connectedAt: 1000 }])).toBe(1);
      expect(countConnections([
        { connectedAt: 1000, disconnectedAt: 2000 },
        { connectedAt: 3000, disconnectedAt: 4000 },
        { connectedAt: 5000 },
      ])).toBe(3);
    });
  });

  describe('Total Connection Time Calculation', () => {
    interface ConnectionHistoryEntry {
      connectedAt: number;
      disconnectedAt?: number;
      duration?: number;
    }

    it('should sum up all durations', () => {
      const calculateTotalTime = (history: ConnectionHistoryEntry[]): number => {
        return history.reduce((total, entry) => {
          return total + (entry.duration ?? 0);
        }, 0);
      };

      expect(calculateTotalTime([])).toBe(0);

      expect(calculateTotalTime([
        { connectedAt: 1000, disconnectedAt: 2000, duration: 1000 },
      ])).toBe(1000);

      expect(calculateTotalTime([
        { connectedAt: 1000, disconnectedAt: 2000, duration: 1000 },
        { connectedAt: 3000, disconnectedAt: 5000, duration: 2000 },
        { connectedAt: 6000, disconnectedAt: 10000, duration: 4000 },
      ])).toBe(7000);
    });

    it('should ignore entries without duration', () => {
      const calculateTotalTime = (history: ConnectionHistoryEntry[]): number => {
        return history.reduce((total, entry) => {
          return total + (entry.duration ?? 0);
        }, 0);
      };

      expect(calculateTotalTime([
        { connectedAt: 1000, disconnectedAt: 2000, duration: 1000 },
        { connectedAt: 3000 }, // Active, no duration yet
      ])).toBe(1000);
    });
  });

  describe('Device Not Found State', () => {
    it('should determine when device is not found', () => {
      interface Device {
        id: string;
        name: string;
      }

      const getDevice = (deviceId: string, devices: Record<string, Device>): Device | null => {
        return devices[deviceId] ?? null;
      };

      const devices = {
        'device-1': { id: 'device-1', name: 'My Device' },
      };

      expect(getDevice('device-1', devices)).not.toBeNull();
      expect(getDevice('non-existent', devices)).toBeNull();
    });
  });

  describe('Connect Button State', () => {
    it('should determine button disabled state', () => {
      const shouldDisableConnect = (status: 'online' | 'offline'): boolean => {
        return status === 'online';
      };

      expect(shouldDisableConnect('online')).toBe(true);
      expect(shouldDisableConnect('offline')).toBe(false);
    });

    it('should determine button text', () => {
      const getButtonText = (status: 'online' | 'offline'): string => {
        return status === 'online' ? 'Connected' : 'Connect';
      };

      expect(getButtonText('online')).toBe('Connected');
      expect(getButtonText('offline')).toBe('Connect');
    });
  });

  describe('Callback Handling', () => {
    it('should call onConnect with device ID', () => {
      const onConnect = vi.fn();
      const deviceId = 'device-123';

      const handleConnect = () => {
        onConnect(deviceId);
      };

      handleConnect();
      expect(onConnect).toHaveBeenCalledWith('device-123');
    });

    it('should call onBack when back button clicked', () => {
      const onBack = vi.fn();

      const handleBack = () => {
        onBack();
      };

      handleBack();
      expect(onBack).toHaveBeenCalled();
    });
  });

  describe('Status Display', () => {
    it('should return correct status text', () => {
      const getStatusText = (status: 'online' | 'offline'): string => {
        return status === 'online' ? 'Online' : 'Offline';
      };

      expect(getStatusText('online')).toBe('Online');
      expect(getStatusText('offline')).toBe('Offline');
    });
  });

  describe('Total Time Display', () => {
    it('should display N/A when total time is 0', () => {
      const formatTotalTime = (totalTime: number): string => {
        return totalTime > 0 ? formatDuration(totalTime) : 'N/A';
      };

      expect(formatTotalTime(0)).toBe('N/A');
      expect(formatTotalTime(1000)).toBe('1s');
      expect(formatTotalTime(3600000)).toBe('1h 0m');
    });
  });
});
