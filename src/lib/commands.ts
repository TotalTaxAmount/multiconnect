import { invoke } from "@tauri-apps/api/core";
import type { Device } from "./types";
import logger from "./logger";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export function sendPairingRequest(device: Device) {
  logger.info(`Sending pairing request to: ${device.peer}`)
  invoke('send_pairing_request', { device });
}

/**
 * Send a pairing request to a peer
 * @param accepted If the request is accepted or not
 * @param req_id The id of the pair request
 * @param peer 
 */
export function sendPairingResponse(accepted: boolean, req_uuid: string) {
  logger.info(`Sending pairing response for request ${req_uuid}, status = ${accepted}`)
  invoke('send_pairing_response', { accepted, reqUuid: req_uuid})
}

/**
 * Refresh peers
 */
export function refreshMdns() {
  logger.info("Refreshing peers");
  invoke('refresh_mdns');
}