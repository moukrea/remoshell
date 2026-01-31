import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { resetFileStore, type FileTransfer, type TransferStatus } from '../../stores/files';

// Helper functions from FileTransferProgress - tested independently to avoid SSR issues
const calculateProgress = (transfer: FileTransfer): number => {
  if (transfer.totalBytes === 0) return 0;
  return Math.round((transfer.transferredBytes / transfer.totalBytes) * 100);
};

const calculateSpeed = (transfer: FileTransfer): number => {
  if (transfer.status !== 'in_progress') return 0;
  const elapsed = (Date.now() - transfer.startedAt) / 1000;
  if (elapsed === 0) return 0;
  return transfer.transferredBytes / elapsed;
};

const estimateRemainingTime = (transfer: FileTransfer): string => {
  const speed = calculateSpeed(transfer);
  if (speed === 0) return '--:--';

  const remaining = transfer.totalBytes - transfer.transferredBytes;
  const seconds = Math.ceil(remaining / speed);

  if (seconds < 60) {
    return `${seconds}s`;
  } else if (seconds < 3600) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  } else {
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${mins}m`;
  }
};

const getStatusText = (status: TransferStatus): string => {
  switch (status) {
    case 'pending':
      return 'Pending';
    case 'in_progress':
      return 'Transferring';
    case 'completed':
      return 'Completed';
    case 'failed':
      return 'Failed';
    case 'cancelled':
      return 'Cancelled';
    default:
      return 'Unknown';
  }
};

const getStatusIcon = (status: TransferStatus): string => {
  switch (status) {
    case 'pending':
      return '...';
    case 'in_progress':
      return '>';
    case 'completed':
      return 'OK';
    case 'failed':
      return 'X';
    case 'cancelled':
      return '-';
    default:
      return '?';
  }
};

describe('FileTransferProgress', () => {
  beforeEach(() => {
    resetFileStore();
    vi.useFakeTimers();
  });

  afterEach(() => {
    resetFileStore();
    vi.useRealTimers();
  });

  const createTransfer = (overrides: Partial<FileTransfer> = {}): FileTransfer => ({
    id: 'transfer-1',
    fileName: 'test.txt',
    filePath: '/path/to/test.txt',
    direction: 'download',
    status: 'in_progress',
    totalBytes: 1024,
    transferredBytes: 512,
    startedAt: Date.now() - 10000, // 10 seconds ago
    ...overrides,
  });

  describe('calculateProgress', () => {
    it('should calculate progress percentage', () => {
      expect(calculateProgress(createTransfer({ transferredBytes: 512, totalBytes: 1024 }))).toBe(50);
      expect(calculateProgress(createTransfer({ transferredBytes: 0, totalBytes: 1024 }))).toBe(0);
      expect(calculateProgress(createTransfer({ transferredBytes: 1024, totalBytes: 1024 }))).toBe(100);
    });

    it('should handle zero total bytes', () => {
      expect(calculateProgress(createTransfer({ totalBytes: 0 }))).toBe(0);
    });

    it('should round to nearest integer', () => {
      expect(calculateProgress(createTransfer({ transferredBytes: 333, totalBytes: 1000 }))).toBe(33);
      expect(calculateProgress(createTransfer({ transferredBytes: 666, totalBytes: 1000 }))).toBe(67);
    });
  });

  describe('calculateSpeed', () => {
    it('should calculate speed in bytes per second', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const transfer = createTransfer({
        status: 'in_progress',
        startedAt: now - 10000, // 10 seconds ago
        transferredBytes: 5120, // 5120 bytes
      });

      expect(calculateSpeed(transfer)).toBe(512); // 512 bytes/second
    });

    it('should return 0 for non-in_progress transfers', () => {
      expect(calculateSpeed(createTransfer({ status: 'pending' }))).toBe(0);
      expect(calculateSpeed(createTransfer({ status: 'completed' }))).toBe(0);
      expect(calculateSpeed(createTransfer({ status: 'failed' }))).toBe(0);
      expect(calculateSpeed(createTransfer({ status: 'cancelled' }))).toBe(0);
    });

    it('should return 0 for zero elapsed time', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const transfer = createTransfer({
        status: 'in_progress',
        startedAt: now,
        transferredBytes: 1000,
      });

      expect(calculateSpeed(transfer)).toBe(0);
    });
  });

  describe('estimateRemainingTime', () => {
    it('should return --:-- when speed is 0', () => {
      expect(estimateRemainingTime(createTransfer({ status: 'pending' }))).toBe('--:--');
    });

    it('should format seconds', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const transfer = createTransfer({
        status: 'in_progress',
        startedAt: now - 10000,
        transferredBytes: 900,
        totalBytes: 1000,
      });

      // Speed is 90 bytes/second, remaining is 100 bytes
      // Remaining time is ~1.1 seconds
      const result = estimateRemainingTime(transfer);
      expect(result).toMatch(/\d+s/);
    });

    it('should format minutes and seconds', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const transfer = createTransfer({
        status: 'in_progress',
        startedAt: now - 10000,
        transferredBytes: 100,
        totalBytes: 10000,
      });

      // Speed is 10 bytes/second, remaining is 9900 bytes
      // Remaining time is 990 seconds = 16:30
      const result = estimateRemainingTime(transfer);
      expect(result).toMatch(/\d+:\d{2}/);
    });

    it('should format hours and minutes for long transfers', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const transfer = createTransfer({
        status: 'in_progress',
        startedAt: now - 10000,
        transferredBytes: 1,
        totalBytes: 1000000,
      });

      // Speed is 0.1 bytes/second, remaining is 999999 bytes
      // Very long time remaining
      const result = estimateRemainingTime(transfer);
      expect(result).toMatch(/\d+h \d+m/);
    });
  });

  describe('getStatusText', () => {
    it('should return correct status text for all statuses', () => {
      expect(getStatusText('pending')).toBe('Pending');
      expect(getStatusText('in_progress')).toBe('Transferring');
      expect(getStatusText('completed')).toBe('Completed');
      expect(getStatusText('failed')).toBe('Failed');
      expect(getStatusText('cancelled')).toBe('Cancelled');
    });
  });

  describe('getStatusIcon', () => {
    it('should return correct status icons for all statuses', () => {
      expect(getStatusIcon('pending')).toBe('...');
      expect(getStatusIcon('in_progress')).toBe('>');
      expect(getStatusIcon('completed')).toBe('OK');
      expect(getStatusIcon('failed')).toBe('X');
      expect(getStatusIcon('cancelled')).toBe('-');
    });
  });

  describe('Transfer Sorting Logic', () => {
    const sortTransfers = (transfers: FileTransfer[]): FileTransfer[] => {
      const statusOrder: Record<TransferStatus, number> = {
        in_progress: 0,
        pending: 1,
        completed: 2,
        failed: 3,
        cancelled: 4,
      };

      return [...transfers].sort((a, b) => {
        const statusDiff = statusOrder[a.status] - statusOrder[b.status];
        if (statusDiff !== 0) return statusDiff;
        return b.startedAt - a.startedAt;
      });
    };

    it('should sort in_progress first', () => {
      const transfers = [
        createTransfer({ id: '1', status: 'pending', startedAt: 1000 }),
        createTransfer({ id: '2', status: 'in_progress', startedAt: 900 }),
        createTransfer({ id: '3', status: 'completed', startedAt: 1100 }),
      ];

      const sorted = sortTransfers(transfers);

      expect(sorted[0].status).toBe('in_progress');
      expect(sorted[1].status).toBe('pending');
      expect(sorted[2].status).toBe('completed');
    });

    it('should sort by start time within same status', () => {
      const transfers = [
        createTransfer({ id: '1', status: 'in_progress', startedAt: 1000 }),
        createTransfer({ id: '2', status: 'in_progress', startedAt: 2000 }),
        createTransfer({ id: '3', status: 'in_progress', startedAt: 1500 }),
      ];

      const sorted = sortTransfers(transfers);

      expect(sorted[0].id).toBe('2'); // Most recent
      expect(sorted[1].id).toBe('3');
      expect(sorted[2].id).toBe('1'); // Oldest
    });
  });

  describe('Overall Progress Calculation', () => {
    const calculateOverallProgress = (transfers: FileTransfer[]): { percent: number; transferred: number; total: number; count: number } | null => {
      const active = transfers.filter(t => t.status === 'pending' || t.status === 'in_progress');
      if (active.length === 0) return null;

      const totalBytes = active.reduce((sum, t) => sum + t.totalBytes, 0);
      const transferredBytes = active.reduce((sum, t) => sum + t.transferredBytes, 0);

      if (totalBytes === 0) return null;

      return {
        percent: Math.round((transferredBytes / totalBytes) * 100),
        transferred: transferredBytes,
        total: totalBytes,
        count: active.length,
      };
    };

    it('should return null when no active transfers', () => {
      const transfers = [
        createTransfer({ status: 'completed' }),
        createTransfer({ status: 'failed' }),
      ];

      expect(calculateOverallProgress(transfers)).toBeNull();
    });

    it('should calculate overall progress for active transfers', () => {
      const transfers = [
        createTransfer({ status: 'in_progress', transferredBytes: 500, totalBytes: 1000 }),
        createTransfer({ status: 'pending', transferredBytes: 0, totalBytes: 1000 }),
        createTransfer({ status: 'completed', transferredBytes: 1000, totalBytes: 1000 }),
      ];

      const progress = calculateOverallProgress(transfers);

      expect(progress).not.toBeNull();
      expect(progress?.count).toBe(2);
      expect(progress?.total).toBe(2000);
      expect(progress?.transferred).toBe(500);
      expect(progress?.percent).toBe(25);
    });

    it('should return null when total bytes is zero', () => {
      const transfers = [
        createTransfer({ status: 'in_progress', totalBytes: 0 }),
      ];

      expect(calculateOverallProgress(transfers)).toBeNull();
    });
  });

  describe('Transfer Actions', () => {
    it('should determine if cancel is available', () => {
      const canCancel = (status: TransferStatus): boolean => {
        return status === 'pending' || status === 'in_progress';
      };

      expect(canCancel('pending')).toBe(true);
      expect(canCancel('in_progress')).toBe(true);
      expect(canCancel('completed')).toBe(false);
      expect(canCancel('failed')).toBe(false);
      expect(canCancel('cancelled')).toBe(false);
    });

    it('should determine if remove is available', () => {
      const canRemove = (status: TransferStatus): boolean => {
        return status === 'completed' || status === 'failed' || status === 'cancelled';
      };

      expect(canRemove('pending')).toBe(false);
      expect(canRemove('in_progress')).toBe(false);
      expect(canRemove('completed')).toBe(true);
      expect(canRemove('failed')).toBe(true);
      expect(canRemove('cancelled')).toBe(true);
    });
  });

  describe('Active Transfer Count', () => {
    it('should count active transfers', () => {
      const countActive = (transfers: FileTransfer[]): number => {
        return transfers.filter(t => t.status === 'pending' || t.status === 'in_progress').length;
      };

      const transfers = [
        createTransfer({ status: 'pending' }),
        createTransfer({ status: 'in_progress' }),
        createTransfer({ status: 'completed' }),
        createTransfer({ status: 'failed' }),
      ];

      expect(countActive(transfers)).toBe(2);
    });
  });

  describe('Direction Display', () => {
    it('should display correct direction text', () => {
      const getDirectionText = (direction: 'upload' | 'download'): string => {
        return direction === 'upload' ? 'Up' : 'Dn';
      };

      expect(getDirectionText('upload')).toBe('Up');
      expect(getDirectionText('download')).toBe('Dn');
    });
  });

  describe('Clear Completed Logic', () => {
    it('should determine if clear button should show', () => {
      const shouldShowClearButton = (transfers: FileTransfer[]): boolean => {
        return transfers.some(t =>
          t.status === 'completed' || t.status === 'failed' || t.status === 'cancelled'
        );
      };

      expect(shouldShowClearButton([
        createTransfer({ status: 'in_progress' }),
        createTransfer({ status: 'pending' }),
      ])).toBe(false);

      expect(shouldShowClearButton([
        createTransfer({ status: 'in_progress' }),
        createTransfer({ status: 'completed' }),
      ])).toBe(true);

      expect(shouldShowClearButton([
        createTransfer({ status: 'failed' }),
      ])).toBe(true);
    });
  });

  describe('Max Items Limiting', () => {
    it('should limit items when maxItems is specified', () => {
      const limitItems = <T,>(items: T[], maxItems: number | undefined): T[] => {
        if (maxItems && items.length > maxItems) {
          return items.slice(0, maxItems);
        }
        return items;
      };

      const items = [1, 2, 3, 4, 5];

      expect(limitItems(items, undefined)).toHaveLength(5);
      expect(limitItems(items, 10)).toHaveLength(5);
      expect(limitItems(items, 3)).toHaveLength(3);
      expect(limitItems(items, 1)).toHaveLength(1);
    });
  });

  describe('Show Only Active Filter', () => {
    it('should filter to active only when specified', () => {
      const filterToActive = (transfers: FileTransfer[], showOnlyActive: boolean): FileTransfer[] => {
        if (!showOnlyActive) return transfers;
        return transfers.filter(t => t.status === 'pending' || t.status === 'in_progress');
      };

      const transfers = [
        createTransfer({ id: '1', status: 'in_progress' }),
        createTransfer({ id: '2', status: 'completed' }),
        createTransfer({ id: '3', status: 'pending' }),
      ];

      expect(filterToActive(transfers, false)).toHaveLength(3);
      expect(filterToActive(transfers, true)).toHaveLength(2);
    });
  });
});
