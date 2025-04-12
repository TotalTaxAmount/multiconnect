import { invoke } from "@tauri-apps/api/core";
import type { Device } from "./types";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export function sendPairingRequest(device: Device) {
  invoke('send_pairing_request', { device });
}

/**
 * Send a pairing request to a peer
 * @param accepted If the request is accepted or not
 * @param req_id The id of the pair request
 * @param peer 
 */
export function sendPairingResponse(accepted: boolean, req_uuid: string) {
  invoke('send_pairing_response', { accepted, reqUuid: req_uuid})
}

/**
 * Refresh peers
 */
export function refreshMdns() {
  invoke('refresh_mdns');
}