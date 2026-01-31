import { Component, createSignal, onMount, onCleanup, Show } from 'solid-js';

/**
 * OfflineIndicator component
 * Shows a banner when the user is offline
 */
const OfflineIndicator: Component = () => {
  const [isOffline, setIsOffline] = createSignal(!navigator.onLine);
  const [showBanner, setShowBanner] = createSignal(false);

  onMount(() => {
    const handleOnline = () => {
      setIsOffline(false);
      // Show "back online" message briefly
      setShowBanner(true);
      setTimeout(() => setShowBanner(false), 3000);
    };

    const handleOffline = () => {
      setIsOffline(true);
      setShowBanner(true);
    };

    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    // Initial check
    if (!navigator.onLine) {
      setShowBanner(true);
    }

    onCleanup(() => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    });
  });

  return (
    <Show when={showBanner()}>
      <div
        class={`offline-banner ${isOffline() ? 'offline-banner--offline' : 'offline-banner--online'}`}
        role="alert"
        aria-live="polite"
        data-testid="offline-indicator"
      >
        <Show
          when={isOffline()}
          fallback={
            <div class="offline-banner__content">
              <svg
                class="offline-banner__icon"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
              <span>You're back online</span>
            </div>
          }
        >
          <div class="offline-banner__content">
            <svg
              class="offline-banner__icon"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="1" y1="1" x2="23" y2="23" />
              <path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55" />
              <path d="M5 12.55a10.94 10.94 0 0 1 5.17-2.39" />
              <path d="M10.71 5.05A16 16 0 0 1 22.58 9" />
              <path d="M1.42 9a15.91 15.91 0 0 1 4.7-2.88" />
              <path d="M8.53 16.11a6 6 0 0 1 6.95 0" />
              <line x1="12" y1="20" x2="12.01" y2="20" />
            </svg>
            <span>You're offline</span>
          </div>
        </Show>
        <button
          class="offline-banner__close"
          onClick={() => setShowBanner(false)}
          aria-label="Dismiss"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </Show>
  );
};

export default OfflineIndicator;
