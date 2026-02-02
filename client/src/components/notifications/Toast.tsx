import { Component, Show, For, createSignal, onMount, onCleanup, createEffect } from 'solid-js';
import {
  type Notification,
  type NotificationType,
  getNotificationStore,
} from '../../stores/notifications';

/**
 * Get CSS class for notification type
 */
const getTypeClass = (type: NotificationType): string => {
  const classes: Record<NotificationType, string> = {
    info: 'toast--info',
    success: 'toast--success',
    warning: 'toast--warning',
    error: 'toast--error',
  };
  return classes[type];
};

/**
 * Get icon for notification type
 */
const getTypeIcon = (type: NotificationType): string => {
  const icons: Record<NotificationType, string> = {
    info: 'i',
    success: '✓',
    warning: '!',
    error: '✕',
  };
  return icons[type];
};

export interface ToastProps {
  notification: Notification;
  /** Called when dismiss animation completes */
  onDismissComplete?: (id: string) => void;
}

/**
 * Individual toast notification component
 */
const Toast: Component<ToastProps> = (props) => {
  const store = getNotificationStore();
  const [isEntering, setIsEntering] = createSignal(true);
  const [isExiting, setIsExiting] = createSignal(false);
  const [executingActions, setExecutingActions] = createSignal<Set<number>>(new Set());

  let toastRef: HTMLDivElement | undefined;

  // Handle enter animation
  onMount(() => {
    // Trigger enter animation on next frame
    requestAnimationFrame(() => {
      setIsEntering(false);
    });
  });

  // Handle exit animation when dismissing
  createEffect(() => {
    if (props.notification.dismissing && !isExiting()) {
      setIsExiting(true);
    }
  });

  // Listen for animation end to complete removal
  onMount(() => {
    const handleAnimationEnd = (e: AnimationEvent) => {
      if (e.animationName.includes('toast-exit') || e.animationName.includes('fadeOut')) {
        props.onDismissComplete?.(props.notification.id);
      }
    };

    toastRef?.addEventListener('animationend', handleAnimationEnd);

    onCleanup(() => {
      toastRef?.removeEventListener('animationend', handleAnimationEnd);
    });
  });

  const handleDismiss = () => {
    store.dismiss(props.notification.id);
  };

  const handleMouseEnter = () => {
    store.pauseTimer(props.notification.id);
  };

  const handleMouseLeave = () => {
    store.resumeTimer(props.notification.id);
  };

  const handleActionClick = async (index: number, onClick: () => void | Promise<void>) => {
    if (executingActions().has(index)) return;
    setExecutingActions(prev => new Set([...prev, index]));
    try {
      await onClick();
      store.dismiss(props.notification.id);
    } finally {
      setExecutingActions(prev => {
        const next = new Set(prev);
        next.delete(index);
        return next;
      });
    }
  };

  return (
    <div
      ref={toastRef}
      class={`toast ${getTypeClass(props.notification.type)} ${isEntering() ? 'toast--entering' : ''} ${isExiting() ? 'toast--exiting' : ''}`}
      data-testid={`toast-${props.notification.id}`}
      role="alert"
      aria-live={props.notification.type === 'error' ? 'assertive' : 'polite'}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Icon */}
      <div
        class={`toast__icon toast__icon--${props.notification.type}`}
        data-testid={`toast-icon-${props.notification.id}`}
      >
        {getTypeIcon(props.notification.type)}
      </div>

      {/* Content */}
      <div class="toast__content">
        <div
          class="toast__title"
          data-testid={`toast-title-${props.notification.id}`}
        >
          {props.notification.title}
        </div>
        <Show when={props.notification.message}>
          <div
            class="toast__message"
            data-testid={`toast-message-${props.notification.id}`}
          >
            {props.notification.message}
          </div>
        </Show>

        {/* Action buttons */}
        <Show when={props.notification.actions && props.notification.actions.length > 0}>
          <div class="toast__actions" data-testid={`toast-actions-${props.notification.id}`}>
            <For each={props.notification.actions}>
              {(action, index) => (
                <button
                  class="toast__action"
                  disabled={executingActions().has(index())}
                  onClick={() => handleActionClick(index(), action.onClick)}
                  data-testid={`toast-action-${props.notification.id}-${index()}`}
                >
                  {action.label}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* Dismiss button */}
      <button
        class="toast__dismiss"
        onClick={handleDismiss}
        aria-label="Dismiss notification"
        data-testid={`toast-dismiss-${props.notification.id}`}
      >
        ✕
      </button>
    </div>
  );
};

export default Toast;
export { getTypeClass, getTypeIcon };
