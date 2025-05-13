<script lang="ts">
  import IconMdiPlus from "virtual:icons/mdi/plus";
  import { refreshPeers, sendPairingRequest } from "$lib/commands";
  import { Device } from "$lib/types";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { writable } from "svelte/store";

  const peers = writable<Map<string, Device>>(new Map());

  onMount(() => {
    const unsubs: (() => void)[] = [];

    const setup = async () => {
      unsubs.push(
        await listen<Device>("peer-found", (event) => {
          const device = event.payload;
          console.debug(`Device: ${device.deviceName}`);
          peers.update((map) => {
            map.set(device.peer, device);
            return new Map(map);
          });
        }),
      );

      unsubs.push(
        await listen<string>("peer-expired", (event) => {
          console.debug(`Device: ${event.payload}`);
          peers.update((map) => {
            map.delete(event.payload);
            return new Map(map);
          });
        }),
      );

      unsubs.push(
        await listen<Device>("peer-pair-request", (event) => {
          console.debug(`Pair request from ${event.payload}`);
          // TODO: Do something?
        }),
      );

      await refreshPeers();
    };

    setup();

    return () => {
      unsubs.forEach((unsub) => unsub());
    };
  });
</script>

<div class="p-6 space-y-6">
  <h2 class="text-2xl font-bold text-gray-900 dark:text-gray-100">
    Devices Nearby
  </h2>

  {#if $peers.size === 0}
    <p class="text-gray-500 dark:text-gray-400 italic">
      No devices discovered.
    </p>
  {:else}
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
      {#each Array.from($peers.values()) as device (device.peer)}
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

  <div class="flex justify-center items-center h-20">
    <button class="rounded-lg px-4 text-2xl text-gray-900 dark:text-gray-100"
      >Add devices</button
    >
    <IconMdiPlus />
  </div>
</div>
