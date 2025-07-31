<script lang="ts">
  import { debugCmd } from "$lib/commands";

  export let peerId: string;

  async function openStream() {
    await debugCmd(`open-stream:${peerId}`);
  }

  async function sendPing() {
    await debugCmd(`send-ping:${peerId}`);
  }

  async function closeStream() {
    await debugCmd(`close-stream:${peerId}`);
  }
</script>

<div class="container">
  <button on:click={openStream}>Open Stream</button>
  <button on:click={closeStream}>Close Stream</button>
  <button on:click={sendPing}>Send Ping</button>
</div>
