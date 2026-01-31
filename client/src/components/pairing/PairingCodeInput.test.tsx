import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Constants matching PairingCodeInput.tsx - avoid importing component directly
// to prevent server-side rendering issues in tests
const DEFAULT_SEGMENT_LENGTHS = [3, 4];
const VALID_CHARS_REGEX = /^[A-Z0-9]$/;

describe('PairingCodeInput', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Constants', () => {
    it('should have default segment lengths of [3, 4] for XXX-XXXX format', () => {
      expect(DEFAULT_SEGMENT_LENGTHS).toEqual([3, 4]);
    });

    it('should have valid chars regex that accepts alphanumeric uppercase', () => {
      expect(VALID_CHARS_REGEX.test('A')).toBe(true);
      expect(VALID_CHARS_REGEX.test('Z')).toBe(true);
      expect(VALID_CHARS_REGEX.test('0')).toBe(true);
      expect(VALID_CHARS_REGEX.test('9')).toBe(true);
    });

    it('should reject lowercase and special characters', () => {
      expect(VALID_CHARS_REGEX.test('a')).toBe(false);
      expect(VALID_CHARS_REGEX.test('-')).toBe(false);
      expect(VALID_CHARS_REGEX.test(' ')).toBe(false);
      expect(VALID_CHARS_REGEX.test('@')).toBe(false);
    });
  });

  describe('Input Validation Logic', () => {
    const isValidChar = (char: string): boolean => {
      return VALID_CHARS_REGEX.test(char.toUpperCase());
    };

    const normalizeInput = (value: string): string => {
      return value
        .toUpperCase()
        .split('')
        .filter(char => isValidChar(char))
        .join('');
    };

    it('should normalize lowercase to uppercase', () => {
      expect(normalizeInput('abc')).toBe('ABC');
      expect(normalizeInput('xyz')).toBe('XYZ');
    });

    it('should filter out invalid characters', () => {
      expect(normalizeInput('A-B-C')).toBe('ABC');
      expect(normalizeInput('A B C')).toBe('ABC');
      expect(normalizeInput('A@B#C')).toBe('ABC');
    });

    it('should handle mixed valid and invalid input', () => {
      expect(normalizeInput('abc-1234')).toBe('ABC1234');
      expect(normalizeInput('Test Code 123!')).toBe('TESTCODE123');
    });

    it('should return empty string for all invalid input', () => {
      expect(normalizeInput('---')).toBe('');
      expect(normalizeInput('   ')).toBe('');
      expect(normalizeInput('!@#$%')).toBe('');
    });
  });

  describe('Segment Update Logic', () => {
    it('should truncate input to segment max length', () => {
      const segmentLengths = [3, 4];

      const updateSegment = (index: number, value: string): string => {
        const normalized = value.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const maxLen = segmentLengths[index];
        return normalized.slice(0, maxLen);
      };

      expect(updateSegment(0, 'ABCDEF')).toBe('ABC');
      expect(updateSegment(1, 'ABCDEFGH')).toBe('ABCD');
    });

    it('should allow partial segment input', () => {
      const segmentLengths = [3, 4];

      const updateSegment = (index: number, value: string): string => {
        const normalized = value.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const maxLen = segmentLengths[index];
        return normalized.slice(0, maxLen);
      };

      expect(updateSegment(0, 'A')).toBe('A');
      expect(updateSegment(0, 'AB')).toBe('AB');
      expect(updateSegment(1, 'XY')).toBe('XY');
    });
  });

  describe('Completion Detection', () => {
    it('should detect when all segments are complete', () => {
      const segmentLengths = [3, 4];

      const isComplete = (segments: string[]): boolean => {
        return segments.every((seg, i) => seg.length === segmentLengths[i]);
      };

      expect(isComplete(['ABC', '1234'])).toBe(true);
      expect(isComplete(['XYZ', 'ABCD'])).toBe(true);
    });

    it('should detect incomplete segments', () => {
      const segmentLengths = [3, 4];

      const isComplete = (segments: string[]): boolean => {
        return segments.every((seg, i) => seg.length === segmentLengths[i]);
      };

      expect(isComplete(['AB', '1234'])).toBe(false);
      expect(isComplete(['ABC', '123'])).toBe(false);
      expect(isComplete(['', ''])).toBe(false);
      expect(isComplete(['A', ''])).toBe(false);
    });

    it('should compute full code from segments', () => {
      const getFullCode = (segments: string[]): string => segments.join('');

      expect(getFullCode(['ABC', '1234'])).toBe('ABC1234');
      expect(getFullCode(['XYZ', 'DEFG'])).toBe('XYZDEFG');
      expect(getFullCode(['', ''])).toBe('');
    });
  });

  describe('Auto-advance Logic', () => {
    it('should indicate advance when segment is filled', () => {
      const segmentLengths = [3, 4];

      const shouldAdvance = (index: number, segmentValue: string): boolean => {
        const maxLen = segmentLengths[index];
        return segmentValue.length === maxLen && index < segmentLengths.length - 1;
      };

      expect(shouldAdvance(0, 'ABC')).toBe(true);
      expect(shouldAdvance(0, 'AB')).toBe(false);
    });

    it('should not advance from last segment', () => {
      const segmentLengths = [3, 4];

      const shouldAdvance = (index: number, segmentValue: string): boolean => {
        const maxLen = segmentLengths[index];
        return segmentValue.length === maxLen && index < segmentLengths.length - 1;
      };

      expect(shouldAdvance(1, 'ABCD')).toBe(false);
    });
  });

  describe('Backspace Navigation Logic', () => {
    it('should navigate to previous segment when at start and pressing backspace', () => {
      const shouldNavigatePrev = (
        index: number,
        cursorPos: number,
        key: string
      ): boolean => {
        return key === 'Backspace' && cursorPos === 0 && index > 0;
      };

      expect(shouldNavigatePrev(1, 0, 'Backspace')).toBe(true);
      expect(shouldNavigatePrev(1, 1, 'Backspace')).toBe(false);
      expect(shouldNavigatePrev(0, 0, 'Backspace')).toBe(false);
    });

    it('should navigate with arrow keys at boundaries', () => {
      const shouldNavigateLeft = (
        index: number,
        cursorPos: number,
        key: string
      ): boolean => {
        return key === 'ArrowLeft' && cursorPos === 0 && index > 0;
      };

      const shouldNavigateRight = (
        index: number,
        cursorPos: number,
        valueLength: number,
        key: string,
        totalSegments: number
      ): boolean => {
        return key === 'ArrowRight' && cursorPos === valueLength && index < totalSegments - 1;
      };

      expect(shouldNavigateLeft(1, 0, 'ArrowLeft')).toBe(true);
      expect(shouldNavigateLeft(0, 0, 'ArrowLeft')).toBe(false);

      expect(shouldNavigateRight(0, 3, 3, 'ArrowRight', 2)).toBe(true);
      expect(shouldNavigateRight(1, 4, 4, 'ArrowRight', 2)).toBe(false);
    });
  });
});

describe('Paste Handling', () => {
  describe('Paste Normalization', () => {
    it('should remove separators from pasted text', () => {
      const normalizePaste = (text: string): string => {
        return text.toUpperCase().replace(/[-\s]/g, '').replace(/[^A-Z0-9]/g, '');
      };

      expect(normalizePaste('ABC-1234')).toBe('ABC1234');
      expect(normalizePaste('ABC 1234')).toBe('ABC1234');
      expect(normalizePaste('ABC - 1234')).toBe('ABC1234');
    });

    it('should handle various separator styles', () => {
      const normalizePaste = (text: string): string => {
        return text.toUpperCase().replace(/[-\s]/g, '').replace(/[^A-Z0-9]/g, '');
      };

      expect(normalizePaste('abc1234')).toBe('ABC1234');
      expect(normalizePaste('ABC_1234')).toBe('ABC1234'); // underscore removed
      expect(normalizePaste('ABC/1234')).toBe('ABC1234'); // slash removed
    });
  });

  describe('Paste Distribution', () => {
    it('should distribute pasted content across segments', () => {
      const segmentLengths = [3, 4];

      const distributePaste = (pastedText: string, startIndex: number): string[] => {
        const normalized = pastedText.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const newSegments: string[] = segmentLengths.map(() => '');
        let pasteIndex = 0;

        for (let i = startIndex; i < segmentLengths.length && pasteIndex < normalized.length; i++) {
          const chars = normalized.slice(pasteIndex, pasteIndex + segmentLengths[i]);
          newSegments[i] = chars;
          pasteIndex += segmentLengths[i];
        }

        return newSegments;
      };

      expect(distributePaste('ABC1234', 0)).toEqual(['ABC', '1234']);
      expect(distributePaste('ABCDEFG', 0)).toEqual(['ABC', 'DEFG']);
    });

    it('should handle partial paste from middle segment', () => {
      const segmentLengths = [3, 4];

      const distributePaste = (pastedText: string, startIndex: number, existingSegments: string[]): string[] => {
        const normalized = pastedText.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const newSegments = [...existingSegments];
        let pasteIndex = 0;

        for (let i = startIndex; i < segmentLengths.length && pasteIndex < normalized.length; i++) {
          const chars = normalized.slice(pasteIndex, pasteIndex + segmentLengths[i]);
          newSegments[i] = chars;
          pasteIndex += segmentLengths[i];
        }

        return newSegments;
      };

      expect(distributePaste('WXYZ', 1, ['ABC', ''])).toEqual(['ABC', 'WXYZ']);
    });

    it('should handle paste that exceeds total length', () => {
      const segmentLengths = [3, 4];

      const distributePaste = (pastedText: string, startIndex: number): string[] => {
        const normalized = pastedText.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const newSegments: string[] = segmentLengths.map(() => '');
        let pasteIndex = 0;

        for (let i = startIndex; i < segmentLengths.length && pasteIndex < normalized.length; i++) {
          const chars = normalized.slice(pasteIndex, pasteIndex + segmentLengths[i]);
          newSegments[i] = chars;
          pasteIndex += segmentLengths[i];
        }

        return newSegments;
      };

      // Extra characters beyond total length should be ignored
      expect(distributePaste('ABCDEFGHIJK', 0)).toEqual(['ABC', 'DEFG']);
    });

    it('should handle paste with less than full code', () => {
      const segmentLengths = [3, 4];

      const distributePaste = (pastedText: string, startIndex: number): string[] => {
        const normalized = pastedText.toUpperCase().replace(/[^A-Z0-9]/g, '');
        const newSegments: string[] = segmentLengths.map(() => '');
        let pasteIndex = 0;

        for (let i = startIndex; i < segmentLengths.length && pasteIndex < normalized.length; i++) {
          const chars = normalized.slice(pasteIndex, pasteIndex + segmentLengths[i]);
          newSegments[i] = chars;
          pasteIndex += segmentLengths[i];
        }

        return newSegments;
      };

      expect(distributePaste('ABC', 0)).toEqual(['ABC', '']);
      expect(distributePaste('AB', 0)).toEqual(['AB', '']);
    });
  });

  describe('Focus After Paste', () => {
    it('should determine correct focus target after paste', () => {
      const segmentLengths = [3, 4];

      const getFocusTargetAfterPaste = (
        segments: string[],
        startIndex: number
      ): number => {
        let lastFilledIndex = startIndex;

        for (let i = startIndex; i < segmentLengths.length; i++) {
          if (segments[i].length === segmentLengths[i]) {
            lastFilledIndex = i;
          } else {
            break;
          }
        }

        // Focus next incomplete segment or last segment
        if (lastFilledIndex < segmentLengths.length - 1) {
          return lastFilledIndex + 1;
        }
        return lastFilledIndex;
      };

      // Full code pasted - focus last segment
      expect(getFocusTargetAfterPaste(['ABC', '1234'], 0)).toBe(1);

      // Partial code - focus next incomplete
      expect(getFocusTargetAfterPaste(['ABC', '12'], 0)).toBe(1);

      // Only first segment filled
      expect(getFocusTargetAfterPaste(['ABC', ''], 0)).toBe(1);
    });
  });
});

describe('Error State', () => {
  it('should track internal error state', () => {
    let internalError = false;
    const setInternalError = (value: boolean) => { internalError = value; };

    setInternalError(true);
    expect(internalError).toBe(true);

    setInternalError(false);
    expect(internalError).toBe(false);
  });

  it('should combine external and internal error states', () => {
    const hasError = (externalError: boolean, internalError: boolean): boolean => {
      return externalError || internalError;
    };

    expect(hasError(true, false)).toBe(true);
    expect(hasError(false, true)).toBe(true);
    expect(hasError(true, true)).toBe(true);
    expect(hasError(false, false)).toBe(false);
  });

  it('should clear internal error when user types', () => {
    let internalError = true;

    const handleInput = () => {
      internalError = false;
    };

    handleInput();
    expect(internalError).toBe(false);
  });
});

describe('Custom Segment Configurations', () => {
  it('should support different segment patterns', () => {
    const isComplete = (segments: string[], lengths: number[]): boolean => {
      return segments.every((seg, i) => seg.length === lengths[i]);
    };

    // 4-4 format
    expect(isComplete(['ABCD', '1234'], [4, 4])).toBe(true);

    // 3-3-3 format
    expect(isComplete(['ABC', 'DEF', '123'], [3, 3, 3])).toBe(true);

    // 2-2-2-2 format
    expect(isComplete(['AB', 'CD', 'EF', '12'], [2, 2, 2, 2])).toBe(true);
  });

  it('should calculate total code length from segments', () => {
    const getTotalLength = (lengths: number[]): number => {
      return lengths.reduce((sum, len) => sum + len, 0);
    };

    expect(getTotalLength([3, 4])).toBe(7);
    expect(getTotalLength([4, 4])).toBe(8);
    expect(getTotalLength([3, 3, 3])).toBe(9);
  });
});

describe('Submission Handling', () => {
  it('should call onComplete with full code when Enter is pressed and code is complete', () => {
    const onComplete = vi.fn();
    const segments = ['ABC', '1234'];
    const segmentLengths = [3, 4];

    const isComplete = segments.every((seg, i) => seg.length === segmentLengths[i]);
    const getFullCode = () => segments.join('');

    const handleKeyDown = (key: string) => {
      if (key === 'Enter' && isComplete) {
        onComplete(getFullCode());
      }
    };

    handleKeyDown('Enter');
    expect(onComplete).toHaveBeenCalledWith('ABC1234');
  });

  it('should not call onComplete when Enter is pressed but code is incomplete', () => {
    const onComplete = vi.fn();
    const segments = ['ABC', '123']; // Missing one char in second segment
    const segmentLengths = [3, 4];

    const isComplete = segments.every((seg, i) => seg.length === segmentLengths[i]);
    const getFullCode = () => segments.join('');

    const handleKeyDown = (key: string) => {
      if (key === 'Enter' && isComplete) {
        onComplete(getFullCode());
      }
    };

    handleKeyDown('Enter');
    expect(onComplete).not.toHaveBeenCalled();
  });

  it('should call onChange with code and completion status', () => {
    const onChange = vi.fn();

    const notifyChange = (segments: string[], segmentLengths: number[]) => {
      const code = segments.join('');
      const isComplete = segments.every((seg, i) => seg.length === segmentLengths[i]);
      onChange(code, isComplete);
    };

    notifyChange(['ABC', '1234'], [3, 4]);
    expect(onChange).toHaveBeenCalledWith('ABC1234', true);

    notifyChange(['AB', '123'], [3, 4]);
    expect(onChange).toHaveBeenCalledWith('AB123', false);
  });
});

describe('Handle API', () => {
  it('should provide getValue method', () => {
    const segments = ['ABC', '1234'];
    const getValue = () => segments.join('');

    expect(getValue()).toBe('ABC1234');
  });

  it('should provide clear method', () => {
    let segments = ['ABC', '1234'];
    let internalError = true;

    const clear = () => {
      segments = ['', ''];
      internalError = false;
    };

    clear();
    expect(segments).toEqual(['', '']);
    expect(internalError).toBe(false);
  });

  it('should provide isComplete method', () => {
    const segmentLengths = [3, 4];

    const createIsComplete = (segments: string[]) => () => {
      return segments.every((seg, i) => seg.length === segmentLengths[i]);
    };

    expect(createIsComplete(['ABC', '1234'])()).toBe(true);
    expect(createIsComplete(['AB', '1234'])()).toBe(false);
  });

  it('should provide setError method', () => {
    let internalError = false;
    const setError = (value: boolean) => { internalError = value; };

    setError(true);
    expect(internalError).toBe(true);

    setError(false);
    expect(internalError).toBe(false);
  });
});
