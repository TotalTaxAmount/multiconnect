import { invoke } from "@tauri-apps/api/core";
import type { Peer } from "./types";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export function sendPairingRequest(peer: Peer) {
  invoke('send_pairing_request', { peer: peer });
}

export function sendPairingResponse(accepted: boolean, req_id: number) {
  invoke('send_pairing_response', { accepted: accepted, reqId: req_id })
}