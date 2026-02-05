import { Component, createSignal, createEffect, For, onMount } from 'solid-js';

/** Default segment pattern: XXXX-XXXX (4 chars, hyphen, 4 chars) */
const DEFAULT_SEGMENT_LENGTHS = [4, 4];

/** Valid characters for pairing codes (alphanumeric, uppercase) */
const VALID_CHARS_REGEX = /^[A-Z0-9]$/;

export interface PairingCodeInputProps {
  /** Segment lengths, e.g., [3, 4] for XXX-XXXX format */
  segmentLengths?: number[];
  /** Separator character between segments (visual only) */
  separator?: string;
  /** Called when a complete valid code is entered */
  onComplete?: (code: string) => void;
  /** Called when code changes (partial or complete) */
  onChange?: (code: string, isComplete: boolean) => void;
  /** External error state */
  error?: boolean;
  /** Error message to display */
  errorMessage?: string;
  /** Disable the input */
  disabled?: boolean;
  /** Auto-focus first segment on mount */
  autoFocus?: boolean;
  /** Additional CSS class for the container */
  class?: string;
}

export interface PairingCodeInputHandle {
  /** Get the current code value */
  getValue: () => string;
  /** Clear all segments */
  clear: () => void;
  /** Focus the first segment */
  focus: () => void;
  /** Check if the code is complete */
  isComplete: () => boolean;
  /** Set an error state programmatically */
  setError: (hasError: boolean) => void;
}

/**
 * Split-segment pairing code entry component.
 *
 * Renders multiple input segments with automatic advance on completion,
 * backspace navigation, and paste handling.
 */
const PairingCodeInput: Component<PairingCodeInputProps> = (props) => {
  const segmentLengths = () => props.segmentLengths ?? DEFAULT_SEGMENT_LENGTHS;
  const separator = () => props.separator ?? '-';

  // Create signals for each segment
  const [segments, setSegments] = createSignal<string[]>(
    segmentLengths().map(() => '')
  );
  const [internalError, setInternalError] = createSignal(false);
  const [focusedSegment, setFocusedSegment] = createSignal<number | null>(null);

  // References to input elements
  let inputRefs: HTMLInputElement[] = [];

  // Compute the full code from segments
  const getFullCode = () => segments().join('');

  // Check if all segments are complete
  const isComplete = () => {
    const segs = segments();
    const lengths = segmentLengths();
    return segs.every((seg, i) => seg.length === lengths[i]);
  };

  // Validate a single character
  const isValidChar = (char: string): boolean => {
    return VALID_CHARS_REGEX.test(char.toUpperCase());
  };

  // Normalize input to uppercase
  const normalizeInput = (value: string): string => {
    return value
      .toUpperCase()
      .split('')
      .filter(char => isValidChar(char))
      .join('');
  };

  // Update a specific segment
  const updateSegment = (index: number, value: string) => {
    const normalized = normalizeInput(value);
    const maxLen = segmentLengths()[index];
    const truncated = normalized.slice(0, maxLen);

    setSegments(prev => {
      const newSegments = [...prev];
      newSegments[index] = truncated;
      return newSegments;
    });

    return truncated;
  };

  // Focus a specific segment
  const focusSegment = (index: number) => {
    if (index >= 0 && index < inputRefs.length && inputRefs[index]) {
      inputRefs[index].focus();
    }
  };

  // Handle input change
  const handleInput = (index: number, e: InputEvent) => {
    const target = e.target as HTMLInputElement;
    const value = target.value;
    const updated = updateSegment(index, value);

    // Auto-advance to next segment when current is filled
    if (updated.length === segmentLengths()[index] && index < segmentLengths().length - 1) {
      focusSegment(index + 1);
    }

    // Clear internal error when user types
    setInternalError(false);
  };

  // Handle keydown for navigation
  const handleKeyDown = (index: number, e: KeyboardEvent) => {
    const target = e.target as HTMLInputElement;

    // Backspace at beginning of segment navigates to previous
    if (e.key === 'Backspace' && target.selectionStart === 0 && target.selectionEnd === 0) {
      if (index > 0) {
        e.preventDefault();
        const prevInput = inputRefs[index - 1];
        if (prevInput) {
          prevInput.focus();
          // Place cursor at end
          prevInput.setSelectionRange(prevInput.value.length, prevInput.value.length);
        }
      }
    }

    // Left arrow at beginning navigates to previous segment
    if (e.key === 'ArrowLeft' && target.selectionStart === 0) {
      if (index > 0) {
        e.preventDefault();
        const prevInput = inputRefs[index - 1];
        if (prevInput) {
          prevInput.focus();
          prevInput.setSelectionRange(prevInput.value.length, prevInput.value.length);
        }
      }
    }

    // Right arrow at end navigates to next segment
    if (e.key === 'ArrowRight' && target.selectionStart === target.value.length) {
      if (index < segmentLengths().length - 1) {
        e.preventDefault();
        const nextInput = inputRefs[index + 1];
        if (nextInput) {
          nextInput.focus();
          nextInput.setSelectionRange(0, 0);
        }
      }
    }

    // Enter key triggers submit if complete
    if (e.key === 'Enter' && isComplete()) {
      props.onComplete?.(getFullCode());
    }
  };

  // Handle paste - distribute pasted content across segments
  const handlePaste = (index: number, e: ClipboardEvent) => {
    e.preventDefault();

    const pastedText = e.clipboardData?.getData('text') || '';
    // Remove any separators and normalize
    const normalizedPaste = normalizeInput(pastedText.replace(/[-\s]/g, ''));

    if (normalizedPaste.length === 0) return;

    // Distribute pasted content across segments starting from current
    const newSegments = [...segments()];
    const lengths = segmentLengths();
    let pasteIndex = 0;

    for (let i = index; i < lengths.length && pasteIndex < normalizedPaste.length; i++) {
      const chars = normalizedPaste.slice(pasteIndex, pasteIndex + lengths[i]);
      newSegments[i] = chars;
      pasteIndex += lengths[i];
    }

    setSegments(newSegments);
    setInternalError(false);

    // Focus the appropriate segment after paste
    let lastFilledIndex = index;
    for (let i = index; i < lengths.length; i++) {
      if (newSegments[i].length === lengths[i]) {
        lastFilledIndex = i;
      } else {
        break;
      }
    }

    // Focus next incomplete segment or last segment
    if (lastFilledIndex < lengths.length - 1) {
      focusSegment(lastFilledIndex + 1);
    } else {
      focusSegment(lastFilledIndex);
    }
  };

  // Handle focus tracking
  const handleFocus = (index: number) => {
    setFocusedSegment(index);
  };

  const handleBlur = () => {
    setFocusedSegment(null);
  };

  // Effect to notify changes
  createEffect(() => {
    const code = getFullCode();
    const complete = isComplete();
    props.onChange?.(code, complete);

    // Auto-submit on completion if onComplete is provided
    if (complete && focusedSegment() === segmentLengths().length - 1) {
      // Small delay to allow UI to update
      setTimeout(() => {
        if (isComplete()) {
          props.onComplete?.(getFullCode());
        }
      }, 100);
    }
  });

  // Reset segments when segment lengths change
  createEffect(() => {
    const lengths = segmentLengths();
    setSegments(lengths.map(() => ''));
    inputRefs = [];
  });

  // Auto-focus on mount
  onMount(() => {
    if (props.autoFocus && inputRefs[0]) {
      inputRefs[0].focus();
    }
  });

  // Expose handle
  const handle: PairingCodeInputHandle = {
    getValue: getFullCode,
    clear: () => {
      setSegments(segmentLengths().map(() => ''));
      setInternalError(false);
      focusSegment(0);
    },
    focus: () => focusSegment(0),
    isComplete,
    setError: setInternalError,
  };

  // Attach handle to component
  (PairingCodeInput as any).handle = handle;

  const hasError = () => props.error || internalError();

  return (
    <div
      class={`pairing-code-input ${props.class ?? ''} ${hasError() ? 'pairing-code-input--error' : ''} ${props.disabled ? 'pairing-code-input--disabled' : ''}`}
      data-testid="pairing-code-input"
    >
      <div class="pairing-code-input__segments">
        <For each={segmentLengths()}>
          {(length, index) => (
            <>
              {index() > 0 && (
                <span class="pairing-code-input__separator" aria-hidden="true">
                  {separator()}
                </span>
              )}
              <input
                ref={(el) => { inputRefs[index()] = el; }}
                type="text"
                inputmode="text"
                autocomplete="off"
                autocapitalize="characters"
                spellcheck={false}
                maxLength={length}
                value={segments()[index()]}
                disabled={props.disabled}
                class={`pairing-code-input__segment ${
                  focusedSegment() === index() ? 'pairing-code-input__segment--focused' : ''
                } ${hasError() ? 'pairing-code-input__segment--error' : ''}`}
                style={{ width: `${length * 1.5 + 0.5}em` }}
                aria-label={`Pairing code segment ${index() + 1} of ${segmentLengths().length}`}
                data-testid={`segment-${index()}`}
                onInput={(e) => handleInput(index(), e)}
                onKeyDown={(e) => handleKeyDown(index(), e)}
                onPaste={(e) => handlePaste(index(), e)}
                onFocus={() => handleFocus(index())}
                onBlur={handleBlur}
              />
            </>
          )}
        </For>
      </div>
      {hasError() && props.errorMessage && (
        <div class="pairing-code-input__error" role="alert" aria-live="polite">
          {props.errorMessage}
        </div>
      )}
    </div>
  );
};

export default PairingCodeInput;
export { DEFAULT_SEGMENT_LENGTHS, VALID_CHARS_REGEX };
