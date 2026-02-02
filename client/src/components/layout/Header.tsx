import { Component, Show, createMemo } from 'solid-js';

/**
 * Connection status type
 */
export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected';

export interface HeaderProps {
  /** Current connection status */
  connectionStatus: ConnectionStatus;
  /** Whether dark theme is active */
  isDarkTheme: boolean;
  /** Called when theme toggle is clicked */
  onThemeToggle: () => void;
  /** Called when mobile menu toggle is clicked */
  onMobileMenuToggle?: () => void;
  /** Whether mobile menu is open (for aria-expanded) */
  mobileMenuOpen?: boolean;
  /** Connection error message to display */
  connectionError?: string | null;
  /** Called when retry button is clicked */
  onConnectionRetry?: () => void;
  /** Additional CSS class */
  class?: string;
}

/**
 * Get display text for connection status
 */
const getStatusText = (status: ConnectionStatus): string => {
  const texts: Record<ConnectionStatus, string> = {
    connected: 'Connected',
    connecting: 'Connecting...',
    disconnected: 'Disconnected',
  };
  return texts[status];
};

/**
 * Get CSS class modifier for connection status
 */
const getStatusClass = (status: ConnectionStatus): string => {
  const classes: Record<ConnectionStatus, string> = {
    connected: 'header__status-indicator--connected',
    connecting: 'header__status-indicator--connecting',
    disconnected: 'header__status-indicator--disconnected',
  };
  return classes[status];
};

/**
 * Header component with connection status indicator and theme toggle.
 * Also contains mobile menu button for responsive layout.
 */
const Header: Component<HeaderProps> = (props) => {
  const statusText = createMemo(() => getStatusText(props.connectionStatus));
  const statusClass = createMemo(() => getStatusClass(props.connectionStatus));

  return (
    <header
      class={`header ${props.class ?? ''}`}
      data-testid="header"
      role="banner"
    >
      {/* Connection error banner */}
      <Show when={props.connectionError}>
        <div
          class="connection-error-banner"
          data-testid="connection-error"
          role="alert"
          aria-live="assertive"
        >
          <span class="connection-error-banner__icon" aria-hidden="true">!</span>
          <span class="connection-error-banner__message">{props.connectionError}</span>
          <button
            class="connection-error-banner__retry"
            data-testid="connection-retry"
            onClick={() => props.onConnectionRetry?.()}
            aria-label="Retry connection"
          >
            Retry
          </button>
        </div>
      </Show>

      {/* Mobile menu button */}
      <button
        class="header__mobile-menu-btn"
        onClick={props.onMobileMenuToggle}
        data-testid="mobile-menu-btn"
        aria-label="Toggle navigation menu"
        aria-expanded={props.mobileMenuOpen}
      >
        <span class="header__hamburger">
          <span class="header__hamburger-line" />
          <span class="header__hamburger-line" />
          <span class="header__hamburger-line" />
        </span>
      </button>

      {/* Left section - can contain breadcrumbs or title */}
      <div class="header__left" data-testid="header-left">
        <h1 class="header__title">RemoShell</h1>
      </div>

      {/* Right section - status and controls */}
      <div class="header__right" data-testid="header-right">
        {/* Connection status indicator */}
        <div
          class="header__status"
          data-testid="connection-status"
          role="status"
          aria-live="polite"
        >
          <span
            class={`header__status-indicator ${statusClass()}`}
            data-testid="status-indicator"
            aria-hidden="true"
          />
          <span class="header__status-text" data-testid="status-text">
            {statusText()}
          </span>
        </div>

        {/* Theme toggle */}
        <button
          class="header__theme-toggle"
          onClick={props.onThemeToggle}
          data-testid="theme-toggle"
          aria-label={props.isDarkTheme ? 'Switch to light theme' : 'Switch to dark theme'}
          title={props.isDarkTheme ? 'Light mode' : 'Dark mode'}
        >
          <Show
            when={props.isDarkTheme}
            fallback={
              <span class="header__theme-icon header__theme-icon--light" data-testid="theme-icon-light">
                L
              </span>
            }
          >
            <span class="header__theme-icon header__theme-icon--dark" data-testid="theme-icon-dark">
              D
            </span>
          </Show>
        </button>
      </div>
    </header>
  );
};

export default Header;
export { Header, getStatusText, getStatusClass };
