import { Component, For } from 'solid-js';

/**
 * Skip link target configuration
 */
export interface SkipLinkTarget {
  /** Target element ID (without #) */
  id: string;
  /** Label for the skip link */
  label: string;
}

/**
 * Default skip link targets
 */
export const defaultSkipLinkTargets: SkipLinkTarget[] = [
  { id: 'main-content', label: 'Skip to main content' },
  { id: 'main-navigation', label: 'Skip to navigation' },
];

export interface SkipLinkProps {
  /** Skip link targets */
  targets?: SkipLinkTarget[];
  /** Additional CSS class */
  class?: string;
}

/**
 * SkipLink component for keyboard-only users.
 * Hidden until focused, appears at the top of the page.
 * Allows users to skip repetitive navigation and jump to main content.
 */
const SkipLink: Component<SkipLinkProps> = (props) => {
  const targets = () => props.targets ?? defaultSkipLinkTargets;

  const handleClick = (targetId: string, e: MouseEvent) => {
    e.preventDefault();
    const target = document.getElementById(targetId);
    if (target) {
      // Make target focusable if it isn't already
      if (!target.hasAttribute('tabindex')) {
        target.setAttribute('tabindex', '-1');
      }
      target.focus();
      // Scroll into view
      target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  };

  const handleKeyDown = (targetId: string, e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      const target = document.getElementById(targetId);
      if (target) {
        if (!target.hasAttribute('tabindex')) {
          target.setAttribute('tabindex', '-1');
        }
        target.focus();
        target.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    }
  };

  return (
    <div class={`skip-links ${props.class ?? ''}`} data-testid="skip-links">
      <For each={targets()}>
        {(target) => (
          <a
            href={`#${target.id}`}
            class="skip-link"
            data-testid={`skip-link-${target.id}`}
            onClick={(e) => handleClick(target.id, e)}
            onKeyDown={(e) => handleKeyDown(target.id, e)}
          >
            {target.label}
          </a>
        )}
      </For>
    </div>
  );
};

export default SkipLink;
export { SkipLink };
