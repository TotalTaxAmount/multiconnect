import { invoke } from "@tauri-apps/api/core";
import type { Device } from "./types";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export async function sendPairingRequest(device: Device) {
  await invoke('send_pairing_request', { device });
}

/**
 * Send a pairing request to a peer
 * @param accepted If the request is accepted or not
 * @param req_id The id of the pair request
 * @param peer 
 */
export async function sendPairingResponse(accepted: boolean, req_uuid: string) {
  await invoke('send_pairing_response', { accepted, reqUuid: req_uuid});
}

/**
 * Refresh peers
 */
export async function refreshPeers() {
  await invoke('refresh_peers');
}

export async function getTheme(): Promise<string> {
   return await invoke('get_theme');
}

export async function setTheme(theme: string) {
  await invoke('set_theme', { theme: theme });
}