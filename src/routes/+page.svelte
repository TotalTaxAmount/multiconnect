<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  type Peer = { peer_id: string; multiaddr: string };
  let peers: Peer[] = [];
  let loading = true;

  function listPeers() {
    invoke<Peer[]>('list_peers')
      .then((res: Peer[]) => {
        peers = res;
        if (peers.length > 0) {
          loading = false;
        }
      })
      .catch((e) => {
        console.error("Failed to fetch peers: ", e);
      });
  }

  onMount(() => {
    listPeers();
    const interval = setInterval(() => {
      listPeers();
    }, 3000);

    onDestroy(() => {
      clearInterval(interval);
    });
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
      {#each peers as peer}
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
