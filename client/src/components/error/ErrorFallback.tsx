import { Component } from 'solid-js';

export interface ErrorFallbackProps {
  error: Error;
  reset: () => void;
}

const ErrorFallback: Component<ErrorFallbackProps> = (props) => {
  return (
    <div class="error-fallback" role="alert" data-testid="error-fallback">
      <div class="error-fallback__icon">!</div>
      <h2 class="error-fallback__title">Something went wrong</h2>
      <p class="error-fallback__message">{props.error.message}</p>
      <button
        class="error-fallback__retry"
        onClick={() => props.reset()}
        type="button"
      >
        Try Again
      </button>
    </div>
  );
};

export default ErrorFallback;
export { ErrorFallback };
