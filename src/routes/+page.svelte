<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { Device, DeviceType } from "../lib/types";
  import {
    refreshMdns,
    sendPairingRequest,
    sendPairingResponse,
  } from "../lib/commands";

  let devices: Map<string, Device> = new Map<string, Device>();
  let loading = true;
  let pairingRequest: [string, Device] | null = null;

  listen<Device>("peer-found", (event) => {
    devices.set(event.payload.peer, event.payload);
    console.debug(`Found a peer, id = ${event.payload.peer}`);
    if (devices.size > 0) loading = false;
  });

  listen<string>("peer-expired", (event) => {
    console.debug(`Peer expired, id = ${event.payload}`);
    devices.delete(event.payload);
    if (devices.size <= 0) loading = true;
  });

  listen<[string, Device]>("pair-request", (event) => {
    pairingRequest = event.payload;
    console.debug(`New pairing request from: ${pairingRequest?.[1].peer}`);
  });

  function closePopup() {
    pairingRequest = null;
  }

  function acceptPair() {
    if (!pairingRequest) return;
    console.log(`Pairing with ${pairingRequest?.[1].peer}`);
    sendPairingResponse(true, pairingRequest[0]);
    closePopup();
  }

  function rejectPair() {
    if (!pairingRequest) return;
    console.log(`Rejected pair rq from ${pairingRequest?.[1].peer}`);
    sendPairingResponse(false, pairingRequest[0]);
    closePopup();
  }
</script>

<main class="container">
  <h1 class="title">Peer List</h1>
  <div class="discovery">
    <ul class="peer-list">
      {#if loading}
        <li class="spinner-container">
          <div class="spinner"></div>
        </li>
      {:else}
        {#each devices.values() as device}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <li class="peer" on:click={() => sendPairingRequest(device)}>
            <strong>Peer ID:</strong>
            {device.peer}
            {device.deviceName}
            {device.osName}
            {device.mcVersion}
            {device.deviceType}
          </li>
        {/each}
      {/if}
    </ul>

    <button on:click={() => refreshMdns()}>Refresh</button>
  </div>

  {#if pairingRequest}
    <div class="popup">
      <div class="popup-content">
        <h2>Pairing Request</h2>
        <p><strong>Peer ID:</strong> {pairingRequest[1].peer}</p>
        <p>
          <strong>Other shit:</strong>
          {pairingRequest[1].deviceName}
          {pairingRequest[1].osName}
          {pairingRequest[1].mcVersion}
          {pairingRequest[1].osName}
        </p>
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

  .discovery {
    display: block;
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
