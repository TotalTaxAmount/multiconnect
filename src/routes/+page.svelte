<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import type { Peer } from "../lib/types";
  import { sendPairingRequest } from "../lib/commands";

  let peers: Map<string, Peer> = new Map<string, Peer>();
  let loading = true;
  let pairingRequest: Peer | null = null;

  listen<Peer>('peer-found', (event) => {
    peers.set(event.payload.peer_id, event.payload);
    console.debug(`Found a peer, id = ${event.payload.peer_id}`);
    if (peers.size > 0 ) loading = false;
  });

  listen<string>('peer-expired', (event) => {
    console.debug(`Peer expired, id = ${event.payload}`);
    peers.delete(event.payload);
    if (peers.size <= 0) loading = true;
  });

  listen<Peer>('peer-pair', (event) => {
    pairingRequest = event.payload;
    console.debug(`New pairing request from: ${pairingRequest?.peer_id}`);
  });

  function closePopup() {
    pairingRequest = null;
  }

  function acceptPair() {
    // TODO
    console.log(`Pairing with ${pairingRequest?.peer_id}`);
    closePopup();
  }

  function rejectPair() {
    // TODO
    console.log(`Rejected pair rq from ${pairingRequest?.peer_id}`);
    closePopup();
  }
</script>

<main class="container">
  <h1 class="title">Peer List</h1>
  <ul class="peer-list">
    {#if loading}
      <li class="spinner-container">
        <div class="spinner"></div>
      </li>
    {:else}
      {#each peers.values() as peer}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <li class="peer" on:click={() => sendPairingRequest(peer)}>
          <strong>Peer ID:</strong> {peer.peer_id} <br />
          <strong>Multiaddr:</strong> {peer.multiaddr}
        </li>
      {/each}
    {/if}
  </ul>

  {#if pairingRequest}
    <div class="popup">
      <div class="popup-content">
        <h2>Pairing Request</h2>
        <p><strong>Peer ID:</strong> {pairingRequest.peer_id}</p>
        <p><strong>Multiaddr:</strong> {pairingRequest.multiaddr}</p>
        <button on:click={acceptPair}>Accept</button>
        <button on:click={rejectPair} class="reject">Reject</button>
      </div>
    </div>
  {/if}
</main>

<style>
  .container {
    font-size: 20px;
    padding: 20px;
  }

  .title {
    font-size: 24px;
    margin-bottom: 10px;
  }

  button {
    font-size: 18px;
    padding: 10px 20px;
    margin-bottom: 20px;
    cursor: pointer;
  }

  .peer-list {
    list-style-type: none;
    padding: 0;
  }

  .peer-list li {
    margin-bottom: 10px;
    background: #9f9999;
    padding: 10px;
    border-radius: 5px;
  }

  .peer-list .peer:hover {
    background: #6e6969;
    cursor: pointer;
  }

  .spinner-container {
    display: flex;
    justify-content: center;
    align-items: center;
    height: 100px;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 5px solid #ddd;
    border-top: 5px solid #333;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .popup {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .popup-content {
    background: white;
    padding: 20px;
    border-radius: 8px;
    text-align: center;
  }

  .popup button {
    margin: 10px;
    padding: 10px 20px;
    font-size: 18px;
    cursor: pointer;
  }

  .popup .reject {
    background: #ff4d4d;
    color: white;
    border: none;
  }

  .popup .reject:hover {
    background: #cc0000;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
