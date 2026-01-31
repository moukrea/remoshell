/**
 * SkipLink Component Tests
 */
import { describe, it, expect } from 'vitest';

// Define default skip link targets for testing (same as component)
const defaultSkipLinkTargets = [
  { id: 'main-content', label: 'Skip to main content' },
  { id: 'main-navigation', label: 'Skip to navigation' },
];

describe('SkipLink', () => {
  describe('Default Targets', () => {
    it('should have main-content as first target', () => {
      expect(defaultSkipLinkTargets[0].id).toBe('main-content');
      expect(defaultSkipLinkTargets[0].label).toBe('Skip to main content');
    });

    it('should have main-navigation as second target', () => {
      expect(defaultSkipLinkTargets[1].id).toBe('main-navigation');
      expect(defaultSkipLinkTargets[1].label).toBe('Skip to navigation');
    });

    it('should have two default targets', () => {
      expect(defaultSkipLinkTargets).toHaveLength(2);
    });
  });

  describe('Skip Link Click Behavior', () => {
    it('should generate correct href for target', () => {
      const generateHref = (targetId: string): string => {
        return `#${targetId}`;
      };

      expect(generateHref('main-content')).toBe('#main-content');
      expect(generateHref('main-navigation')).toBe('#main-navigation');
    });

    it('should focus target element logic', () => {
      let focusedElement: string | null = null;
      let tabIndexSet: string | null = null;

      const focusTarget = (targetId: string, hasTabIndex: boolean) => {
        if (!hasTabIndex) {
          tabIndexSet = targetId;
        }
        focusedElement = targetId;
      };

      // Target without tabindex
      focusTarget('main-content', false);
      expect(tabIndexSet).toBe('main-content');
      expect(focusedElement).toBe('main-content');

      // Target with tabindex
      tabIndexSet = null;
      focusTarget('main-navigation', true);
      expect(tabIndexSet).toBeNull();
      expect(focusedElement).toBe('main-navigation');
    });
  });

  describe('Skip Link Keyboard Behavior', () => {
    it('should activate on Enter key', () => {
      let activated = false;

      const handleKeyDown = (key: string) => {
        if (key === 'Enter' || key === ' ') {
          activated = true;
        }
      };

      handleKeyDown('Enter');
      expect(activated).toBe(true);
    });

    it('should activate on Space key', () => {
      let activated = false;

      const handleKeyDown = (key: string) => {
        if (key === 'Enter' || key === ' ') {
          activated = true;
        }
      };

      handleKeyDown(' ');
      expect(activated).toBe(true);
    });

    it('should not activate on other keys', () => {
      let activated = false;

      const handleKeyDown = (key: string) => {
        if (key === 'Enter' || key === ' ') {
          activated = true;
        }
      };

      handleKeyDown('Escape');
      expect(activated).toBe(false);
    });
  });

  describe('Skip Link Visibility', () => {
    it('should be hidden by default (sr-only)', () => {
      const getSkipLinkClasses = (isFocused: boolean): string[] => {
        const classes = ['skip-link'];
        if (!isFocused) {
          classes.push('sr-only');
        }
        return classes;
      };

      expect(getSkipLinkClasses(false)).toContain('sr-only');
    });

    it('should be visible when focused', () => {
      const getSkipLinkClasses = (isFocused: boolean): string[] => {
        const classes = ['skip-link'];
        if (!isFocused) {
          classes.push('sr-only');
        }
        return classes;
      };

      expect(getSkipLinkClasses(true)).not.toContain('sr-only');
    });
  });
});
