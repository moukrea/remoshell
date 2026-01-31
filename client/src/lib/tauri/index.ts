/**
 * Tauri module exports
 */

export {
  TauriIPCBridge,
  getTauriIPCBridge,
  resetTauriIPCBridge,
  TauriNotAvailableError,
  TauriCommandError,
  uint8ArrayToBase64,
  base64ToUint8Array,
  type ChannelType,
  type ConnectionState,
  type CommandError,
  type ConnectRequest,
  type ConnectResponse,
  type DisconnectResponse,
  type SendDataRequest,
  type SendDataResponse,
  type ConnectionStatusResponse,
  type DeviceKeysResponse,
  type PairedDevice,
  type StorePairedDeviceRequest,
  type RemoveDeviceResponse,
  type NotificationRequest,
  type NotificationResponse,
  type InitRequest,
  type InitResponse,
  type ConnectionEvent,
  type TauriEventSubscriber,
} from './TauriIPCBridge';
