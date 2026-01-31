import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  SignalingClient,
  getSignalingClient,
  resetSignalingClient,
  type SignalingConnectionState,
  type AnySignalingEvent,
  type ConnectedEvent,
  type PeerJoinedEvent,
  type PeerLeftEvent,
  type OfferEvent,
  type AnswerEvent,
  type IceEvent,
  type SignalingErrorEvent,
  type StateChangeEvent,
} from './SignalingClient';

// Mock WebSocket
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  url: string;
  readyState: number = MockWebSocket.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  sentMessages: string[] = [];

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(data: string): void {
    if (this.readyState !== MockWebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }
    this.sentMessages.push(data);
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    if (this.onclose) {
      this.onclose(new CloseEvent('close', { code: 1000, reason: 'Normal closure' }));
    }
  }

  // Test helpers
  simulateOpen(): void {
    this.readyState = MockWebSocket.OPEN;
    if (this.onopen) {
      this.onopen(new Event('open'));
    }
  }

  simulateMessage(data: unknown): void {
    if (this.onmessage) {
      this.onmessage(new MessageEvent('message', { data: JSON.stringify(data) }));
    }
  }

  simulateClose(code = 1000, reason = ''): void {
    this.readyState = MockWebSocket.CLOSED;
    if (this.onclose) {
      this.onclose(new CloseEvent('close', { code, reason }));
    }
  }

  simulateError(): void {
    if (this.onerror) {
      this.onerror(new Event('error'));
    }
  }
}

// Replace global WebSocket with mock
const originalWebSocket = global.WebSocket;

describe('SignalingClient', () => {
  let client: SignalingClient;

  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    // @ts-expect-error - Assigning mock to global
    global.WebSocket = MockWebSocket;
    resetSignalingClient();
    client = new SignalingClient({
      serverUrl: 'https://signaling.example.com',
    });
  });

  afterEach(() => {
    client.destroy();
    resetSignalingClient();
    global.WebSocket = originalWebSocket;
    vi.useRealTimers();
  });

  describe('Constructor and Configuration', () => {
    it('should create client with default configuration', () => {
      const c = new SignalingClient({
        serverUrl: 'https://test.example.com',
      });
      expect(c.getState()).toBe('disconnected');
      expect(c.getPeerId()).toBeNull();
      expect(c.getRoomId()).toBeNull();
      c.destroy();
    });

    it('should accept custom configuration', () => {
      const c = new SignalingClient({
        serverUrl: 'https://test.example.com',
        maxReconnectAttempts: 10,
        reconnectBaseDelay: 500,
        reconnectMaxDelay: 60000,
      });
      expect(c.getState()).toBe('disconnected');
      c.destroy();
    });
  });

  describe('Room Join', () => {
    it('should connect to WebSocket when joining a room', () => {
      client.join('test-room');

      expect(MockWebSocket.instances.length).toBe(1);
      expect(MockWebSocket.instances[0].url).toBe('wss://signaling.example.com/room/test-room');
    });

    it('should convert http to ws in URL', () => {
      client.join('test-room');

      expect(MockWebSocket.instances[0].url).toBe('wss://signaling.example.com/room/test-room');
    });

    it('should set state to connecting', () => {
      const states: SignalingConnectionState[] = [];
      client.subscribe((event) => {
        if (event.type === 'state_change') {
          states.push((event as StateChangeEvent).state);
        }
      });

      client.join('test-room');

      expect(states).toContain('connecting');
      expect(client.getState()).toBe('connecting');
    });

    it('should emit connected event after receiving join message', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: ['peer-1', 'peer-2'] },
      });

      const connectedEvent = events.find((e) => e.type === 'connected') as ConnectedEvent;
      expect(connectedEvent).toBeDefined();
      expect(connectedEvent.peerId).toBe('my-peer-id');
      expect(connectedEvent.existingPeers).toEqual(['peer-1', 'peer-2']);
    });

    it('should set peerId and roomId after join', () => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      expect(client.getPeerId()).toBe('my-peer-id');
      expect(client.getRoomId()).toBe('test-room');
      expect(client.isConnected()).toBe(true);
    });

    it('should warn when already connected', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      client.join('another-room');

      expect(consoleWarn).toHaveBeenCalledWith('Already connected or connecting to signaling server');
      consoleWarn.mockRestore();
    });
  });

  describe('Room Leave', () => {
    it('should close WebSocket when leaving', () => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      client.leave();

      expect(ws.readyState).toBe(MockWebSocket.CLOSED);
    });

    it('should emit disconnected event when leaving', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      client.leave();

      expect(client.getState()).toBe('disconnected');
    });

    it('should reset peerId and roomId after leave', () => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      client.leave();

      expect(client.getPeerId()).toBeNull();
      expect(client.getRoomId()).toBeNull();
    });
  });

  describe('Peer Events', () => {
    beforeEach(() => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });
    });

    it('should emit peer_joined event', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      const ws = MockWebSocket.instances[0];
      ws.simulateMessage({
        type: 'peer-joined',
        peerId: 'new-peer',
      });

      const peerJoinedEvent = events.find((e) => e.type === 'peer_joined') as PeerJoinedEvent;
      expect(peerJoinedEvent).toBeDefined();
      expect(peerJoinedEvent.peerId).toBe('new-peer');
    });

    it('should emit peer_left event', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      const ws = MockWebSocket.instances[0];
      ws.simulateMessage({
        type: 'peer-left',
        peerId: 'leaving-peer',
      });

      const peerLeftEvent = events.find((e) => e.type === 'peer_left') as PeerLeftEvent;
      expect(peerLeftEvent).toBeDefined();
      expect(peerLeftEvent.peerId).toBe('leaving-peer');
    });
  });

  describe('Signal Relay', () => {
    beforeEach(() => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });
    });

    it('should emit offer event', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      const ws = MockWebSocket.instances[0];
      ws.simulateMessage({
        type: 'offer',
        peerId: 'remote-peer',
        data: { type: 'offer', sdp: 'test-sdp' },
      });

      const offerEvent = events.find((e) => e.type === 'offer') as OfferEvent;
      expect(offerEvent).toBeDefined();
      expect(offerEvent.peerId).toBe('remote-peer');
      expect(offerEvent.offer).toEqual({ type: 'offer', sdp: 'test-sdp' });
    });

    it('should emit answer event', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      const ws = MockWebSocket.instances[0];
      ws.simulateMessage({
        type: 'answer',
        peerId: 'remote-peer',
        data: { type: 'answer', sdp: 'answer-sdp' },
      });

      const answerEvent = events.find((e) => e.type === 'answer') as AnswerEvent;
      expect(answerEvent).toBeDefined();
      expect(answerEvent.peerId).toBe('remote-peer');
      expect(answerEvent.answer).toEqual({ type: 'answer', sdp: 'answer-sdp' });
    });

    it('should emit ice event', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      const ws = MockWebSocket.instances[0];
      ws.simulateMessage({
        type: 'ice',
        peerId: 'remote-peer',
        data: { candidate: 'ice-candidate', sdpMLineIndex: 0 },
      });

      const iceEvent = events.find((e) => e.type === 'ice') as IceEvent;
      expect(iceEvent).toBeDefined();
      expect(iceEvent.peerId).toBe('remote-peer');
      expect(iceEvent.candidate).toEqual({ candidate: 'ice-candidate', sdpMLineIndex: 0 });
    });
  });

  describe('Sending Signals', () => {
    beforeEach(() => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });
    });

    it('should send offer message', () => {
      const result = client.sendOffer({ type: 'offer', sdp: 'my-offer-sdp' });

      expect(result).toBe(true);
      const ws = MockWebSocket.instances[0];
      expect(ws.sentMessages.length).toBe(1);
      const sent = JSON.parse(ws.sentMessages[0]);
      expect(sent.type).toBe('offer');
      expect(sent.data).toEqual({ type: 'offer', sdp: 'my-offer-sdp' });
    });

    it('should send answer message', () => {
      const result = client.sendAnswer({ type: 'answer', sdp: 'my-answer-sdp' });

      expect(result).toBe(true);
      const ws = MockWebSocket.instances[0];
      const sent = JSON.parse(ws.sentMessages[0]);
      expect(sent.type).toBe('answer');
      expect(sent.data).toEqual({ type: 'answer', sdp: 'my-answer-sdp' });
    });

    it('should send ICE candidate message', () => {
      const result = client.sendIceCandidate({ candidate: 'my-candidate', sdpMLineIndex: 0 });

      expect(result).toBe(true);
      const ws = MockWebSocket.instances[0];
      const sent = JSON.parse(ws.sentMessages[0]);
      expect(sent.type).toBe('ice');
      expect(sent.data).toEqual({ candidate: 'my-candidate', sdpMLineIndex: 0 });
    });

    it('should return false when not connected', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      client.leave();
      const result = client.sendOffer({ type: 'offer', sdp: 'test' });

      expect(result).toBe(false);
      consoleWarn.mockRestore();
    });
  });

  describe('Reconnection', () => {
    it('should attempt to reconnect on unexpected close', () => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      // Simulate unexpected close
      ws.simulateClose(1006, 'Abnormal closure');

      expect(client.getState()).toBe('reconnecting');

      // Advance timer to trigger reconnection
      vi.advanceTimersByTime(1500);

      expect(MockWebSocket.instances.length).toBe(2);
    });

    it('should use exponential backoff for reconnection', () => {
      client.join('test-room');
      const ws1 = MockWebSocket.instances[0];
      ws1.simulateOpen();
      ws1.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      // First disconnect
      ws1.simulateClose(1006, '');
      // First retry after ~1000ms + up to 30% jitter = max 1300ms
      vi.advanceTimersByTime(1500);

      const ws2 = MockWebSocket.instances[1];
      ws2.simulateClose(1006, ''); // Second disconnect
      // Second retry after ~2000ms + up to 30% jitter = max 2600ms
      vi.advanceTimersByTime(3000);

      const ws3 = MockWebSocket.instances[2];
      ws3.simulateClose(1006, ''); // Third disconnect
      // Third retry after ~4000ms + up to 30% jitter = max 5200ms
      vi.advanceTimersByTime(6000);

      expect(MockWebSocket.instances.length).toBe(4);
    });

    it('should stop reconnecting after max attempts', () => {
      const c = new SignalingClient({
        serverUrl: 'https://signaling.example.com',
        maxReconnectAttempts: 2,
        reconnectBaseDelay: 100,
      });

      const events: AnySignalingEvent[] = [];
      c.subscribe((event) => events.push(event));

      c.join('test-room');
      const ws1 = MockWebSocket.instances[0];
      ws1.simulateOpen();
      ws1.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      // First disconnect and reconnect
      ws1.simulateClose(1006, '');
      vi.advanceTimersByTime(200);

      // Second disconnect and reconnect
      const ws2 = MockWebSocket.instances[1];
      ws2.simulateClose(1006, '');
      vi.advanceTimersByTime(400);

      // Third disconnect - should not reconnect
      const ws3 = MockWebSocket.instances[2];
      ws3.simulateClose(1006, '');

      expect(c.getState()).toBe('disconnected');
      const errorEvent = events.find(
        (e) => e.type === 'error' && (e as SignalingErrorEvent).message === 'Max reconnection attempts reached'
      );
      expect(errorEvent).toBeDefined();

      c.destroy();
    });

    it('should not reconnect on intentional leave', () => {
      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      client.leave();

      vi.advanceTimersByTime(5000);

      // Should only have the one initial WebSocket
      expect(MockWebSocket.instances.length).toBe(1);
      expect(client.getState()).toBe('disconnected');
    });

    it('should reset reconnect attempts after successful connection', () => {
      client.join('test-room');
      const ws1 = MockWebSocket.instances[0];
      ws1.simulateOpen();
      ws1.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      // First disconnect
      ws1.simulateClose(1006, '');
      vi.advanceTimersByTime(1500);

      // Successful reconnect
      const ws2 = MockWebSocket.instances[1];
      ws2.simulateOpen();
      ws2.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id-2',
        data: { peers: [] },
      });

      // Another disconnect - should start from attempt 1
      ws2.simulateClose(1006, '');
      vi.advanceTimersByTime(1500); // Should use base delay again

      expect(MockWebSocket.instances.length).toBe(3);
    });
  });

  describe('Error Handling', () => {
    it('should emit error event on server error message', () => {
      const events: AnySignalingEvent[] = [];
      client.subscribe((event) => events.push(event));

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      ws.simulateMessage({
        type: 'error',
        data: { message: 'Rate limit exceeded' },
      });

      const errorEvent = events.find((e) => e.type === 'error') as SignalingErrorEvent;
      expect(errorEvent).toBeDefined();
      expect(errorEvent.message).toBe('Rate limit exceeded');
    });

    it('should handle invalid JSON gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();

      // Directly trigger onmessage with invalid JSON
      ws.onmessage?.(new MessageEvent('message', { data: 'invalid json' }));

      expect(consoleError).toHaveBeenCalledWith(
        'Failed to parse signaling message:',
        'invalid json'
      );
      consoleError.mockRestore();
    });

    it('should handle unknown message types gracefully', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'unknown-type',
        data: {},
      });

      expect(consoleWarn).toHaveBeenCalledWith('Unknown signaling message type:', 'unknown-type');
      consoleWarn.mockRestore();
    });

    it('should handle subscriber errors gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      client.subscribe(errorSubscriber);
      client.subscribe(goodSubscriber);

      client.join('test-room');
      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      expect(errorSubscriber).toHaveBeenCalled();
      expect(goodSubscriber).toHaveBeenCalled();
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      client.subscribe(subscriber);

      client.join('test-room');

      expect(subscriber).toHaveBeenCalled();
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = client.subscribe(subscriber);

      client.join('test-room');
      expect(subscriber).toHaveBeenCalledTimes(1); // state_change to connecting

      unsubscribe();

      const ws = MockWebSocket.instances[0];
      ws.simulateOpen();
      ws.simulateMessage({
        type: 'join',
        peerId: 'my-peer-id',
        data: { peers: [] },
      });

      // Should not receive any more events after unsubscribe
      expect(subscriber).toHaveBeenCalledTimes(1);
    });
  });

  describe('Singleton', () => {
    it('should throw error when getting singleton without config', () => {
      expect(() => getSignalingClient()).toThrow('SignalingClient not initialized');
    });

    it('should return same instance from getSignalingClient', () => {
      const instance1 = getSignalingClient({
        serverUrl: 'https://test.example.com',
      });
      const instance2 = getSignalingClient();

      expect(instance1).toBe(instance2);
    });

    it('should create new instance after resetSignalingClient', () => {
      const instance1 = getSignalingClient({
        serverUrl: 'https://test.example.com',
      });
      resetSignalingClient();
      const instance2 = getSignalingClient({
        serverUrl: 'https://test2.example.com',
      });

      expect(instance1).not.toBe(instance2);
    });
  });
});
