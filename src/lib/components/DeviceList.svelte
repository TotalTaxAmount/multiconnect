<script lang="ts">
  import { refreshDevices, sendPairingRequest } from "$lib/commands";
  import { Device } from "$lib/types";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { writable } from "svelte/store";

  const devices = writable<Map<string, Device>>(new Map());
  const isPairingScreenVisible = writable(false);

  onMount(() => {
    const unsubs: (() => void)[] = [];

    const setup = async () => {
      await refreshDevices();
    };

    setup();

    return () => {
      unsubs.forEach((unsub) => unsub());
    };
  });

  const togglePairingScreen = () => {
    isPairingScreenVisible.update((prev) => !prev);
  };
</script>

<div class="p-6 space-y-6">
  <h2 class="text-center text-3xl font-bold text-gray-900 dark:text-gray-100">
    Pairied Devices
  </h2>

  {#if $devices.size === 0}
    <p class="text-center text-gray-500 dark:text-gray-400 italic">
      Pair some devies to get started
    </p>
  {:else}
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
      {#each Array.from($devices.values()) as device (device.peer)}
        <div
          class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow"
        >
          <div class="flex justify-between items-center">
            <h3 class="text-lg font-semibold text-gray-800 dark:text-gray-100">
              {device.deviceName}
            </h3>
            <span
              class="text-sm px-2 py-1 rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300"
            >
              {device.deviceType}
            </span>
          </div>

          <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
            OS: {device.osName}
          </p>
          <p class="text-sm text-gray-500 dark:text-gray-400">
            Version: {device.mcVersion}
          </p>
          <p class="text-xs text-gray-400 dark:text-gray-600 mt-2 break-all">
            Peer ID: {device.peer}
          </p>

          <div class="mt-4 flex justify-end">
            <button
              class="px-4 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors"
              on:click={async () => {
                await sendPairingRequest(device);
              }}
            >
              Pair
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
