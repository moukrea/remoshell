import { Component, JSX, createSignal, Show, onMount, onCleanup } from 'solid-js';
import Sidebar from './Sidebar';
import Header from './Header';
import SkipLink from '../a11y/SkipLink';

/**
 * View type for navigation
 */
export type AppView = 'terminal' | 'files' | 'devices';

export interface AppShellProps {
  /** The main content to render */
  children?: JSX.Element;
  /** Current active view */
  activeView?: AppView;
  /** Called when view changes */
  onViewChange?: (view: AppView) => void;
  /** Called when theme changes */
  onThemeChange?: (isDark: boolean) => void;
  /** Current theme (true for dark, false for light) */
  isDarkTheme?: boolean;
  /** Connection status for the header */
  connectionStatus?: 'connected' | 'connecting' | 'disconnected';
  /** Additional CSS class */
  class?: string;
}

/**
 * Main application shell with responsive layout.
 * Contains sidebar navigation, header with status/controls, and main content area.
 */
const AppShell: Component<AppShellProps> = (props) => {
  const [sidebarCollapsed, setSidebarCollapsed] = createSignal(false);
  const [mobileMenuOpen, setMobileMenuOpen] = createSignal(false);
  const [activeView, setActiveView] = createSignal<AppView>(props.activeView ?? 'terminal');
  const [isDark, setIsDark] = createSignal(props.isDarkTheme ?? true);

  // Handle view change from sidebar
  const handleViewChange = (view: AppView) => {
    setActiveView(view);
    props.onViewChange?.(view);
    // Close mobile menu when view changes
    setMobileMenuOpen(false);
  };

  // Handle theme toggle
  const handleThemeToggle = () => {
    const newIsDark = !isDark();
    setIsDark(newIsDark);
    props.onThemeChange?.(newIsDark);
  };

  // Handle sidebar collapse toggle
  const handleSidebarToggle = () => {
    setSidebarCollapsed(!sidebarCollapsed());
  };

  // Handle mobile menu toggle
  const handleMobileMenuToggle = () => {
    setMobileMenuOpen(!mobileMenuOpen());
  };

  // Close mobile menu when clicking overlay
  const handleOverlayClick = () => {
    setMobileMenuOpen(false);
  };

  // Handle Escape key to close mobile menu
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && mobileMenuOpen()) {
      setMobileMenuOpen(false);
    }
  };

  // Set up global keyboard listener for Escape
  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
  });

  return (
    <div
      class={`app-shell ${props.class ?? ''}`}
      data-testid="app-shell"
      data-theme={isDark() ? 'dark' : 'light'}
    >
      {/* Skip links for keyboard navigation */}
      <SkipLink />

      {/* Mobile menu overlay */}
      <Show when={mobileMenuOpen()}>
        <div
          class="app-shell__overlay"
          data-testid="mobile-overlay"
          onClick={handleOverlayClick}
          role="presentation"
          aria-hidden="true"
        />
      </Show>

      {/* Sidebar */}
      <Sidebar
        activeView={activeView()}
        onViewChange={handleViewChange}
        collapsed={sidebarCollapsed()}
        onToggleCollapse={handleSidebarToggle}
        mobileOpen={mobileMenuOpen()}
        class={`app-shell__sidebar ${sidebarCollapsed() ? 'app-shell__sidebar--collapsed' : ''} ${mobileMenuOpen() ? 'app-shell__sidebar--mobile-open' : ''}`}
      />

      {/* Main content area */}
      <div
        class={`app-shell__main ${sidebarCollapsed() ? 'app-shell__main--sidebar-collapsed' : ''}`}
        data-testid="main-content"
      >
        {/* Header */}
        <Header
          connectionStatus={props.connectionStatus ?? 'disconnected'}
          isDarkTheme={isDark()}
          onThemeToggle={handleThemeToggle}
          onMobileMenuToggle={handleMobileMenuToggle}
          class="app-shell__header"
        />

        {/* Content area */}
        <main
          id="main-content"
          class="app-shell__content"
          data-testid="content-area"
          role="main"
          aria-label="Main content"
          tabIndex={-1}
        >
          {props.children}
        </main>
      </div>
    </div>
  );
};

export default AppShell;
export { AppShell };
