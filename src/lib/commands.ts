import { invoke } from "@tauri-apps/api/core";
import type { Peer } from "./types";

/**
 * Send a paring request the daemon
 * @param peer The peer to pair with;
 */
export function sendPairingRequest(peer: Peer) {
  invoke('send_pairing_request', { peer: peer });
}