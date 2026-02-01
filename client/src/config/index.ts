/**
 * Application configuration module
 * Provides centralized configuration with environment variable support
 */

export interface AppConfig {
  /** WebSocket URL for signaling server */
  signalingUrl: string;
  /** ICE servers for WebRTC */
  iceServers: RTCIceServer[];
}

const DEFAULT_CONFIG: AppConfig = {
  signalingUrl: import.meta.env.VITE_SIGNALING_URL || 'wss://remoshell-signaling.moukrea.workers.dev',
  iceServers: [
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
  ],
};

let config: AppConfig = { ...DEFAULT_CONFIG };

/**
 * Get current application configuration
 */
export function getConfig(): AppConfig {
  return config;
}

/**
 * Update signaling server URL at runtime
 * Used when pairing with a device that specifies a different relay URL
 */
export function setSignalingUrl(url: string): void {
  config = { ...config, signalingUrl: url };
}

/**
 * Reset configuration to defaults
 * Useful for testing
 */
export function resetConfig(): void {
  config = { ...DEFAULT_CONFIG };
}
