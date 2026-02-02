import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import {
  type FileEntry,
  type FileStore,
  getFileStore,
  formatBytes,
  formatPermissions,
} from '../../stores/files';

/**
 * Format a timestamp to a human-readable date string
 */
const formatDate = (timestamp: number): string => {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - timestamp;

  // If less than 24 hours ago, show time
  if (diff < 24 * 60 * 60 * 1000) {
    return date.toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  // If this year, show month and day
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
    });
  }

  // Otherwise show full date
  return date.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
};

/**
 * Get an icon character for file type
 */
const getFileIcon = (entry: FileEntry): string => {
  if (entry.type === 'directory') return 'D';
  if (entry.type === 'symlink') return 'L';

  // Check extension for common file types
  const ext = entry.name.split('.').pop()?.toLowerCase() ?? '';
  const iconMap: Record<string, string> = {
    txt: 'T',
    md: 'M',
    json: 'J',
    js: 'j',
    ts: 't',
    jsx: 'j',
    tsx: 't',
    html: 'h',
    css: 'c',
    py: 'p',
    sh: 's',
    zip: 'Z',
    tar: 'Z',
    gz: 'Z',
    png: 'I',
    jpg: 'I',
    jpeg: 'I',
    gif: 'I',
    svg: 'I',
    pdf: 'P',
  };

  return iconMap[ext] ?? 'F';
};

/**
 * Default validation constants
 */
const DEFAULT_MAX_SIZE = 100 * 1024 * 1024; // 100MB
const DEFAULT_MAX_COUNT = 100;
const DEFAULT_BLOCKED_EXTENSIONS = ['.exe', '.dll', '.so', '.dylib'];

/**
 * Validate files for upload
 */
const validateFiles = (
  files: File[],
  options: {
    maxFileSize: number;
    maxFileCount: number;
    allowedExtensions: string[];
    blockedExtensions: string[];
  }
): { valid: File[]; errors: ValidationError[] } => {
  const valid: File[] = [];
  const errors: ValidationError[] = [];

  // Check total count
  if (files.length > options.maxFileCount) {
    errors.push({
      fileName: '',
      reason: 'count',
      message: `Too many files. Maximum ${options.maxFileCount} files allowed, got ${files.length}.`,
    });
    // Still validate individual files
  }

  for (const file of files) {
    const nameParts = file.name.split('.');
    const ext = nameParts.length > 1 ? '.' + nameParts.pop()?.toLowerCase() : '';

    // Check size
    if (file.size > options.maxFileSize) {
      errors.push({
        fileName: file.name,
        reason: 'size',
        message: `File too large (${formatBytes(file.size)}). Maximum size is ${formatBytes(options.maxFileSize)}.`,
      });
      continue;
    }

    // Check blocked extensions
    if (options.blockedExtensions.length > 0 && options.blockedExtensions.includes(ext)) {
      errors.push({
        fileName: file.name,
        reason: 'type',
        message: `File type ${ext} is not allowed.`,
      });
      continue;
    }

    // Check allowed extensions (if specified)
    if (options.allowedExtensions.length > 0 && !options.allowedExtensions.includes(ext)) {
      errors.push({
        fileName: file.name,
        reason: 'type',
        message: `File type ${ext} is not in allowed list.`,
      });
      continue;
    }

    // Only add if within count limit
    if (valid.length < options.maxFileCount) {
      valid.push(file);
    }
  }

  return { valid, errors };
};

/**
 * Validation error for file upload
 */
export interface ValidationError {
  fileName: string;
  reason: 'size' | 'type' | 'count' | 'unknown';
  message: string;
}

export interface FileBrowserProps {
  /** Called when a file download is requested */
  onDownload?: (entry: FileEntry) => void;
  /** Called when files are dropped for upload */
  onUpload?: (files: File[]) => void;
  /** Store instance (uses singleton if not provided) */
  store?: FileStore;
  /** Additional CSS class for the container */
  class?: string;
  /** Maximum file size in bytes (default: 100MB) */
  maxFileSize?: number;
  /** Maximum number of files per upload (default: 100) */
  maxFileCount?: number;
  /** Allowed file extensions (e.g., ['.txt', '.pdf']). Empty = all allowed */
  allowedExtensions?: string[];
  /** Blocked file extensions (e.g., ['.exe', '.dll']) */
  blockedExtensions?: string[];
  /** Callback for validation errors */
  onValidationError?: (errors: ValidationError[]) => void;
}

export interface FileEntryRowProps {
  entry: FileEntry;
  isSelected: boolean;
  onSelect: (path: string, additive: boolean) => void;
  onActivate: (entry: FileEntry) => void;
  onDownload?: (entry: FileEntry) => void;
}

/**
 * Individual file entry row component
 */
const FileEntryRow: Component<FileEntryRowProps> = (props) => {
  const handleClick = (e: MouseEvent) => {
    const additive = e.ctrlKey || e.metaKey;
    props.onSelect(props.entry.path, additive);
  };

  const handleDoubleClick = () => {
    props.onActivate(props.entry);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter') {
      props.onActivate(props.entry);
    }
  };

  const handleDownloadClick = (e: MouseEvent) => {
    e.stopPropagation();
    if (props.entry.type !== 'directory') {
      props.onDownload?.(props.entry);
    }
  };

  return (
    <div
      class={`file-entry ${props.isSelected ? 'file-entry--selected' : ''} ${props.entry.type === 'directory' ? 'file-entry--directory' : 'file-entry--file'}`}
      data-testid={`file-entry-${props.entry.name}`}
      onClick={handleClick}
      onDblClick={handleDoubleClick}
      onKeyDown={handleKeyDown}
      tabIndex={0}
      role="row"
      aria-selected={props.isSelected}
    >
      {/* File Icon */}
      <div class="file-entry__icon" data-testid={`file-icon-${props.entry.name}`}>
        {getFileIcon(props.entry)}
      </div>

      {/* File Name */}
      <div class="file-entry__name" data-testid={`file-name-${props.entry.name}`}>
        {props.entry.name}
        <Show when={props.entry.type === 'symlink'}>
          <span class="file-entry__symlink-indicator" title="Symbolic link">
            {' -> '}
          </span>
        </Show>
      </div>

      {/* File Size */}
      <div class="file-entry__size" data-testid={`file-size-${props.entry.name}`}>
        <Show when={props.entry.type !== 'directory'} fallback="-">
          {formatBytes(props.entry.size)}
        </Show>
      </div>

      {/* Modified Date */}
      <div class="file-entry__date" data-testid={`file-date-${props.entry.name}`}>
        {formatDate(props.entry.modifiedAt)}
      </div>

      {/* Permissions */}
      <div class="file-entry__permissions" data-testid={`file-permissions-${props.entry.name}`}>
        {formatPermissions(props.entry.permissions)}
      </div>

      {/* Actions */}
      <div class="file-entry__actions">
        <Show when={props.entry.type !== 'directory' && props.onDownload}>
          <button
            class="file-entry__action file-entry__action--download"
            onClick={handleDownloadClick}
            title="Download"
            data-testid={`file-download-${props.entry.name}`}
            aria-label={`Download ${props.entry.name}`}
          >
            Download
          </button>
        </Show>
      </div>
    </div>
  );
};

/**
 * Breadcrumb navigation component
 */
interface BreadcrumbProps {
  path: string;
  onNavigate: (path: string) => void;
}

const Breadcrumb: Component<BreadcrumbProps> = (props) => {
  const pathParts = createMemo(() => {
    const parts = props.path.split('/').filter(Boolean);
    const result: { name: string; path: string }[] = [{ name: '/', path: '/' }];

    let currentPath = '';
    for (const part of parts) {
      currentPath += '/' + part;
      result.push({ name: part, path: currentPath });
    }

    return result;
  });

  return (
    <nav class="file-breadcrumb" data-testid="file-breadcrumb" aria-label="File path">
      <For each={pathParts()}>
        {(part, index) => (
          <>
            <Show when={index() > 0}>
              <span class="file-breadcrumb__separator">/</span>
            </Show>
            <button
              class="file-breadcrumb__part"
              onClick={() => props.onNavigate(part.path)}
              data-testid={`breadcrumb-${part.name}`}
            >
              {part.name}
            </button>
          </>
        )}
      </For>
    </nav>
  );
};

/**
 * File browser component for navigating remote file system
 */
const FileBrowser: Component<FileBrowserProps> = (props) => {
  const store = props.store ?? getFileStore();
  const [isDragOver, setIsDragOver] = createSignal(false);
  const [lastSelectedIndex, setLastSelectedIndex] = createSignal<number | null>(null);
  const [validationErrors, setValidationErrors] = createSignal<ValidationError[]>([]);

  let containerRef: HTMLDivElement | undefined;

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    const entries = store.state.entries;
    if (entries.length === 0) return;

    const currentIndex = lastSelectedIndex() ?? -1;

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        const nextIndex = Math.min(currentIndex + 1, entries.length - 1);
        const entry = entries[nextIndex];
        store.select(entry.path, e.shiftKey);
        setLastSelectedIndex(nextIndex);
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        const prevIndex = Math.max(currentIndex - 1, 0);
        const entry = entries[prevIndex];
        store.select(entry.path, e.shiftKey);
        setLastSelectedIndex(prevIndex);
        break;
      }
      case 'Enter': {
        if (store.state.selectedPaths.size === 1) {
          const selectedPath = Array.from(store.state.selectedPaths)[0];
          const entry = store.getEntry(selectedPath);
          if (entry) {
            handleActivate(entry);
          }
        }
        break;
      }
      case 'Backspace': {
        store.navigateUp();
        break;
      }
      case 'a': {
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          store.selectAll();
        }
        break;
      }
      case 'Escape': {
        store.clearSelection();
        break;
      }
    }
  };

  // Handle file selection
  const handleSelect = (path: string, additive: boolean) => {
    const index = store.state.entries.findIndex(e => e.path === path);
    setLastSelectedIndex(index);
    store.select(path, additive);
  };

  // Handle file/directory activation (double-click or enter)
  const handleActivate = (entry: FileEntry) => {
    if (entry.type === 'directory') {
      store.navigate(entry.path);
      setLastSelectedIndex(null);
    } else {
      // For files, trigger download
      props.onDownload?.(entry);
    }
  };

  // Handle download action
  const handleDownload = (entry: FileEntry) => {
    props.onDownload?.(entry);
  };

  // Drag and drop handlers
  const handleDragEnter = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = 'copy';
    }
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Only set to false if we're leaving the container
    const rect = containerRef?.getBoundingClientRect();
    if (rect) {
      const { clientX, clientY } = e;
      if (
        clientX < rect.left ||
        clientX >= rect.right ||
        clientY < rect.top ||
        clientY >= rect.bottom
      ) {
        setIsDragOver(false);
      }
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);

    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const fileArray = Array.from(files);

      // Validate files
      const { valid, errors } = validateFiles(fileArray, {
        maxFileSize: props.maxFileSize ?? DEFAULT_MAX_SIZE,
        maxFileCount: props.maxFileCount ?? DEFAULT_MAX_COUNT,
        allowedExtensions: props.allowedExtensions ?? [],
        blockedExtensions: props.blockedExtensions ?? DEFAULT_BLOCKED_EXTENSIONS,
      });

      // Report validation errors
      if (errors.length > 0) {
        setValidationErrors(errors);
        props.onValidationError?.(errors);
      }

      // Upload valid files
      if (valid.length > 0) {
        props.onUpload?.(valid);
      }
    }
  };

  // Sorted and filtered entries
  const displayEntries = createMemo(() => {
    return store.state.entries;
  });

  // Toolbar actions
  const handleRefresh = () => {
    store.refresh();
  };

  const handleNavigateUp = () => {
    store.navigateUp();
  };

  const handleToggleHidden = () => {
    store.toggleHidden();
  };

  const handleSortChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value as 'name' | 'size' | 'modifiedAt' | 'type';
    store.setSort(value, store.state.sortAscending);
  };

  const handleToggleSortOrder = () => {
    store.setSort(store.state.sortBy, !store.state.sortAscending);
  };

  return (
    <div
      ref={containerRef}
      class={`file-browser ${isDragOver() ? 'file-browser--drag-over' : ''} ${props.class ?? ''}`}
      data-testid="file-browser"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      {/* Toolbar */}
      <div class="file-browser__toolbar" data-testid="file-toolbar">
        <button
          class="file-browser__toolbar-btn"
          onClick={handleNavigateUp}
          title="Go to parent directory"
          data-testid="btn-navigate-up"
          disabled={store.state.currentPath === '/'}
        >
          Up
        </button>
        <button
          class="file-browser__toolbar-btn"
          onClick={handleRefresh}
          title="Refresh"
          data-testid="btn-refresh"
          disabled={store.state.isLoading}
        >
          Refresh
        </button>
        <button
          class="file-browser__toolbar-btn"
          onClick={handleToggleHidden}
          title={store.state.showHidden ? 'Hide hidden files' : 'Show hidden files'}
          data-testid="btn-toggle-hidden"
        >
          {store.state.showHidden ? 'Hide Hidden' : 'Show Hidden'}
        </button>
        <div class="file-browser__toolbar-separator" />
        <label class="file-browser__sort-label">
          Sort by:
          <select
            class="file-browser__sort-select"
            value={store.state.sortBy}
            onChange={handleSortChange}
            data-testid="sort-select"
          >
            <option value="name">Name</option>
            <option value="size">Size</option>
            <option value="modifiedAt">Date</option>
            <option value="type">Type</option>
          </select>
        </label>
        <button
          class="file-browser__toolbar-btn"
          onClick={handleToggleSortOrder}
          title={store.state.sortAscending ? 'Sort descending' : 'Sort ascending'}
          data-testid="btn-sort-order"
        >
          {store.state.sortAscending ? 'Asc' : 'Desc'}
        </button>
      </div>

      {/* Breadcrumb Navigation */}
      <Breadcrumb path={store.state.currentPath} onNavigate={store.navigate} />

      {/* Column Headers */}
      <div class="file-browser__header" role="row">
        <div class="file-browser__header-cell file-browser__header-cell--icon" />
        <div class="file-browser__header-cell file-browser__header-cell--name">Name</div>
        <div class="file-browser__header-cell file-browser__header-cell--size">Size</div>
        <div class="file-browser__header-cell file-browser__header-cell--date">Modified</div>
        <div class="file-browser__header-cell file-browser__header-cell--permissions">Permissions</div>
        <div class="file-browser__header-cell file-browser__header-cell--actions">Actions</div>
      </div>

      {/* File List */}
      <div
        class="file-browser__list"
        data-testid="file-list"
        role="grid"
        aria-label="File list"
        aria-live="polite"
      >
        <Show when={store.state.isLoading}>
          <div
            class="file-browser__loading"
            data-testid="file-loading"
            role="status"
            aria-live="polite"
          >
            Loading...
          </div>
        </Show>

        <Show when={store.state.error}>
          <div
            class="file-browser__error"
            data-testid="file-error"
            role="alert"
            aria-live="assertive"
          >
            Error: {store.state.error}
          </div>
        </Show>

        <Show when={!store.state.isLoading && !store.state.error}>
          <Show
            when={displayEntries().length > 0}
            fallback={
              <div class="file-browser__empty" data-testid="file-empty">
                This directory is empty
              </div>
            }
          >
            <For each={displayEntries()}>
              {(entry) => (
                <FileEntryRow
                  entry={entry}
                  isSelected={store.isSelected(entry.path)}
                  onSelect={handleSelect}
                  onActivate={handleActivate}
                  onDownload={handleDownload}
                />
              )}
            </For>
          </Show>
        </Show>
      </div>

      {/* Drag Overlay */}
      <Show when={isDragOver()}>
        <div class="file-browser__drop-overlay" data-testid="drop-overlay">
          <div class="file-browser__drop-message">
            Drop files here to upload
          </div>
        </div>
      </Show>

      {/* Validation Errors */}
      <Show when={validationErrors().length > 0}>
        <div class="file-browser__validation-errors" data-testid="validation-errors" role="alert">
          <div class="file-browser__validation-errors-header">
            <span class="file-browser__validation-errors-title">Upload Validation Errors</span>
            <button
              class="file-browser__dismiss-errors"
              onClick={() => setValidationErrors([])}
              data-testid="dismiss-errors"
              aria-label="Dismiss validation errors"
            >
              Dismiss
            </button>
          </div>
          <For each={validationErrors()}>
            {(error) => (
              <div class="file-browser__validation-error" data-testid={`validation-error-${error.reason}`}>
                <span class="file-browser__validation-error-icon">!</span>
                <span class="file-browser__validation-error-message">
                  {error.fileName ? `${error.fileName}: ` : ''}{error.message}
                </span>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Status Bar */}
      <div class="file-browser__status" data-testid="file-status">
        <span data-testid="file-count">
          {displayEntries().length} item{displayEntries().length !== 1 ? 's' : ''}
        </span>
        <Show when={store.state.selectedPaths.size > 0}>
          <span data-testid="selection-count">
            {' '}| {store.state.selectedPaths.size} selected
          </span>
        </Show>
      </div>
    </div>
  );
};

export default FileBrowser;
export { FileEntryRow, Breadcrumb, formatDate, getFileIcon, validateFiles };
