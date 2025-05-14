<script lang="ts">
  import { goto } from "$app/navigation";
  import { pairedDevices } from "$lib/stores";
  import { get } from "svelte/store";

  export let data: { peer: string };

  const peerId = data.peer;
  const deviceEntry = get(pairedDevices).get(peerId);

  function goBack() {
    goto("/");
  }
</script>

<div class="p-6 max-w-3xl mx-auto">
  <button
    on:click={goBack}
    class="mb-4 px-4 py-2 bg-gray-200 dark:bg-gray-700 text-sm rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition"
  >
    ← Back
  </button>

  {#if deviceEntry}
    <h1 class="text-2xl font-bold mb-2">{deviceEntry.device.deviceName}</h1>
    <p class="text-gray-600 dark:text-gray-300">
      <strong>Type:</strong>
      {deviceEntry.device.deviceType}
    </p>
    <p class="text-gray-600 dark:text-gray-300">
      <strong>OS:</strong>
      {deviceEntry.device.osName}
    </p>
    <p class="text-gray-600 dark:text-gray-300">
      <strong>Online:</strong>
      {deviceEntry.online ? "Yes" : "No"}
    </p>
    <p class="text-gray-500 text-sm mt-2 break-all">
      <strong>Peer ID:</strong>
      {deviceEntry.device.peer}
    </p>

    <div class="mt-6">
      <h2 class="text-xl font-semibold mb-2">Modules</h2>
    </div>
  {:else}
    <p class="text-red-500">Device not found or no longer paired.</p>
  {/if}
</div>
