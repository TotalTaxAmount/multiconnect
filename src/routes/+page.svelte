<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";

  type Peer = { peer_id: string; multiaddr: string };
  let peers: Map<string, Peer> = new Map<string, Peer>();
  let loading = true;

  listen<Peer>('peer-found', (event) => {
    peers.set(event.payload.peer_id, event.payload);
    if (peers.size > 0 ) loading = false;
  });

  listen<Peer>('peer-expired', (event) => {
    console.log("peer expired");
    peers.delete(event.payload.peer_id);
    if (peers.size <= 0) loading = true;
  });
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
        <li>
          <strong>Peer ID:</strong> {peer.peer_id} <br />
          <strong>Multiaddr:</strong> {peer.multiaddr}
        </li>
      {/each}
    {/if}
  </ul>
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

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
