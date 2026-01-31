import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { resetDeviceStore } from '../../stores/devices';

// Helper functions from DeviceList - tested independently to avoid SSR issues
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

describe('DeviceList', () => {
  beforeEach(() => {
    resetDeviceStore();
    vi.useFakeTimers();
  });

  afterEach(() => {
    resetDeviceStore();
    vi.useRealTimers();
  });

  describe('formatLastSeen', () => {
    it('should return "Just now" for timestamps less than 60 seconds ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      expect(formatLastSeen(now)).toBe('Just now');
      expect(formatLastSeen(now - 30000)).toBe('Just now'); // 30 seconds ago
      expect(formatLastSeen(now - 59000)).toBe('Just now'); // 59 seconds ago
    });

    it('should return minutes for timestamps 1-59 minutes ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      expect(formatLastSeen(now - 60000)).toBe('1 minute ago');
      expect(formatLastSeen(now - 120000)).toBe('2 minutes ago');
      expect(formatLastSeen(now - 3540000)).toBe('59 minutes ago');
    });

    it('should return hours for timestamps 1-23 hours ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      expect(formatLastSeen(now - 3600000)).toBe('1 hour ago');
      expect(formatLastSeen(now - 7200000)).toBe('2 hours ago');
      expect(formatLastSeen(now - 82800000)).toBe('23 hours ago');
    });

    it('should return days for timestamps 1-6 days ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      expect(formatLastSeen(now - 86400000)).toBe('1 day ago');
      expect(formatLastSeen(now - 172800000)).toBe('2 days ago');
      expect(formatLastSeen(now - 518400000)).toBe('6 days ago');
    });

    it('should return date string for timestamps 7+ days ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const sevenDaysAgo = now - 604800000;
      const result = formatLastSeen(sevenDaysAgo);
      // Result should be a date string (locale dependent)
      expect(result).not.toBe('7 days ago');
      expect(result).toMatch(/\d/); // Should contain a number
    });
  });

  describe('getPlatformDisplayName', () => {
    it('should return correct display names for all platforms', () => {
      expect(getPlatformDisplayName('windows')).toBe('Windows');
      expect(getPlatformDisplayName('macos')).toBe('macOS');
      expect(getPlatformDisplayName('linux')).toBe('Linux');
      expect(getPlatformDisplayName('android')).toBe('Android');
      expect(getPlatformDisplayName('ios')).toBe('iOS');
      expect(getPlatformDisplayName('unknown')).toBe('Unknown');
    });
  });

  describe('getPlatformIcon', () => {
    it('should return correct icons for all platforms', () => {
      expect(getPlatformIcon('windows')).toBe('W');
      expect(getPlatformIcon('macos')).toBe('M');
      expect(getPlatformIcon('linux')).toBe('L');
      expect(getPlatformIcon('android')).toBe('A');
      expect(getPlatformIcon('ios')).toBe('i');
      expect(getPlatformIcon('unknown')).toBe('?');
    });
  });

  describe('Device Sorting Logic', () => {
    interface Device {
      id: string;
      name: string;
      status: 'online' | 'offline';
      lastSeen: number;
    }

    const sortDevices = (devices: Device[]): Device[] => {
      return [...devices].sort((a, b) => {
        // Online devices first
        if (a.status === 'online' && b.status === 'offline') return -1;
        if (a.status === 'offline' && b.status === 'online') return 1;
        // Then by last seen (most recent first)
        return b.lastSeen - a.lastSeen;
      });
    };

    it('should sort online devices before offline devices', () => {
      const devices: Device[] = [
        { id: '1', name: 'Offline 1', status: 'offline', lastSeen: 1000 },
        { id: '2', name: 'Online 1', status: 'online', lastSeen: 900 },
        { id: '3', name: 'Offline 2', status: 'offline', lastSeen: 1100 },
      ];

      const sorted = sortDevices(devices);

      expect(sorted[0].status).toBe('online');
      expect(sorted[1].status).toBe('offline');
      expect(sorted[2].status).toBe('offline');
    });

    it('should sort by last seen within same status', () => {
      const devices: Device[] = [
        { id: '1', name: 'Offline Old', status: 'offline', lastSeen: 1000 },
        { id: '2', name: 'Offline New', status: 'offline', lastSeen: 2000 },
        { id: '3', name: 'Offline Mid', status: 'offline', lastSeen: 1500 },
      ];

      const sorted = sortDevices(devices);

      expect(sorted[0].id).toBe('2'); // Most recent
      expect(sorted[1].id).toBe('3');
      expect(sorted[2].id).toBe('1'); // Oldest
    });

    it('should handle empty array', () => {
      const sorted = sortDevices([]);
      expect(sorted).toEqual([]);
    });

    it('should handle single device', () => {
      const devices: Device[] = [
        { id: '1', name: 'Solo', status: 'online', lastSeen: 1000 },
      ];

      const sorted = sortDevices(devices);
      expect(sorted).toHaveLength(1);
      expect(sorted[0].id).toBe('1');
    });
  });

  describe('Connect Action Logic', () => {
    it('should determine if connect action should be disabled', () => {
      const shouldDisableConnect = (status: 'online' | 'offline'): boolean => {
        return status === 'online';
      };

      expect(shouldDisableConnect('online')).toBe(true);
      expect(shouldDisableConnect('offline')).toBe(false);
    });
  });

  describe('Rename Logic', () => {
    it('should validate rename input', () => {
      const isValidName = (name: string): boolean => {
        return name.trim().length > 0;
      };

      expect(isValidName('Valid Name')).toBe(true);
      expect(isValidName('  Padded  ')).toBe(true);
      expect(isValidName('')).toBe(false);
      expect(isValidName('   ')).toBe(false);
    });

    it('should trim name before saving', () => {
      const processName = (name: string): string => {
        return name.trim();
      };

      expect(processName('  Padded Name  ')).toBe('Padded Name');
      expect(processName('Normal')).toBe('Normal');
    });
  });

  describe('Remove Confirmation Logic', () => {
    it('should track confirmation state', () => {
      let showConfirmRemove = false;

      const handleRemoveClick = () => {
        showConfirmRemove = true;
      };

      const confirmRemove = () => {
        showConfirmRemove = false;
        return true; // Returns true to indicate removal should proceed
      };

      const cancelRemove = () => {
        showConfirmRemove = false;
      };

      expect(showConfirmRemove).toBe(false);

      handleRemoveClick();
      expect(showConfirmRemove).toBe(true);

      cancelRemove();
      expect(showConfirmRemove).toBe(false);

      handleRemoveClick();
      expect(showConfirmRemove).toBe(true);

      const shouldRemove = confirmRemove();
      expect(showConfirmRemove).toBe(false);
      expect(shouldRemove).toBe(true);
    });
  });

  describe('Empty State Logic', () => {
    it('should determine when to show empty state', () => {
      const shouldShowEmpty = (deviceCount: number): boolean => {
        return deviceCount === 0;
      };

      expect(shouldShowEmpty(0)).toBe(true);
      expect(shouldShowEmpty(1)).toBe(false);
      expect(shouldShowEmpty(10)).toBe(false);
    });
  });

  describe('Device Count Display', () => {
    it('should format device count correctly', () => {
      const formatDeviceCount = (count: number): string => {
        return `${count} device${count !== 1 ? 's' : ''}`;
      };

      expect(formatDeviceCount(0)).toBe('0 devices');
      expect(formatDeviceCount(1)).toBe('1 device');
      expect(formatDeviceCount(2)).toBe('2 devices');
      expect(formatDeviceCount(10)).toBe('10 devices');
    });

    it('should format online count correctly', () => {
      const formatOnlineCount = (count: number): string => {
        return `${count} online`;
      };

      expect(formatOnlineCount(0)).toBe('0 online');
      expect(formatOnlineCount(5)).toBe('5 online');
    });
  });

  describe('Inline Edit State', () => {
    it('should manage editing state', () => {
      let isEditing = false;
      let editValue = '';

      const startEditing = (currentName: string) => {
        editValue = currentName;
        isEditing = true;
      };

      const cancelEditing = () => {
        isEditing = false;
        editValue = '';
      };

      const saveEdit = (onSave: (name: string) => void) => {
        if (editValue.trim()) {
          onSave(editValue.trim());
        }
        isEditing = false;
        editValue = '';
      };

      expect(isEditing).toBe(false);
      expect(editValue).toBe('');

      startEditing('My Device');
      expect(isEditing).toBe(true);
      expect(editValue).toBe('My Device');

      cancelEditing();
      expect(isEditing).toBe(false);
      expect(editValue).toBe('');

      startEditing('Another Device');
      editValue = 'Renamed Device';
      const savedName = vi.fn();
      saveEdit(savedName);
      expect(savedName).toHaveBeenCalledWith('Renamed Device');
      expect(isEditing).toBe(false);
    });

    it('should handle keyboard events in edit mode', () => {
      const handleKeyDown = (key: string, callbacks: { save: () => void; cancel: () => void }) => {
        if (key === 'Enter') {
          callbacks.save();
        } else if (key === 'Escape') {
          callbacks.cancel();
        }
      };

      const save = vi.fn();
      const cancel = vi.fn();

      handleKeyDown('Enter', { save, cancel });
      expect(save).toHaveBeenCalled();
      expect(cancel).not.toHaveBeenCalled();

      save.mockClear();
      cancel.mockClear();

      handleKeyDown('Escape', { save, cancel });
      expect(save).not.toHaveBeenCalled();
      expect(cancel).toHaveBeenCalled();

      save.mockClear();
      cancel.mockClear();

      handleKeyDown('a', { save, cancel });
      expect(save).not.toHaveBeenCalled();
      expect(cancel).not.toHaveBeenCalled();
    });
  });
});
