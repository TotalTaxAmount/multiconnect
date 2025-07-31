<script lang="ts">
  import { streamTest } from "$lib/commands";

  export let peerId: string;

  async function debug() {
    await streamTest(peerId);
  }
</script>

<div class="container">
  <button on:click={debug}> Test </button>
</div>
