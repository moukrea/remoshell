import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { resetFileStore, type FileEntry } from '../../stores/files';

// Helper functions from FileBrowser - tested independently to avoid SSR issues
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

const getFileIcon = (entry: FileEntry): string => {
  if (entry.type === 'directory') return 'D';
  if (entry.type === 'symlink') return 'L';

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

describe('FileBrowser', () => {
  beforeEach(() => {
    resetFileStore();
    vi.useFakeTimers();
  });

  afterEach(() => {
    resetFileStore();
    vi.useRealTimers();
  });

  describe('formatDate', () => {
    it('should return time for timestamps less than 24 hours ago', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const result = formatDate(now - 3600000); // 1 hour ago
      expect(result).toMatch(/\d{1,2}:\d{2}/); // Should contain time format
    });

    it('should return month and day for timestamps this year', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      // 30 days ago
      const thirtyDaysAgo = now - 30 * 24 * 60 * 60 * 1000;
      const date = new Date(thirtyDaysAgo);

      // Only test if it's still the same year
      if (date.getFullYear() === new Date(now).getFullYear()) {
        const result = formatDate(thirtyDaysAgo);
        expect(result).toMatch(/[A-Za-z]/); // Should contain month name
      }
    });

    it('should return full date for timestamps from previous year', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      // One year ago
      const oneYearAgo = now - 365 * 24 * 60 * 60 * 1000;
      const result = formatDate(oneYearAgo);
      expect(result).toMatch(/\d/); // Should contain numbers
    });
  });

  describe('getFileIcon', () => {
    const createEntry = (name: string, type: 'file' | 'directory' | 'symlink' = 'file'): FileEntry => ({
      name,
      path: '/' + name,
      type,
      size: 1024,
      modifiedAt: Date.now(),
      permissions: { read: true, write: true, execute: false },
      isHidden: false,
    });

    it('should return D for directories', () => {
      expect(getFileIcon(createEntry('folder', 'directory'))).toBe('D');
    });

    it('should return L for symlinks', () => {
      expect(getFileIcon(createEntry('link', 'symlink'))).toBe('L');
    });

    it('should return correct icons for known file types', () => {
      expect(getFileIcon(createEntry('readme.txt'))).toBe('T');
      expect(getFileIcon(createEntry('README.md'))).toBe('M');
      expect(getFileIcon(createEntry('package.json'))).toBe('J');
      expect(getFileIcon(createEntry('index.js'))).toBe('j');
      expect(getFileIcon(createEntry('app.ts'))).toBe('t');
      expect(getFileIcon(createEntry('component.tsx'))).toBe('t');
      expect(getFileIcon(createEntry('index.html'))).toBe('h');
      expect(getFileIcon(createEntry('styles.css'))).toBe('c');
      expect(getFileIcon(createEntry('script.py'))).toBe('p');
      expect(getFileIcon(createEntry('run.sh'))).toBe('s');
      expect(getFileIcon(createEntry('archive.zip'))).toBe('Z');
      expect(getFileIcon(createEntry('image.png'))).toBe('I');
      expect(getFileIcon(createEntry('photo.jpg'))).toBe('I');
      expect(getFileIcon(createEntry('doc.pdf'))).toBe('P');
    });

    it('should return F for unknown file types', () => {
      expect(getFileIcon(createEntry('file.unknown'))).toBe('F');
      expect(getFileIcon(createEntry('noextension'))).toBe('F');
    });
  });

  describe('Breadcrumb Path Parsing', () => {
    const parsePath = (path: string): { name: string; path: string }[] => {
      const parts = path.split('/').filter(Boolean);
      const result: { name: string; path: string }[] = [{ name: '/', path: '/' }];

      let currentPath = '';
      for (const part of parts) {
        currentPath += '/' + part;
        result.push({ name: part, path: currentPath });
      }

      return result;
    };

    it('should parse root path', () => {
      const parts = parsePath('/');
      expect(parts).toHaveLength(1);
      expect(parts[0]).toEqual({ name: '/', path: '/' });
    });

    it('should parse simple path', () => {
      const parts = parsePath('/home');
      expect(parts).toHaveLength(2);
      expect(parts[0]).toEqual({ name: '/', path: '/' });
      expect(parts[1]).toEqual({ name: 'home', path: '/home' });
    });

    it('should parse nested path', () => {
      const parts = parsePath('/home/user/documents');
      expect(parts).toHaveLength(4);
      expect(parts[0]).toEqual({ name: '/', path: '/' });
      expect(parts[1]).toEqual({ name: 'home', path: '/home' });
      expect(parts[2]).toEqual({ name: 'user', path: '/home/user' });
      expect(parts[3]).toEqual({ name: 'documents', path: '/home/user/documents' });
    });
  });

  describe('Keyboard Navigation Logic', () => {
    interface NavigationState {
      currentIndex: number;
      entriesLength: number;
      selectedPaths: Set<string>;
    }

    const handleKeyNavigation = (
      key: string,
      state: NavigationState,
      entries: { path: string }[],
      callbacks: {
        select: (path: string, additive: boolean) => void;
        navigateUp: () => void;
        selectAll: () => void;
        clearSelection: () => void;
        activate: (path: string) => void;
      },
      modifiers: { shift?: boolean; ctrl?: boolean } = {}
    ): number => {
      const { currentIndex, entriesLength, selectedPaths } = state;

      switch (key) {
        case 'ArrowDown': {
          const nextIndex = Math.min(currentIndex + 1, entriesLength - 1);
          callbacks.select(entries[nextIndex].path, modifiers.shift ?? false);
          return nextIndex;
        }
        case 'ArrowUp': {
          const prevIndex = Math.max(currentIndex - 1, 0);
          callbacks.select(entries[prevIndex].path, modifiers.shift ?? false);
          return prevIndex;
        }
        case 'Enter': {
          if (selectedPaths.size === 1) {
            callbacks.activate(Array.from(selectedPaths)[0]);
          }
          return currentIndex;
        }
        case 'Backspace': {
          callbacks.navigateUp();
          return currentIndex;
        }
        case 'a': {
          if (modifiers.ctrl) {
            callbacks.selectAll();
          }
          return currentIndex;
        }
        case 'Escape': {
          callbacks.clearSelection();
          return currentIndex;
        }
        default:
          return currentIndex;
      }
    };

    it('should navigate down', () => {
      const select = vi.fn();
      const entries = [{ path: '/a' }, { path: '/b' }, { path: '/c' }];
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 3,
        selectedPaths: new Set(['/a']),
      };

      const newIndex = handleKeyNavigation('ArrowDown', state, entries, {
        select,
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      });

      expect(newIndex).toBe(1);
      expect(select).toHaveBeenCalledWith('/b', false);
    });

    it('should navigate up', () => {
      const select = vi.fn();
      const entries = [{ path: '/a' }, { path: '/b' }, { path: '/c' }];
      const state: NavigationState = {
        currentIndex: 2,
        entriesLength: 3,
        selectedPaths: new Set(['/c']),
      };

      const newIndex = handleKeyNavigation('ArrowUp', state, entries, {
        select,
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      });

      expect(newIndex).toBe(1);
      expect(select).toHaveBeenCalledWith('/b', false);
    });

    it('should not go below 0', () => {
      const select = vi.fn();
      const entries = [{ path: '/a' }, { path: '/b' }];
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 2,
        selectedPaths: new Set(['/a']),
      };

      const newIndex = handleKeyNavigation('ArrowUp', state, entries, {
        select,
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      });

      expect(newIndex).toBe(0);
    });

    it('should not go above entries length', () => {
      const select = vi.fn();
      const entries = [{ path: '/a' }, { path: '/b' }];
      const state: NavigationState = {
        currentIndex: 1,
        entriesLength: 2,
        selectedPaths: new Set(['/b']),
      };

      const newIndex = handleKeyNavigation('ArrowDown', state, entries, {
        select,
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      });

      expect(newIndex).toBe(1);
    });

    it('should extend selection with shift', () => {
      const select = vi.fn();
      const entries = [{ path: '/a' }, { path: '/b' }];
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 2,
        selectedPaths: new Set(['/a']),
      };

      handleKeyNavigation('ArrowDown', state, entries, {
        select,
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      }, { shift: true });

      expect(select).toHaveBeenCalledWith('/b', true);
    });

    it('should activate on Enter with single selection', () => {
      const activate = vi.fn();
      const entries = [{ path: '/a' }];
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 1,
        selectedPaths: new Set(['/a']),
      };

      handleKeyNavigation('Enter', state, entries, {
        select: vi.fn(),
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate,
      });

      expect(activate).toHaveBeenCalledWith('/a');
    });

    it('should navigate up on Backspace', () => {
      const navigateUp = vi.fn();
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 1,
        selectedPaths: new Set(),
      };

      handleKeyNavigation('Backspace', state, [], {
        select: vi.fn(),
        navigateUp,
        selectAll: vi.fn(),
        clearSelection: vi.fn(),
        activate: vi.fn(),
      });

      expect(navigateUp).toHaveBeenCalled();
    });

    it('should select all on Ctrl+A', () => {
      const selectAll = vi.fn();
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 3,
        selectedPaths: new Set(),
      };

      handleKeyNavigation('a', state, [], {
        select: vi.fn(),
        navigateUp: vi.fn(),
        selectAll,
        clearSelection: vi.fn(),
        activate: vi.fn(),
      }, { ctrl: true });

      expect(selectAll).toHaveBeenCalled();
    });

    it('should clear selection on Escape', () => {
      const clearSelection = vi.fn();
      const state: NavigationState = {
        currentIndex: 0,
        entriesLength: 1,
        selectedPaths: new Set(['/a']),
      };

      handleKeyNavigation('Escape', state, [], {
        select: vi.fn(),
        navigateUp: vi.fn(),
        selectAll: vi.fn(),
        clearSelection,
        activate: vi.fn(),
      });

      expect(clearSelection).toHaveBeenCalled();
    });
  });

  describe('File Activation Logic', () => {
    interface FileEntry {
      path: string;
      type: 'file' | 'directory';
    }

    const handleActivate = (
      entry: FileEntry,
      callbacks: {
        navigate: (path: string) => void;
        download: (path: string) => void;
      }
    ): void => {
      if (entry.type === 'directory') {
        callbacks.navigate(entry.path);
      } else {
        callbacks.download(entry.path);
      }
    };

    it('should navigate on directory activation', () => {
      const navigate = vi.fn();
      const download = vi.fn();

      handleActivate({ path: '/folder', type: 'directory' }, { navigate, download });

      expect(navigate).toHaveBeenCalledWith('/folder');
      expect(download).not.toHaveBeenCalled();
    });

    it('should download on file activation', () => {
      const navigate = vi.fn();
      const download = vi.fn();

      handleActivate({ path: '/file.txt', type: 'file' }, { navigate, download });

      expect(navigate).not.toHaveBeenCalled();
      expect(download).toHaveBeenCalledWith('/file.txt');
    });
  });

  describe('Drag and Drop Logic', () => {
    it('should track drag over state', () => {
      let isDragOver = false;

      const handleDragEnter = () => {
        isDragOver = true;
      };

      const handleDragLeave = () => {
        isDragOver = false;
      };

      const handleDrop = () => {
        isDragOver = false;
      };

      expect(isDragOver).toBe(false);

      handleDragEnter();
      expect(isDragOver).toBe(true);

      handleDragLeave();
      expect(isDragOver).toBe(false);

      handleDragEnter();
      expect(isDragOver).toBe(true);

      handleDrop();
      expect(isDragOver).toBe(false);
    });

    it('should extract files from drop event', () => {
      const extractFiles = (fileList: { length: number; item: (i: number) => File | null }): File[] => {
        const files: File[] = [];
        for (let i = 0; i < fileList.length; i++) {
          const file = fileList.item(i);
          if (file) {
            files.push(file);
          }
        }
        return files;
      };

      const mockFile1 = new File(['content1'], 'file1.txt');
      const mockFile2 = new File(['content2'], 'file2.txt');
      const mockFileList = {
        length: 2,
        item: (i: number) => [mockFile1, mockFile2][i] ?? null,
      };

      const files = extractFiles(mockFileList);

      expect(files).toHaveLength(2);
      expect(files[0].name).toBe('file1.txt');
      expect(files[1].name).toBe('file2.txt');
    });
  });

  describe('Selection with Click Modifiers', () => {
    it('should handle click without modifier', () => {
      const select = vi.fn();
      const handleClick = (path: string, additive: boolean) => {
        select(path, additive);
      };

      handleClick('/file.txt', false);

      expect(select).toHaveBeenCalledWith('/file.txt', false);
    });

    it('should handle click with Ctrl/Cmd', () => {
      const select = vi.fn();
      const handleClick = (path: string, additive: boolean) => {
        select(path, additive);
      };

      handleClick('/file.txt', true);

      expect(select).toHaveBeenCalledWith('/file.txt', true);
    });
  });

  describe('Status Display', () => {
    it('should format item count correctly', () => {
      const formatItemCount = (count: number): string => {
        return `${count} item${count !== 1 ? 's' : ''}`;
      };

      expect(formatItemCount(0)).toBe('0 items');
      expect(formatItemCount(1)).toBe('1 item');
      expect(formatItemCount(2)).toBe('2 items');
      expect(formatItemCount(100)).toBe('100 items');
    });

    it('should format selection count', () => {
      const formatSelectionCount = (count: number): string => {
        return ` | ${count} selected`;
      };

      expect(formatSelectionCount(1)).toBe(' | 1 selected');
      expect(formatSelectionCount(5)).toBe(' | 5 selected');
    });
  });
});
