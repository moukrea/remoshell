import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createFileStore,
  getFileStore,
  resetFileStore,
  formatBytes,
  formatPermissions,
  type FileStore,
  type FileEntry,
  type FilePermissions,
} from './files';

describe('File Store', () => {
  let store: FileStore;

  beforeEach(() => {
    resetFileStore();
    store = createFileStore();
  });

  afterEach(() => {
    resetFileStore();
  });

  describe('Initial State', () => {
    it('should have correct initial state', () => {
      expect(store.state.currentPath).toBe('/');
      expect(store.state.entries).toEqual([]);
      expect(store.state.selectedPaths.size).toBe(0);
      expect(store.state.transfers).toEqual({});
      expect(store.state.isLoading).toBe(false);
      expect(store.state.error).toBeNull();
      expect(store.state.sortBy).toBe('name');
      expect(store.state.sortAscending).toBe(true);
      expect(store.state.showHidden).toBe(false);
    });
  });

  describe('Navigation', () => {
    it('should navigate to a path', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.navigate('/home/user');

      expect(store.state.currentPath).toBe('/home/user');
      expect(store.state.isLoading).toBe(true);
      expect(store.state.selectedPaths.size).toBe(0);
      expect(subscriber).toHaveBeenCalledWith({
        type: 'files:navigate',
        path: '/home/user',
      });
    });

    it('should navigate up to parent directory', () => {
      store.navigate('/home/user/documents');
      store.setLoading(false);

      store.navigateUp();

      expect(store.state.currentPath).toBe('/home/user');
    });

    it('should navigate up from root to root', () => {
      store.navigate('/home');
      store.setLoading(false);

      store.navigateUp();

      expect(store.state.currentPath).toBe('/');
    });

    it('should stay at root when already at root', () => {
      store.navigate('/');
      store.setLoading(false);

      store.navigateUp();

      expect(store.state.currentPath).toBe('/');
    });
  });

  describe('Entries', () => {
    const mockEntries: FileEntry[] = [
      {
        name: 'file1.txt',
        path: '/file1.txt',
        type: 'file',
        size: 1024,
        modifiedAt: Date.now(),
        permissions: { read: true, write: true, execute: false },
        isHidden: false,
      },
      {
        name: 'folder1',
        path: '/folder1',
        type: 'directory',
        size: 4096,
        modifiedAt: Date.now(),
        permissions: { read: true, write: true, execute: true },
        isHidden: false,
      },
      {
        name: '.hidden',
        path: '/.hidden',
        type: 'file',
        size: 512,
        modifiedAt: Date.now(),
        permissions: { read: true, write: false, execute: false },
        isHidden: true,
      },
    ];

    it('should set entries and stop loading', () => {
      store.navigate('/');
      store.setEntries(mockEntries);

      expect(store.state.entries.length).toBeGreaterThan(0);
      expect(store.state.isLoading).toBe(false);
      expect(store.state.error).toBeNull();
    });

    it('should filter hidden files by default', () => {
      store.setEntries(mockEntries);

      expect(store.state.entries).toHaveLength(2);
      expect(store.state.entries.every(e => !e.isHidden)).toBe(true);
    });

    it('should show hidden files when enabled', () => {
      store.toggleHidden();
      store.setEntries(mockEntries);

      expect(store.state.showHidden).toBe(true);
    });

    it('should sort directories first by default', () => {
      const entries: FileEntry[] = [
        { name: 'a.txt', path: '/a.txt', type: 'file', size: 100, modifiedAt: Date.now(), permissions: { read: true, write: true, execute: false }, isHidden: false },
        { name: 'dir', path: '/dir', type: 'directory', size: 4096, modifiedAt: Date.now(), permissions: { read: true, write: true, execute: true }, isHidden: false },
      ];

      store.setEntries(entries);

      expect(store.state.entries[0].type).toBe('directory');
    });

    it('should get entry by path', () => {
      store.setEntries(mockEntries);

      const entry = store.getEntry('/file1.txt');

      expect(entry).not.toBeNull();
      expect(entry?.name).toBe('file1.txt');
    });

    it('should return null for non-existent entry', () => {
      store.setEntries(mockEntries);

      const entry = store.getEntry('/nonexistent');

      expect(entry).toBeNull();
    });
  });

  describe('Selection', () => {
    const mockEntries: FileEntry[] = [
      { name: 'file1.txt', path: '/file1.txt', type: 'file', size: 100, modifiedAt: Date.now(), permissions: { read: true, write: true, execute: false }, isHidden: false },
      { name: 'file2.txt', path: '/file2.txt', type: 'file', size: 200, modifiedAt: Date.now(), permissions: { read: true, write: true, execute: false }, isHidden: false },
      { name: 'file3.txt', path: '/file3.txt', type: 'file', size: 300, modifiedAt: Date.now(), permissions: { read: true, write: true, execute: false }, isHidden: false },
    ];

    beforeEach(() => {
      store.setEntries(mockEntries);
    });

    it('should select a file', () => {
      store.select('/file1.txt');

      expect(store.isSelected('/file1.txt')).toBe(true);
      expect(store.state.selectedPaths.size).toBe(1);
    });

    it('should replace selection when not additive', () => {
      store.select('/file1.txt');
      store.select('/file2.txt', false);

      expect(store.isSelected('/file1.txt')).toBe(false);
      expect(store.isSelected('/file2.txt')).toBe(true);
      expect(store.state.selectedPaths.size).toBe(1);
    });

    it('should add to selection when additive', () => {
      store.select('/file1.txt');
      store.select('/file2.txt', true);

      expect(store.isSelected('/file1.txt')).toBe(true);
      expect(store.isSelected('/file2.txt')).toBe(true);
      expect(store.state.selectedPaths.size).toBe(2);
    });

    it('should deselect a file', () => {
      store.select('/file1.txt');
      store.deselect('/file1.txt');

      expect(store.isSelected('/file1.txt')).toBe(false);
    });

    it('should toggle selection', () => {
      store.select('/file1.txt');
      store.toggleSelection('/file1.txt');

      expect(store.isSelected('/file1.txt')).toBe(false);

      store.toggleSelection('/file1.txt');

      expect(store.isSelected('/file1.txt')).toBe(true);
    });

    it('should select multiple files', () => {
      store.selectMultiple(['/file1.txt', '/file2.txt']);

      expect(store.isSelected('/file1.txt')).toBe(true);
      expect(store.isSelected('/file2.txt')).toBe(true);
      expect(store.state.selectedPaths.size).toBe(2);
    });

    it('should clear selection', () => {
      store.selectMultiple(['/file1.txt', '/file2.txt']);
      store.clearSelection();

      expect(store.state.selectedPaths.size).toBe(0);
    });

    it('should select all entries', () => {
      store.selectAll();

      expect(store.state.selectedPaths.size).toBe(3);
    });

    it('should get selected entries', () => {
      store.selectMultiple(['/file1.txt', '/file3.txt']);

      const selected = store.getSelectedEntries();

      expect(selected).toHaveLength(2);
      expect(selected.map(e => e.path)).toContain('/file1.txt');
      expect(selected.map(e => e.path)).toContain('/file3.txt');
    });
  });

  describe('Transfers', () => {
    it('should start a download transfer', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const transferId = store.startDownload('/path/to/file.txt', 'file.txt', 1024);

      expect(transferId).toMatch(/^transfer-\d+-[a-z0-9]+$/);
      expect(store.state.transfers[transferId]).toBeDefined();
      expect(store.state.transfers[transferId].direction).toBe('download');
      expect(store.state.transfers[transferId].status).toBe('pending');
      expect(subscriber).toHaveBeenCalledWith(expect.objectContaining({
        type: 'files:download',
        path: '/path/to/file.txt',
        transferId,
      }));
    });

    it('should start an upload transfer', () => {
      const mockFile = new File(['content'], 'upload.txt', { type: 'text/plain' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const transferId = store.startUpload(mockFile, '/destination');

      expect(transferId).toMatch(/^transfer-\d+-[a-z0-9]+$/);
      expect(store.state.transfers[transferId]).toBeDefined();
      expect(store.state.transfers[transferId].direction).toBe('upload');
      expect(store.state.transfers[transferId].status).toBe('pending');
      expect(subscriber).toHaveBeenCalledWith(expect.objectContaining({
        type: 'files:upload',
        transferId,
      }));
    });

    it('should update transfer to started', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.setTransferStarted(transferId);

      expect(store.state.transfers[transferId].status).toBe('in_progress');
      expect(subscriber).toHaveBeenCalledWith({
        type: 'transfer:started',
        transferId,
      });
    });

    it('should update transfer progress', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.setTransferStarted(transferId);

      store.updateTransferProgress(transferId, 512);

      expect(store.state.transfers[transferId].transferredBytes).toBe(512);
      expect(store.state.transfers[transferId].status).toBe('in_progress');
    });

    it('should complete a transfer', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.setTransferStarted(transferId);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.completeTransfer(transferId);

      expect(store.state.transfers[transferId].status).toBe('completed');
      expect(store.state.transfers[transferId].transferredBytes).toBe(1024);
      expect(store.state.transfers[transferId].completedAt).toBeDefined();
      expect(subscriber).toHaveBeenCalledWith({
        type: 'transfer:completed',
        transferId,
      });
    });

    it('should fail a transfer', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.setTransferStarted(transferId);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.failTransfer(transferId, 'Network error');

      expect(store.state.transfers[transferId].status).toBe('failed');
      expect(store.state.transfers[transferId].error).toBe('Network error');
      expect(subscriber).toHaveBeenCalledWith({
        type: 'transfer:failed',
        transferId,
        error: 'Network error',
      });
    });

    it('should cancel a transfer', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.setTransferStarted(transferId);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.cancelTransfer(transferId);

      expect(store.state.transfers[transferId].status).toBe('cancelled');
      expect(subscriber).toHaveBeenCalledWith({
        type: 'transfer:cancelled',
        transferId,
      });
    });

    it('should not cancel completed transfer', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.completeTransfer(transferId);

      store.cancelTransfer(transferId);

      expect(store.state.transfers[transferId].status).toBe('completed');
    });

    it('should remove a transfer', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);
      store.completeTransfer(transferId);

      store.removeTransfer(transferId);

      expect(store.state.transfers[transferId]).toBeUndefined();
    });

    it('should clear completed transfers', () => {
      const id1 = store.startDownload('/file1.txt', 'file1.txt', 1024);
      const id2 = store.startDownload('/file2.txt', 'file2.txt', 2048);
      const id3 = store.startDownload('/file3.txt', 'file3.txt', 4096);

      store.completeTransfer(id1);
      store.setTransferStarted(id2);
      store.failTransfer(id3, 'Error');

      store.clearCompletedTransfers();

      expect(store.state.transfers[id1]).toBeUndefined();
      expect(store.state.transfers[id2]).toBeDefined();
      expect(store.state.transfers[id3]).toBeUndefined();
    });

    it('should get transfer by ID', () => {
      const transferId = store.startDownload('/file.txt', 'file.txt', 1024);

      const transfer = store.getTransfer(transferId);

      expect(transfer).not.toBeNull();
      expect(transfer?.id).toBe(transferId);
    });

    it('should return null for non-existent transfer', () => {
      const transfer = store.getTransfer('non-existent');

      expect(transfer).toBeNull();
    });

    it('should get all transfers', () => {
      store.startDownload('/file1.txt', 'file1.txt', 1024);
      store.startDownload('/file2.txt', 'file2.txt', 2048);

      const transfers = store.getAllTransfers();

      expect(transfers).toHaveLength(2);
    });

    it('should get active transfers', () => {
      const id1 = store.startDownload('/file1.txt', 'file1.txt', 1024);
      const id2 = store.startDownload('/file2.txt', 'file2.txt', 2048);
      const id3 = store.startDownload('/file3.txt', 'file3.txt', 4096);

      store.setTransferStarted(id1);
      store.completeTransfer(id2);
      // id3 is still pending

      const active = store.getActiveTransfers();

      expect(active).toHaveLength(2);
      expect(active.map(t => t.id)).toContain(id1);
      expect(active.map(t => t.id)).toContain(id3);
    });
  });

  describe('Sorting', () => {
    const mockEntries: FileEntry[] = [
      { name: 'zebra.txt', path: '/zebra.txt', type: 'file', size: 300, modifiedAt: 3000, permissions: { read: true, write: true, execute: false }, isHidden: false },
      { name: 'apple.txt', path: '/apple.txt', type: 'file', size: 100, modifiedAt: 1000, permissions: { read: true, write: true, execute: false }, isHidden: false },
      { name: 'mango.txt', path: '/mango.txt', type: 'file', size: 200, modifiedAt: 2000, permissions: { read: true, write: true, execute: false }, isHidden: false },
    ];

    it('should sort by name ascending by default', () => {
      store.setEntries(mockEntries);

      expect(store.state.entries[0].name).toBe('apple.txt');
      expect(store.state.entries[2].name).toBe('zebra.txt');
    });

    it('should sort by name descending', () => {
      store.setSort('name', false);
      store.setEntries(mockEntries);

      expect(store.state.entries[0].name).toBe('zebra.txt');
      expect(store.state.entries[2].name).toBe('apple.txt');
    });

    it('should sort by size', () => {
      store.setSort('size', true);
      store.setEntries(mockEntries);

      expect(store.state.entries[0].size).toBe(100);
      expect(store.state.entries[2].size).toBe(300);
    });

    it('should sort by date', () => {
      store.setSort('modifiedAt', true);
      store.setEntries(mockEntries);

      expect(store.state.entries[0].modifiedAt).toBe(1000);
      expect(store.state.entries[2].modifiedAt).toBe(3000);
    });
  });

  describe('Error Handling', () => {
    it('should set error and stop loading', () => {
      store.navigate('/some/path');
      store.setError('Permission denied');

      expect(store.state.error).toBe('Permission denied');
      expect(store.state.isLoading).toBe(false);
    });

    it('should clear error on successful navigation', () => {
      store.setError('Previous error');
      store.navigate('/new/path');

      expect(store.state.error).toBeNull();
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.navigate('/test');

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = store.subscribe(subscriber);

      store.navigate('/test1');
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      store.navigate('/test2');

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should handle errors in subscribers gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      store.subscribe(errorSubscriber);
      store.subscribe(goodSubscriber);

      expect(() => store.navigate('/test')).not.toThrow();

      expect(errorSubscriber).toHaveBeenCalledTimes(1);
      expect(goodSubscriber).toHaveBeenCalledTimes(1);
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Reset', () => {
    it('should reset store to initial state', () => {
      store.navigate('/some/path');
      store.startDownload('/file.txt', 'file.txt', 1024);
      store.select('/file.txt');

      store.reset();

      expect(store.state.currentPath).toBe('/');
      expect(store.state.entries).toEqual([]);
      expect(store.state.selectedPaths.size).toBe(0);
      expect(store.state.transfers).toEqual({});
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getFileStore', () => {
      const store1 = getFileStore();
      const store2 = getFileStore();

      expect(store1).toBe(store2);
    });

    it('should create new instance after resetFileStore', () => {
      const store1 = getFileStore();
      resetFileStore();
      const store2 = getFileStore();

      expect(store1).not.toBe(store2);
    });
  });
});

describe('Utility Functions', () => {
  describe('formatBytes', () => {
    it('should format bytes correctly', () => {
      expect(formatBytes(0)).toBe('0 B');
      expect(formatBytes(100)).toBe('100 B');
      expect(formatBytes(1024)).toBe('1 KB');
      expect(formatBytes(1536)).toBe('1.5 KB');
      expect(formatBytes(1048576)).toBe('1 MB');
      expect(formatBytes(1073741824)).toBe('1 GB');
    });
  });

  describe('formatPermissions', () => {
    it('should format permissions correctly', () => {
      expect(formatPermissions({ read: true, write: true, execute: true })).toBe('rwx');
      expect(formatPermissions({ read: true, write: true, execute: false })).toBe('rw-');
      expect(formatPermissions({ read: true, write: false, execute: false })).toBe('r--');
      expect(formatPermissions({ read: false, write: false, execute: false })).toBe('---');
      expect(formatPermissions({ read: true, write: false, execute: true })).toBe('r-x');
    });
  });
});
