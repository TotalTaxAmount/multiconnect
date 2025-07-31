import { invoke } from "@tauri-apps/api/core";
import type { Device } from "./types";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export async function sendPairingRequest(target: Device) : Promise<string> {
  return await invoke('send_pairing_request', { target });
}

/**
 * Send a pairing request to a peer
 * @param accepted If the request is accepted or not
 * @param req_id The id of the pair request
 * @param peer 
 */
export async function sendPairingResponse(accepted: boolean, req_uuid: string) {
  await invoke('send_pairing_response', { accepted, uuid: req_uuid});
}

/**
 * Refresh peers
 */
export async function refreshDevices() {
  await invoke('refresh_devices');
}

export async function sendFile(peer: String, filePath: String) {
  await invoke('send_file', { peer, filePath })
  
}

export async function getTheme(): Promise<string> {
   return await invoke('get_theme');
}

export async function setTheme(theme: string) {
  await invoke('set_theme', { theme });
}

export async function streamTest(peer: String) {
  await invoke('stream_test', { peer });
}