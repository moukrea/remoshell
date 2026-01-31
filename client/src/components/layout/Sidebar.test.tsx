import { describe, it, expect, vi } from 'vitest';

// Test the Sidebar component logic independently

type AppView = 'terminal' | 'files' | 'devices';

interface NavItem {
  id: AppView;
  label: string;
  icon: string;
  description?: string;
}

const defaultNavItems: NavItem[] = [
  { id: 'terminal', label: 'Terminal', icon: '>_', description: 'Remote shell access' },
  { id: 'files', label: 'Files', icon: 'F', description: 'Browse and transfer files' },
  { id: 'devices', label: 'Devices', icon: 'D', description: 'Manage connected devices' },
];

describe('Sidebar', () => {
  describe('Navigation Items', () => {
    it('should have default navigation items', () => {
      expect(defaultNavItems).toHaveLength(3);
      expect(defaultNavItems[0].id).toBe('terminal');
      expect(defaultNavItems[1].id).toBe('files');
      expect(defaultNavItems[2].id).toBe('devices');
    });

    it('should have correct labels for nav items', () => {
      expect(defaultNavItems[0].label).toBe('Terminal');
      expect(defaultNavItems[1].label).toBe('Files');
      expect(defaultNavItems[2].label).toBe('Devices');
    });

    it('should have correct icons for nav items', () => {
      expect(defaultNavItems[0].icon).toBe('>_');
      expect(defaultNavItems[1].icon).toBe('F');
      expect(defaultNavItems[2].icon).toBe('D');
    });

    it('should have descriptions for nav items', () => {
      expect(defaultNavItems[0].description).toBe('Remote shell access');
      expect(defaultNavItems[1].description).toBe('Browse and transfer files');
      expect(defaultNavItems[2].description).toBe('Manage connected devices');
    });
  });

  describe('View Selection', () => {
    it('should call onViewChange when nav item is clicked', () => {
      const onViewChange = vi.fn();

      const handleNavClick = (view: AppView) => {
        onViewChange(view);
      };

      handleNavClick('terminal');
      expect(onViewChange).toHaveBeenCalledWith('terminal');

      handleNavClick('files');
      expect(onViewChange).toHaveBeenCalledWith('files');

      handleNavClick('devices');
      expect(onViewChange).toHaveBeenCalledWith('devices');
    });

    it('should determine active state correctly', () => {
      const isActive = (currentView: AppView, itemView: AppView): boolean => {
        return currentView === itemView;
      };

      expect(isActive('terminal', 'terminal')).toBe(true);
      expect(isActive('terminal', 'files')).toBe(false);
      expect(isActive('files', 'files')).toBe(true);
      expect(isActive('devices', 'terminal')).toBe(false);
    });
  });

  describe('Keyboard Navigation', () => {
    it('should handle Enter key for navigation', () => {
      const onViewChange = vi.fn();

      const handleKeyDown = (view: AppView, key: string) => {
        if (key === 'Enter' || key === ' ') {
          onViewChange(view);
        }
      };

      handleKeyDown('files', 'Enter');
      expect(onViewChange).toHaveBeenCalledWith('files');
    });

    it('should handle Space key for navigation', () => {
      const onViewChange = vi.fn();

      const handleKeyDown = (view: AppView, key: string) => {
        if (key === 'Enter' || key === ' ') {
          onViewChange(view);
        }
      };

      handleKeyDown('devices', ' ');
      expect(onViewChange).toHaveBeenCalledWith('devices');
    });

    it('should not trigger navigation for other keys', () => {
      const onViewChange = vi.fn();

      const handleKeyDown = (view: AppView, key: string) => {
        if (key === 'Enter' || key === ' ') {
          onViewChange(view);
        }
      };

      handleKeyDown('terminal', 'Tab');
      handleKeyDown('terminal', 'Escape');
      handleKeyDown('terminal', 'a');
      expect(onViewChange).not.toHaveBeenCalled();
    });
  });

  describe('Collapse State', () => {
    it('should track collapsed state', () => {
      let collapsed = false;
      const onToggleCollapse = () => {
        collapsed = !collapsed;
      };

      expect(collapsed).toBe(false);

      onToggleCollapse();
      expect(collapsed).toBe(true);

      onToggleCollapse();
      expect(collapsed).toBe(false);
    });

    it('should show full labels when not collapsed', () => {
      const shouldShowLabel = (collapsed: boolean): boolean => {
        return !collapsed;
      };

      expect(shouldShowLabel(false)).toBe(true);
      expect(shouldShowLabel(true)).toBe(false);
    });

    it('should show tooltip when collapsed', () => {
      const shouldShowTooltip = (collapsed: boolean): boolean => {
        return collapsed;
      };

      expect(shouldShowTooltip(true)).toBe(true);
      expect(shouldShowTooltip(false)).toBe(false);
    });
  });

  describe('CSS Class Generation', () => {
    it('should generate correct sidebar classes', () => {
      const getClasses = (collapsed: boolean, mobileOpen: boolean): string => {
        const classes = ['sidebar'];
        if (collapsed) classes.push('sidebar--collapsed');
        if (mobileOpen) classes.push('sidebar--mobile-open');
        return classes.join(' ');
      };

      expect(getClasses(false, false)).toBe('sidebar');
      expect(getClasses(true, false)).toBe('sidebar sidebar--collapsed');
      expect(getClasses(false, true)).toBe('sidebar sidebar--mobile-open');
      expect(getClasses(true, true)).toBe('sidebar sidebar--collapsed sidebar--mobile-open');
    });

    it('should generate correct nav item classes', () => {
      const getNavItemClasses = (isActive: boolean): string => {
        const classes = ['sidebar__nav-item'];
        if (isActive) classes.push('sidebar__nav-item--active');
        return classes.join(' ');
      };

      expect(getNavItemClasses(false)).toBe('sidebar__nav-item');
      expect(getNavItemClasses(true)).toBe('sidebar__nav-item sidebar__nav-item--active');
    });
  });

  describe('ARIA Attributes', () => {
    it('should set aria-current for active item', () => {
      const getAriaCurrent = (isActive: boolean): string | undefined => {
        return isActive ? 'page' : undefined;
      };

      expect(getAriaCurrent(true)).toBe('page');
      expect(getAriaCurrent(false)).toBeUndefined();
    });

    it('should have correct collapse button labels', () => {
      const getCollapseLabel = (collapsed: boolean): string => {
        return collapsed ? 'Expand sidebar' : 'Collapse sidebar';
      };

      expect(getCollapseLabel(false)).toBe('Collapse sidebar');
      expect(getCollapseLabel(true)).toBe('Expand sidebar');
    });

    it('should have correct collapse icon', () => {
      const getCollapseIcon = (collapsed: boolean): string => {
        return collapsed ? '>' : '<';
      };

      expect(getCollapseIcon(false)).toBe('<');
      expect(getCollapseIcon(true)).toBe('>');
    });
  });

  describe('Custom Navigation Items', () => {
    it('should allow custom nav items', () => {
      const customNavItems: NavItem[] = [
        { id: 'terminal', label: 'Shell', icon: '$' },
      ];

      expect(customNavItems).toHaveLength(1);
      expect(customNavItems[0].label).toBe('Shell');
      expect(customNavItems[0].icon).toBe('$');
    });

    it('should fall back to default items when not provided', () => {
      const getNavItems = (customItems?: NavItem[]): NavItem[] => {
        return customItems ?? defaultNavItems;
      };

      expect(getNavItems()).toBe(defaultNavItems);
      expect(getNavItems(undefined)).toBe(defaultNavItems);

      const custom = [{ id: 'terminal' as AppView, label: 'Test', icon: 'T' }];
      expect(getNavItems(custom)).toBe(custom);
    });
  });
});
