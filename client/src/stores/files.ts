import { createStore, produce } from 'solid-js/store';
import { createSignal, batch } from 'solid-js';

/**
 * File types
 */
export type FileType = 'file' | 'directory' | 'symlink' | 'unknown';

/**
 * File permissions representation
 */
export interface FilePermissions {
  read: boolean;
  write: boolean;
  execute: boolean;
}

/**
 * File entry in the file browser
 */
export interface FileEntry {
  name: string;
  path: string;
  type: FileType;
  size: number;
  modifiedAt: number;
  permissions: FilePermissions;
  isHidden: boolean;
}

/**
 * Transfer direction
 */
export type TransferDirection = 'upload' | 'download';

/**
 * Transfer status
 */
export type TransferStatus = 'pending' | 'in_progress' | 'completed' | 'failed' | 'cancelled';

/**
 * File transfer information
 */
export interface FileTransfer {
  id: string;
  fileName: string;
  filePath: string;
  direction: TransferDirection;
  status: TransferStatus;
  totalBytes: number;
  transferredBytes: number;
  startedAt: number;
  completedAt?: number;
  error?: string;
}

/**
 * File store state
 */
export interface FileState {
  currentPath: string;
  entries: FileEntry[];
  selectedPaths: Set<string>;
  transfers: Record<string, FileTransfer>;
  isLoading: boolean;
  error: string | null;
  sortBy: 'name' | 'size' | 'modifiedAt' | 'type';
  sortAscending: boolean;
  showHidden: boolean;
}

/**
 * Event types emitted by the file store
 */
export type FileEventType =
  | 'files:navigate'
  | 'files:refresh'
  | 'files:select'
  | 'files:deselect'
  | 'files:download'
  | 'files:upload'
  | 'transfer:started'
  | 'transfer:progress'
  | 'transfer:completed'
  | 'transfer:failed'
  | 'transfer:cancelled';

/**
 * Event payload types
 */
export interface FileEvent {
  type: FileEventType;
  path?: string;
  paths?: string[];
  transferId?: string;
  data?: unknown;
  error?: string;
}

/**
 * Event subscriber callback type
 */
export type FileEventSubscriber = (event: FileEvent) => void;

/**
 * Options for navigating to a directory
 */
export interface NavigateOptions {
  path: string;
}

/**
 * Options for uploading a file
 */
export interface UploadOptions {
  file: File;
  destinationPath: string;
}

/**
 * Create initial state for the file store
 */
function createInitialState(): FileState {
  return {
    currentPath: '/',
    entries: [],
    selectedPaths: new Set(),
    transfers: {},
    isLoading: false,
    error: null,
    sortBy: 'name',
    sortAscending: true,
    showHidden: false,
  };
}

/**
 * Generate a unique transfer ID
 */
function generateTransferId(): string {
  return `transfer-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * Format bytes to human-readable string
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

/**
 * Format permissions to unix-style string
 */
export function formatPermissions(permissions: FilePermissions): string {
  return `${permissions.read ? 'r' : '-'}${permissions.write ? 'w' : '-'}${permissions.execute ? 'x' : '-'}`;
}

/**
 * Sort file entries
 */
function sortEntries(
  entries: FileEntry[],
  sortBy: FileState['sortBy'],
  ascending: boolean
): FileEntry[] {
  const sorted = [...entries];

  // Always put directories first
  sorted.sort((a, b) => {
    // Directories first
    if (a.type === 'directory' && b.type !== 'directory') return -1;
    if (a.type !== 'directory' && b.type === 'directory') return 1;

    // Then sort by the specified field
    let comparison = 0;
    switch (sortBy) {
      case 'name':
        comparison = a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
        break;
      case 'size':
        comparison = a.size - b.size;
        break;
      case 'modifiedAt':
        comparison = a.modifiedAt - b.modifiedAt;
        break;
      case 'type':
        comparison = a.type.localeCompare(b.type);
        break;
    }

    return ascending ? comparison : -comparison;
  });

  return sorted;
}

/**
 * Creates a file store for managing file browser state
 */
export function createFileStore() {
  const [state, setState] = createStore<FileState>(createInitialState());
  const [subscribers] = createSignal<Set<FileEventSubscriber>>(new Set());

  /**
   * Emit an event to all subscribers
   */
  const emit = (event: FileEvent): void => {
    subscribers().forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in file event subscriber:', error);
      }
    });
  };

  /**
   * Subscribe to file events
   */
  const subscribe = (callback: FileEventSubscriber): (() => void) => {
    subscribers().add(callback);
    return () => {
      subscribers().delete(callback);
    };
  };

  /**
   * Navigate to a directory
   */
  const navigate = (path: string): void => {
    batch(() => {
      setState('isLoading', true);
      setState('error', null);
      setState('currentPath', path);
      setState('selectedPaths', new Set());
    });

    emit({ type: 'files:navigate', path });
  };

  /**
   * Set directory entries (called when file list is received)
   */
  const setEntries = (entries: FileEntry[]): void => {
    const filteredEntries = state.showHidden
      ? entries
      : entries.filter(e => !e.isHidden);

    const sorted = sortEntries(filteredEntries, state.sortBy, state.sortAscending);

    batch(() => {
      setState('entries', sorted);
      setState('isLoading', false);
      setState('error', null);
    });
  };

  /**
   * Set loading state
   */
  const setLoading = (loading: boolean): void => {
    setState('isLoading', loading);
  };

  /**
   * Set error state
   */
  const setError = (error: string | null): void => {
    batch(() => {
      setState('error', error);
      setState('isLoading', false);
    });
  };

  /**
   * Refresh current directory
   */
  const refresh = (): void => {
    setState('isLoading', true);
    emit({ type: 'files:refresh', path: state.currentPath });
  };

  /**
   * Navigate up to parent directory
   */
  const navigateUp = (): void => {
    const parts = state.currentPath.split('/').filter(Boolean);
    if (parts.length > 0) {
      parts.pop();
      const parentPath = '/' + parts.join('/');
      navigate(parentPath);
    }
  };

  /**
   * Select a file/directory
   */
  const select = (path: string, additive: boolean = false): void => {
    setState(
      produce((s) => {
        if (!additive) {
          s.selectedPaths.clear();
        }
        s.selectedPaths.add(path);
      })
    );
    emit({ type: 'files:select', path });
  };

  /**
   * Select multiple files/directories
   */
  const selectMultiple = (paths: string[], additive: boolean = false): void => {
    setState(
      produce((s) => {
        if (!additive) {
          s.selectedPaths.clear();
        }
        paths.forEach(path => s.selectedPaths.add(path));
      })
    );
    emit({ type: 'files:select', paths });
  };

  /**
   * Deselect a file/directory
   */
  const deselect = (path: string): void => {
    setState(
      produce((s) => {
        s.selectedPaths.delete(path);
      })
    );
    emit({ type: 'files:deselect', path });
  };

  /**
   * Toggle selection of a file/directory
   */
  const toggleSelection = (path: string): void => {
    if (state.selectedPaths.has(path)) {
      deselect(path);
    } else {
      select(path, true);
    }
  };

  /**
   * Clear all selections
   */
  const clearSelection = (): void => {
    setState('selectedPaths', new Set());
  };

  /**
   * Select all entries
   */
  const selectAll = (): void => {
    const allPaths = state.entries.map(e => e.path);
    setState('selectedPaths', new Set(allPaths));
  };

  /**
   * Check if a path is selected
   */
  const isSelected = (path: string): boolean => {
    return state.selectedPaths.has(path);
  };

  /**
   * Get selected entries
   */
  const getSelectedEntries = (): FileEntry[] => {
    return state.entries.filter(e => state.selectedPaths.has(e.path));
  };

  /**
   * Start a download transfer
   */
  const startDownload = (filePath: string, fileName: string, totalBytes: number): string => {
    const transferId = generateTransferId();
    const transfer: FileTransfer = {
      id: transferId,
      fileName,
      filePath,
      direction: 'download',
      status: 'pending',
      totalBytes,
      transferredBytes: 0,
      startedAt: Date.now(),
    };

    setState(
      produce((s) => {
        s.transfers[transferId] = transfer;
      })
    );

    emit({
      type: 'files:download',
      path: filePath,
      transferId,
    });

    return transferId;
  };

  /**
   * Start an upload transfer
   */
  const startUpload = (file: File, destinationPath: string): string => {
    const transferId = generateTransferId();
    const filePath = `${destinationPath}/${file.name}`;
    const transfer: FileTransfer = {
      id: transferId,
      fileName: file.name,
      filePath,
      direction: 'upload',
      status: 'pending',
      totalBytes: file.size,
      transferredBytes: 0,
      startedAt: Date.now(),
    };

    setState(
      produce((s) => {
        s.transfers[transferId] = transfer;
      })
    );

    emit({
      type: 'files:upload',
      path: filePath,
      transferId,
      data: { file },
    });

    return transferId;
  };

  /**
   * Update transfer status to in progress
   */
  const setTransferStarted = (transferId: string): void => {
    setState(
      produce((s) => {
        const transfer = s.transfers[transferId];
        if (transfer) {
          transfer.status = 'in_progress';
        }
      })
    );
    emit({ type: 'transfer:started', transferId });
  };

  /**
   * Update transfer progress
   */
  const updateTransferProgress = (transferId: string, transferredBytes: number): void => {
    setState(
      produce((s) => {
        const transfer = s.transfers[transferId];
        if (transfer) {
          transfer.transferredBytes = transferredBytes;
          transfer.status = 'in_progress';
        }
      })
    );
    emit({
      type: 'transfer:progress',
      transferId,
      data: { transferredBytes },
    });
  };

  /**
   * Mark transfer as completed
   */
  const completeTransfer = (transferId: string): void => {
    setState(
      produce((s) => {
        const transfer = s.transfers[transferId];
        if (transfer) {
          transfer.status = 'completed';
          transfer.transferredBytes = transfer.totalBytes;
          transfer.completedAt = Date.now();
        }
      })
    );
    emit({ type: 'transfer:completed', transferId });
  };

  /**
   * Mark transfer as failed
   */
  const failTransfer = (transferId: string, error: string): void => {
    setState(
      produce((s) => {
        const transfer = s.transfers[transferId];
        if (transfer) {
          transfer.status = 'failed';
          transfer.error = error;
          transfer.completedAt = Date.now();
        }
      })
    );
    emit({ type: 'transfer:failed', transferId, error });
  };

  /**
   * Cancel a transfer
   */
  const cancelTransfer = (transferId: string): void => {
    setState(
      produce((s) => {
        const transfer = s.transfers[transferId];
        if (transfer && transfer.status !== 'completed') {
          transfer.status = 'cancelled';
          transfer.completedAt = Date.now();
        }
      })
    );
    emit({ type: 'transfer:cancelled', transferId });
  };

  /**
   * Remove a completed/failed/cancelled transfer from the list
   */
  const removeTransfer = (transferId: string): void => {
    setState(
      produce((s) => {
        delete s.transfers[transferId];
      })
    );
  };

  /**
   * Clear all completed transfers
   */
  const clearCompletedTransfers = (): void => {
    setState(
      produce((s) => {
        for (const transferId of Object.keys(s.transfers)) {
          const transfer = s.transfers[transferId];
          if (transfer.status === 'completed' || transfer.status === 'failed' || transfer.status === 'cancelled') {
            delete s.transfers[transferId];
          }
        }
      })
    );
  };

  /**
   * Get a transfer by ID
   */
  const getTransfer = (transferId: string): FileTransfer | null => {
    return state.transfers[transferId] ?? null;
  };

  /**
   * Get all transfers
   */
  const getAllTransfers = (): FileTransfer[] => {
    return Object.values(state.transfers);
  };

  /**
   * Get active transfers (pending or in progress)
   */
  const getActiveTransfers = (): FileTransfer[] => {
    return Object.values(state.transfers).filter(
      t => t.status === 'pending' || t.status === 'in_progress'
    );
  };

  /**
   * Set sort options
   */
  const setSort = (sortBy: FileState['sortBy'], ascending: boolean): void => {
    const sorted = sortEntries(state.entries, sortBy, ascending);
    batch(() => {
      setState('sortBy', sortBy);
      setState('sortAscending', ascending);
      setState('entries', sorted);
    });
  };

  /**
   * Toggle hidden files visibility
   */
  const toggleHidden = (): void => {
    setState('showHidden', !state.showHidden);
    // Trigger a refresh to update the list
    refresh();
  };

  /**
   * Get an entry by path
   */
  const getEntry = (path: string): FileEntry | null => {
    return state.entries.find(e => e.path === path) ?? null;
  };

  /**
   * Reset the store to initial state
   */
  const reset = (): void => {
    setState(createInitialState());
  };

  return {
    // State (readonly)
    state,

    // Navigation actions
    navigate,
    navigateUp,
    refresh,
    setEntries,
    setLoading,
    setError,

    // Selection actions
    select,
    selectMultiple,
    deselect,
    toggleSelection,
    clearSelection,
    selectAll,

    // Transfer actions
    startDownload,
    startUpload,
    setTransferStarted,
    updateTransferProgress,
    completeTransfer,
    failTransfer,
    cancelTransfer,
    removeTransfer,
    clearCompletedTransfers,

    // Sort/filter actions
    setSort,
    toggleHidden,

    // Getters
    isSelected,
    getSelectedEntries,
    getTransfer,
    getAllTransfers,
    getActiveTransfers,
    getEntry,

    // Event subscriptions
    subscribe,

    // Utility
    reset,
  };
}

/**
 * Type for the file store instance
 */
export type FileStore = ReturnType<typeof createFileStore>;

/**
 * Singleton instance of the file store
 */
let fileStoreInstance: FileStore | null = null;

/**
 * Get or create the singleton file store instance
 */
export function getFileStore(): FileStore {
  if (!fileStoreInstance) {
    fileStoreInstance = createFileStore();
  }
  return fileStoreInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetFileStore(): void {
  if (fileStoreInstance) {
    fileStoreInstance.reset();
  }
  fileStoreInstance = null;
}
