/**
 * Device
 */
export class Device {
  peer: string;
  osName: string;
  deviceName: string;
  mcVersion: string;
  deviceType: DeviceType;

  constructor(
    peer: string,
    osName: string,
    deviceName: string,
    mcVersion: string,
    deviceType: DeviceType
  ) {
    this.peer = peer;
    this.osName = osName;
    this.deviceName = deviceName;
    this.mcVersion = mcVersion;
    this.deviceType = deviceType;
  }
}

/**
 * Different device types
 */
export enum DeviceType {
  Android = 0,
  Laptop = 1,
  Desktop = 2
}