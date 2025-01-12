<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type Peer = { peer_id: string; multiaddr: string };
  let peers: Peer[] = [];

  function listPeers() {
    invoke<Peer[]>('list_peers')
    .then((res: Peer[]) => {
      peers = res;
    })
    .catch((e) => {
      console.error("Failed to fetch peers: ", e);
    });
  }

  onMount(() => {
    listPeers();
  })
</script>

<main class="container">
  <h1 class="title">Peer List</h1>
  <button on:click={listPeers}>List Peers</button>

  <ul class="peer-list">
    {#if peers.length > 0}
      {#each peers as peer}
        <li>
          <strong>Peer ID:</strong> {peer.peer_id} <br />
          <strong>Multiaddr:</strong> {peer.multiaddr}
        </li>
      {/each}
    {:else}
      <li>No peers found.</li>
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
    background: #f3f3f3;
    padding: 10px;
    border-radius: 5px;
  }
</style>
