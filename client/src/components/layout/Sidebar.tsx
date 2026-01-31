import { Component, For, Show, createMemo } from 'solid-js';
import type { AppView } from './AppShell';

/**
 * Navigation item definition
 */
export interface NavItem {
  id: AppView;
  label: string;
  icon: string;
  description?: string;
}

/**
 * Default navigation items
 */
export const defaultNavItems: NavItem[] = [
  {
    id: 'terminal',
    label: 'Terminal',
    icon: '>_',
    description: 'Remote shell access',
  },
  {
    id: 'files',
    label: 'Files',
    icon: 'F',
    description: 'Browse and transfer files',
  },
  {
    id: 'devices',
    label: 'Devices',
    icon: 'D',
    description: 'Manage connected devices',
  },
];

export interface SidebarProps {
  /** Current active view */
  activeView: AppView;
  /** Called when view changes */
  onViewChange: (view: AppView) => void;
  /** Whether sidebar is collapsed (desktop) */
  collapsed?: boolean;
  /** Called when collapse toggle is clicked */
  onToggleCollapse?: () => void;
  /** Whether mobile menu is open */
  mobileOpen?: boolean;
  /** Navigation items to display */
  navItems?: NavItem[];
  /** Additional CSS class */
  class?: string;
}

/**
 * Sidebar navigation component.
 * Displays navigation items for switching between views.
 * Supports collapsed state on desktop and drawer-style on mobile.
 */
const Sidebar: Component<SidebarProps> = (props) => {
  const navItems = createMemo(() => props.navItems ?? defaultNavItems);

  const handleNavClick = (view: AppView) => {
    props.onViewChange(view);
  };

  const handleKeyDown = (view: AppView, e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      props.onViewChange(view);
    }
  };

  return (
    <aside
      id="main-navigation"
      class={`sidebar ${props.collapsed ? 'sidebar--collapsed' : ''} ${props.mobileOpen ? 'sidebar--mobile-open' : ''} ${props.class ?? ''}`}
      data-testid="sidebar"
      role="navigation"
      aria-label="Main navigation"
      tabIndex={-1}
    >
      {/* Logo / Brand */}
      <div class="sidebar__brand" data-testid="sidebar-brand">
        <span class="sidebar__logo">RS</span>
        <Show when={!props.collapsed}>
          <span class="sidebar__brand-text">RemoShell</span>
        </Show>
      </div>

      {/* Navigation items */}
      <nav class="sidebar__nav" data-testid="sidebar-nav">
        <For each={navItems()}>
          {(item) => (
            <div
              class={`sidebar__nav-item ${props.activeView === item.id ? 'sidebar__nav-item--active' : ''}`}
              data-testid={`nav-item-${item.id}`}
              onClick={() => handleNavClick(item.id)}
              onKeyDown={(e) => handleKeyDown(item.id, e)}
              tabIndex={0}
              role="button"
              aria-current={props.activeView === item.id ? 'page' : undefined}
              title={props.collapsed ? item.label : undefined}
            >
              <span class="sidebar__nav-icon" data-testid={`nav-icon-${item.id}`}>
                {item.icon}
              </span>
              <Show when={!props.collapsed}>
                <span class="sidebar__nav-label">{item.label}</span>
              </Show>
            </div>
          )}
        </For>
      </nav>

      {/* Collapse toggle (desktop only) */}
      <div class="sidebar__footer" data-testid="sidebar-footer">
        <button
          class="sidebar__collapse-btn"
          onClick={props.onToggleCollapse}
          data-testid="collapse-toggle"
          aria-label={props.collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          title={props.collapsed ? 'Expand' : 'Collapse'}
        >
          <span class="sidebar__collapse-icon">
            {props.collapsed ? '>' : '<'}
          </span>
        </button>
      </div>
    </aside>
  );
};

export default Sidebar;
export { Sidebar };
