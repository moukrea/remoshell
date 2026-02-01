import { describe, it, expect, beforeEach } from 'vitest';
import { getConfig, setSignalingUrl, resetConfig, type AppConfig } from './index';

describe('Config Module', () => {
  beforeEach(() => {
    resetConfig();
  });

  describe('getConfig', () => {
    it('should return default configuration', () => {
      const config = getConfig();

      expect(config).toBeDefined();
      expect(config.signalingUrl).toBeDefined();
      expect(config.iceServers).toBeDefined();
    });

    it('should have default signaling URL', () => {
      const config = getConfig();

      expect(config.signalingUrl).toBe('wss://remoshell-signaling.workers.dev');
    });

    it('should have Google STUN servers as default ICE servers', () => {
      const config = getConfig();

      expect(config.iceServers).toHaveLength(2);
      expect(config.iceServers[0].urls).toBe('stun:stun.l.google.com:19302');
      expect(config.iceServers[1].urls).toBe('stun:stun1.l.google.com:19302');
    });

    it('should return same config object on multiple calls', () => {
      const config1 = getConfig();
      const config2 = getConfig();

      expect(config1).toBe(config2);
    });
  });

  describe('setSignalingUrl', () => {
    it('should update signaling URL', () => {
      const newUrl = 'wss://custom-server.example.com';

      setSignalingUrl(newUrl);

      expect(getConfig().signalingUrl).toBe(newUrl);
    });

    it('should preserve other config values when updating signaling URL', () => {
      const originalConfig = getConfig();
      const originalIceServers = [...originalConfig.iceServers];

      setSignalingUrl('wss://new-server.example.com');

      const updatedConfig = getConfig();
      expect(updatedConfig.iceServers).toEqual(originalIceServers);
    });

    it('should return new config object after update', () => {
      const config1 = getConfig();

      setSignalingUrl('wss://new-server.example.com');

      const config2 = getConfig();
      expect(config1).not.toBe(config2);
    });
  });

  describe('resetConfig', () => {
    it('should reset to default configuration', () => {
      setSignalingUrl('wss://custom-server.example.com');

      resetConfig();

      expect(getConfig().signalingUrl).toBe('wss://remoshell-signaling.workers.dev');
    });

    it('should restore default ICE servers', () => {
      resetConfig();

      const config = getConfig();
      expect(config.iceServers).toHaveLength(2);
      expect(config.iceServers[0].urls).toBe('stun:stun.l.google.com:19302');
    });
  });

  describe('AppConfig interface', () => {
    it('should have signalingUrl property', () => {
      const config: AppConfig = getConfig();

      expect(typeof config.signalingUrl).toBe('string');
    });

    it('should have iceServers property as array', () => {
      const config: AppConfig = getConfig();

      expect(Array.isArray(config.iceServers)).toBe(true);
    });

    it('should have RTCIceServer objects with urls property', () => {
      const config: AppConfig = getConfig();

      config.iceServers.forEach((server) => {
        expect(server.urls).toBeDefined();
      });
    });
  });
});
