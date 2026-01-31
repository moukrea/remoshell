import { Component, For, createMemo } from 'solid-js';
import { Portal } from 'solid-js/web';
import { getNotificationStore } from '../../stores/notifications';
import Toast from './Toast';

/**
 * Position options for the toast container
 */
export type ToastPosition =
  | 'top-left'
  | 'top-center'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-center'
  | 'bottom-right';

export interface ToastContainerProps {
  /** Position of the toast container */
  position?: ToastPosition;
  /** Maximum number of toasts to show at once */
  maxToasts?: number;
  /** Additional CSS class for the container */
  class?: string;
  /** Whether to use a portal to render outside the component tree */
  usePortal?: boolean;
}

/**
 * Get CSS class for position
 */
const getPositionClass = (position: ToastPosition): string => {
  const classes: Record<ToastPosition, string> = {
    'top-left': 'toast-container--top-left',
    'top-center': 'toast-container--top-center',
    'top-right': 'toast-container--top-right',
    'bottom-left': 'toast-container--bottom-left',
    'bottom-center': 'toast-container--bottom-center',
    'bottom-right': 'toast-container--bottom-right',
  };
  return classes[position];
};

/**
 * Container component for displaying toast notifications
 */
const ToastContainer: Component<ToastContainerProps> = (props) => {
  const store = getNotificationStore();
  const position = () => props.position ?? 'top-right';
  const maxToasts = () => props.maxToasts ?? 5;
  const usePortal = () => props.usePortal ?? true;

  // Get visible notifications (limited by maxToasts)
  const visibleNotifications = createMemo(() => {
    const notifications = store.getNotifications();
    const max = maxToasts();

    // For bottom positions, we want the newest at the bottom
    // For top positions, we want the newest at the top
    const isBottom = position().startsWith('bottom');

    if (notifications.length <= max) {
      return isBottom ? notifications : notifications;
    }

    // Show only the most recent notifications
    return notifications.slice(-max);
  });

  const handleDismissComplete = (id: string) => {
    store.remove(id);
  };

  const containerContent = () => (
    <div
      class={`toast-container ${getPositionClass(position())} ${props.class ?? ''}`}
      data-testid="toast-container"
      aria-live="polite"
      aria-label="Notifications"
    >
      <For each={visibleNotifications()}>
        {(notification) => (
          <Toast
            notification={notification}
            onDismissComplete={handleDismissComplete}
          />
        )}
      </For>
    </div>
  );

  // Use Portal to render toasts outside the normal DOM hierarchy
  return (
    <>
      {usePortal() ? (
        <Portal>
          {containerContent()}
        </Portal>
      ) : (
        containerContent()
      )}
    </>
  );
};

export default ToastContainer;
export { getPositionClass };
