/**
 * Accessibility testing utilities using axe-core.
 * Provides helper functions for running WCAG 2.1 AA compliance checks.
 */
import type { Result, NodeResult, ImpactValue } from 'axe-core';

/**
 * Axe violation result with formatted information
 */
export interface A11yViolation {
  id: string;
  impact: ImpactValue | undefined;
  description: string;
  help: string;
  helpUrl: string;
  nodes: Array<{
    html: string;
    target: string[];
    failureSummary: string | undefined;
  }>;
}

/**
 * Convert axe-core results to a more readable format
 */
export function formatViolations(violations: Result[]): A11yViolation[] {
  return violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact,
    description: violation.description,
    help: violation.help,
    helpUrl: violation.helpUrl,
    nodes: violation.nodes.map((node: NodeResult) => ({
      html: node.html,
      target: node.target as string[],
      failureSummary: node.failureSummary,
    })),
  }));
}

/**
 * Generate a readable report from violations
 */
export function generateA11yReport(violations: A11yViolation[]): string {
  if (violations.length === 0) {
    return 'No accessibility violations found.';
  }

  const lines: string[] = [
    `Found ${violations.length} accessibility violation(s):`,
    '',
  ];

  violations.forEach((violation, index) => {
    lines.push(`${index + 1}. [${violation.impact?.toUpperCase() || 'UNKNOWN'}] ${violation.id}`);
    lines.push(`   ${violation.description}`);
    lines.push(`   Help: ${violation.help}`);
    lines.push(`   More info: ${violation.helpUrl}`);
    lines.push(`   Affected nodes:`);
    violation.nodes.forEach((node) => {
      lines.push(`     - ${node.target.join(' > ')}`);
      lines.push(`       HTML: ${node.html.substring(0, 100)}${node.html.length > 100 ? '...' : ''}`);
      if (node.failureSummary) {
        lines.push(`       Fix: ${node.failureSummary}`);
      }
    });
    lines.push('');
  });

  return lines.join('\n');
}

/**
 * Run axe-core on a DOM element and return violations
 */
export async function runAxe(element: Element | Document = document): Promise<A11yViolation[]> {
  // Dynamically import axe-core to avoid issues with SSR
  const axe = await import('axe-core');

  const results = await axe.default.run(element as any, {
    // WCAG 2.1 AA compliance
    runOnly: {
      type: 'tag',
      values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'],
    },
  });

  return formatViolations(results.violations);
}

/**
 * Assert that an element has no accessibility violations
 * Throws an error with a detailed report if violations are found
 */
export async function assertNoA11yViolations(
  element: Element | Document = document,
  options?: { ignore?: string[] }
): Promise<void> {
  const violations = await runAxe(element);

  // Filter out ignored violations if specified
  const filteredViolations = options?.ignore
    ? violations.filter((v) => !options.ignore!.includes(v.id))
    : violations;

  if (filteredViolations.length > 0) {
    const report = generateA11yReport(filteredViolations);
    throw new Error(`Accessibility violations found:\n\n${report}`);
  }
}

/**
 * Check if element has required ARIA attributes
 */
export function hasAriaLabel(element: Element): boolean {
  return (
    element.hasAttribute('aria-label') ||
    element.hasAttribute('aria-labelledby') ||
    element.hasAttribute('aria-describedby')
  );
}

/**
 * Check if element is focusable
 */
export function isFocusable(element: Element): boolean {
  const focusableTags = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'];
  const tagName = element.tagName.toUpperCase();

  // Check if it's a naturally focusable element
  if (focusableTags.includes(tagName)) {
    const el = element as HTMLElement;
    return el.tabIndex >= -1 && !el.hasAttribute('disabled');
  }

  // Check for tabIndex on other elements
  const tabIndex = element.getAttribute('tabindex');
  return tabIndex !== null && parseInt(tabIndex, 10) >= 0;
}

/**
 * Get all focusable elements within a container
 */
export function getFocusableElements(container: Element): HTMLElement[] {
  const focusableSelector = [
    'a[href]',
    'button:not([disabled])',
    'input:not([disabled])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    '[tabindex]:not([tabindex="-1"]):not([disabled])',
    '[contenteditable="true"]',
  ].join(', ');

  return Array.from(container.querySelectorAll(focusableSelector)) as HTMLElement[];
}

/**
 * Check keyboard navigation order within a container
 */
export function getKeyboardNavigationOrder(container: Element): HTMLElement[] {
  const focusable = getFocusableElements(container);

  // Sort by tabindex (0 comes after positive values in tab order)
  return focusable.sort((a, b) => {
    const aTabIndex = parseInt(a.getAttribute('tabindex') || '0', 10);
    const bTabIndex = parseInt(b.getAttribute('tabindex') || '0', 10);

    // Elements with tabindex > 0 come first in order
    if (aTabIndex > 0 && bTabIndex > 0) return aTabIndex - bTabIndex;
    if (aTabIndex > 0) return -1;
    if (bTabIndex > 0) return 1;

    // tabindex=0 or natural order: maintain DOM order
    return 0;
  });
}

/**
 * Check color contrast ratio (simplified version)
 * For full contrast checking, use axe-core's color-contrast rule
 */
export function checkColorContrast(
  foreground: string,
  background: string
): { ratio: number; passesAA: boolean; passesAAA: boolean } {
  const getLuminance = (color: string): number => {
    // Parse hex color
    const hex = color.replace('#', '');
    const r = parseInt(hex.substring(0, 2), 16) / 255;
    const g = parseInt(hex.substring(2, 4), 16) / 255;
    const b = parseInt(hex.substring(4, 6), 16) / 255;

    const toLinear = (c: number) =>
      c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);

    return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
  };

  const l1 = getLuminance(foreground);
  const l2 = getLuminance(background);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  const ratio = (lighter + 0.05) / (darker + 0.05);

  return {
    ratio: Math.round(ratio * 100) / 100,
    passesAA: ratio >= 4.5, // Normal text AA
    passesAAA: ratio >= 7, // Normal text AAA
  };
}

/**
 * Simulate keyboard event
 */
export function simulateKeyPress(
  element: Element,
  key: string,
  options: { ctrlKey?: boolean; shiftKey?: boolean; altKey?: boolean; metaKey?: boolean } = {}
): void {
  const event = new KeyboardEvent('keydown', {
    key,
    code: `Key${key.toUpperCase()}`,
    bubbles: true,
    cancelable: true,
    ...options,
  });
  element.dispatchEvent(event);
}

/**
 * Check if element has visible focus indicator
 * Note: This is a heuristic check - actual visibility depends on CSS
 */
export function hasFocusIndicator(element: HTMLElement): boolean {
  // Focus the element
  element.focus();

  // Get computed styles
  const styles = window.getComputedStyle(element);

  // Check for common focus indicators
  const hasOutline = styles.outlineStyle !== 'none' && styles.outlineWidth !== '0px';
  const hasBoxShadow = styles.boxShadow !== 'none';
  const hasBorderChange = styles.borderColor !== 'initial';

  return hasOutline || hasBoxShadow || hasBorderChange;
}

/**
 * Check if element meets minimum touch target size (44x44px)
 */
export function meetsTouchTargetSize(element: Element): boolean {
  const rect = element.getBoundingClientRect();
  return rect.width >= 44 && rect.height >= 44;
}

/**
 * Verify skip link functionality
 */
export function verifySkipLink(
  skipLink: HTMLAnchorElement,
  targetId: string
): { exists: boolean; targetExists: boolean; focusWorks: boolean } {
  const target = document.getElementById(targetId);

  if (!target) {
    return { exists: true, targetExists: false, focusWorks: false };
  }

  // Check if clicking the skip link moves focus to target
  skipLink.click();
  const focusWorks = document.activeElement === target || target.contains(document.activeElement);

  return { exists: true, targetExists: true, focusWorks };
}

/**
 * Find elements missing accessible names
 */
export function findElementsMissingAccessibleName(container: Element): Element[] {
  const interactiveElements = container.querySelectorAll(
    'button, a[href], input, select, textarea, [role="button"], [role="link"]'
  );

  return Array.from(interactiveElements).filter((el) => {
    // Check for accessible name sources
    const hasAriaLabel = el.hasAttribute('aria-label') && el.getAttribute('aria-label')?.trim();
    const hasAriaLabelledBy = el.hasAttribute('aria-labelledby');
    const hasTitle = el.hasAttribute('title') && el.getAttribute('title')?.trim();
    const hasTextContent = el.textContent?.trim();

    // For inputs, check for associated label
    if (el.tagName === 'INPUT' || el.tagName === 'SELECT' || el.tagName === 'TEXTAREA') {
      const id = el.getAttribute('id');
      const hasAssociatedLabel = id && document.querySelector(`label[for="${id}"]`);
      const hasAriaDescribedBy = el.hasAttribute('aria-describedby');
      return !hasAriaLabel && !hasAriaLabelledBy && !hasTitle && !hasAssociatedLabel && !hasAriaDescribedBy;
    }

    return !hasAriaLabel && !hasAriaLabelledBy && !hasTitle && !hasTextContent;
  });
}

export default {
  runAxe,
  assertNoA11yViolations,
  formatViolations,
  generateA11yReport,
  hasAriaLabel,
  isFocusable,
  getFocusableElements,
  getKeyboardNavigationOrder,
  checkColorContrast,
  simulateKeyPress,
  hasFocusIndicator,
  meetsTouchTargetSize,
  verifySkipLink,
  findElementsMissingAccessibleName,
};
