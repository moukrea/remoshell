import { Component, For, Show, createMemo } from 'solid-js';
import {
  type FileTransfer,
  type FileStore,
  type TransferStatus,
  getFileStore,
  formatBytes,
} from '../../stores/files';

/**
 * Calculate transfer progress percentage
 */
const calculateProgress = (transfer: FileTransfer): number => {
  if (transfer.totalBytes === 0) return 0;
  return Math.round((transfer.transferredBytes / transfer.totalBytes) * 100);
};

/**
 * Calculate transfer speed (bytes per second)
 */
const calculateSpeed = (transfer: FileTransfer): number => {
  if (transfer.status !== 'in_progress') return 0;
  const elapsed = (Date.now() - transfer.startedAt) / 1000;
  if (elapsed === 0) return 0;
  return transfer.transferredBytes / elapsed;
};

/**
 * Estimate remaining time
 */
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

/**
 * Get status display text
 */
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

/**
 * Get status icon/indicator
 */
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

export interface FileTransferProgressProps {
  /** Store instance (uses singleton if not provided) */
  store?: FileStore;
  /** Show only active transfers */
  showOnlyActive?: boolean;
  /** Maximum number of transfers to show */
  maxItems?: number;
  /** Additional CSS class for the container */
  class?: string;
}

export interface TransferItemProps {
  transfer: FileTransfer;
  onCancel?: (transferId: string) => void;
  onRemove?: (transferId: string) => void;
  onRetry?: (transfer: FileTransfer) => void;
}

/**
 * Individual transfer progress item
 */
const TransferItem: Component<TransferItemProps> = (props) => {
  const progress = createMemo(() => calculateProgress(props.transfer));
  const speed = createMemo(() => calculateSpeed(props.transfer));
  const remaining = createMemo(() => estimateRemainingTime(props.transfer));

  const handleCancel = (e: MouseEvent) => {
    e.stopPropagation();
    props.onCancel?.(props.transfer.id);
  };

  const handleRemove = (e: MouseEvent) => {
    e.stopPropagation();
    props.onRemove?.(props.transfer.id);
  };

  const handleRetry = (e: MouseEvent) => {
    e.stopPropagation();
    props.onRetry?.(props.transfer);
  };

  return (
    <div
      class={`transfer-item transfer-item--${props.transfer.status} transfer-item--${props.transfer.direction}`}
      data-testid={`transfer-item-${props.transfer.id}`}
    >
      {/* Direction and Status Icon */}
      <div class="transfer-item__icon" data-testid={`transfer-icon-${props.transfer.id}`}>
        <span class="transfer-item__direction">
          {props.transfer.direction === 'upload' ? 'Up' : 'Dn'}
        </span>
        <span class={`transfer-item__status-icon transfer-item__status-icon--${props.transfer.status}`}>
          {getStatusIcon(props.transfer.status)}
        </span>
      </div>

      {/* File Info */}
      <div class="transfer-item__info">
        <div class="transfer-item__name" data-testid={`transfer-name-${props.transfer.id}`}>
          {props.transfer.fileName}
        </div>
        <div class="transfer-item__path" title={props.transfer.filePath}>
          {props.transfer.filePath}
        </div>
      </div>

      {/* Progress Section */}
      <div class="transfer-item__progress-section">
        {/* Progress Bar */}
        <div class="transfer-item__progress-bar-container">
          <div
            class={`transfer-item__progress-bar transfer-item__progress-bar--${props.transfer.status}`}
            style={{ width: `${progress()}%` }}
            data-testid={`transfer-progress-bar-${props.transfer.id}`}
          />
        </div>

        {/* Progress Text */}
        <div class="transfer-item__progress-text">
          <Show when={props.transfer.status === 'in_progress' || props.transfer.status === 'pending'}>
            <span data-testid={`transfer-percent-${props.transfer.id}`}>{progress()}%</span>
            <span class="transfer-item__size">
              {formatBytes(props.transfer.transferredBytes)} / {formatBytes(props.transfer.totalBytes)}
            </span>
          </Show>

          <Show when={props.transfer.status === 'in_progress'}>
            <span class="transfer-item__speed" data-testid={`transfer-speed-${props.transfer.id}`}>
              {formatBytes(speed())}/s
            </span>
            <span class="transfer-item__remaining" data-testid={`transfer-remaining-${props.transfer.id}`}>
              {remaining()} remaining
            </span>
          </Show>

          <Show when={props.transfer.status === 'completed'}>
            <span class="transfer-item__completed-text">
              {formatBytes(props.transfer.totalBytes)} transferred
            </span>
          </Show>

          <Show when={props.transfer.status === 'failed'}>
            <span class="transfer-item__error" data-testid={`transfer-error-${props.transfer.id}`}>
              {props.transfer.error ?? 'Transfer failed'}
            </span>
          </Show>

          <Show when={props.transfer.status === 'cancelled'}>
            <span class="transfer-item__cancelled-text">
              Transfer cancelled
            </span>
          </Show>
        </div>
      </div>

      {/* Actions */}
      <div class="transfer-item__actions">
        <Show when={props.transfer.status === 'pending' || props.transfer.status === 'in_progress'}>
          <button
            class="transfer-item__action transfer-item__action--cancel"
            onClick={handleCancel}
            title="Cancel transfer"
            data-testid={`transfer-cancel-${props.transfer.id}`}
          >
            Cancel
          </button>
        </Show>

        <Show when={props.transfer.status === 'failed' && props.onRetry}>
          <button
            class="transfer-item__action transfer-item__action--retry"
            onClick={handleRetry}
            title="Retry transfer"
            data-testid={`transfer-retry-${props.transfer.id}`}
          >
            Retry
          </button>
        </Show>

        <Show when={props.transfer.status === 'completed' || props.transfer.status === 'failed' || props.transfer.status === 'cancelled'}>
          <button
            class="transfer-item__action transfer-item__action--remove"
            onClick={handleRemove}
            title="Remove from list"
            data-testid={`transfer-remove-${props.transfer.id}`}
          >
            Remove
          </button>
        </Show>
      </div>
    </div>
  );
};

/**
 * File transfer progress component showing all active and recent transfers
 */
const FileTransferProgress: Component<FileTransferProgressProps> = (props) => {
  const store = props.store ?? getFileStore();

  const transfers = createMemo(() => {
    let list = store.getAllTransfers();

    // Filter to active only if requested
    if (props.showOnlyActive) {
      list = list.filter(t => t.status === 'pending' || t.status === 'in_progress');
    }

    // Sort: in_progress first, then pending, then by start time (newest first)
    list.sort((a, b) => {
      const statusOrder: Record<TransferStatus, number> = {
        in_progress: 0,
        pending: 1,
        completed: 2,
        failed: 3,
        cancelled: 4,
      };

      const statusDiff = statusOrder[a.status] - statusOrder[b.status];
      if (statusDiff !== 0) return statusDiff;

      return b.startedAt - a.startedAt;
    });

    // Limit items if specified
    if (props.maxItems && list.length > props.maxItems) {
      list = list.slice(0, props.maxItems);
    }

    return list;
  });

  const activeCount = createMemo(() => {
    return store.getActiveTransfers().length;
  });

  const handleCancel = (transferId: string) => {
    store.cancelTransfer(transferId);
  };

  const handleRemove = (transferId: string) => {
    store.removeTransfer(transferId);
  };

  const handleClearCompleted = () => {
    store.clearCompletedTransfers();
  };

  // Calculate overall progress for summary
  const overallProgress = createMemo(() => {
    const active = store.getActiveTransfers();
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
  });

  return (
    <div
      class={`file-transfer-progress ${props.class ?? ''}`}
      data-testid="file-transfer-progress"
    >
      {/* Header */}
      <div class="file-transfer-progress__header">
        <h3 class="file-transfer-progress__title">
          Transfers
          <Show when={activeCount() > 0}>
            <span class="file-transfer-progress__active-count" data-testid="active-transfer-count">
              ({activeCount()} active)
            </span>
          </Show>
        </h3>
        <Show when={transfers().some(t => t.status === 'completed' || t.status === 'failed' || t.status === 'cancelled')}>
          <button
            class="file-transfer-progress__clear-btn"
            onClick={handleClearCompleted}
            title="Clear completed transfers"
            data-testid="btn-clear-completed"
          >
            Clear Completed
          </button>
        </Show>
      </div>

      {/* Overall Progress Summary (when multiple active) */}
      <Show when={overallProgress()}>
        {(progress) => (
          <div class="file-transfer-progress__summary" data-testid="transfer-summary">
            <div class="file-transfer-progress__summary-bar-container">
              <div
                class="file-transfer-progress__summary-bar"
                style={{ width: `${progress().percent}%` }}
              />
            </div>
            <div class="file-transfer-progress__summary-text">
              Overall: {progress().percent}% ({formatBytes(progress().transferred)} / {formatBytes(progress().total)})
              - {progress().count} transfer{progress().count !== 1 ? 's' : ''}
            </div>
          </div>
        )}
      </Show>

      {/* Transfer List */}
      <div class="file-transfer-progress__list" data-testid="transfer-list">
        <Show
          when={transfers().length > 0}
          fallback={
            <div class="file-transfer-progress__empty" data-testid="transfer-empty">
              No transfers
            </div>
          }
        >
          <For each={transfers()}>
            {(transfer) => (
              <TransferItem
                transfer={transfer}
                onCancel={handleCancel}
                onRemove={handleRemove}
              />
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default FileTransferProgress;
export {
  TransferItem,
  calculateProgress,
  calculateSpeed,
  estimateRemainingTime,
  getStatusText,
  getStatusIcon,
};
