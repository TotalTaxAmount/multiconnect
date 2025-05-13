<script lang="ts">
  import { writable, get } from "svelte/store";
  import { listen } from "@tauri-apps/api/event";
  import { refreshDevices, sendPairingRequest } from "$lib/commands";
  import MdiPlusIcon from "~icons/mdi/plus";
  import MdiCloseIcon from "~icons/mdi/close";
  import type { Device } from "$lib/types";
  import { onMount } from "svelte";

  const pairedDevices = writable<
    Map<string, { device: Device; online: boolean; last_seen: number }>
  >(new Map());
  const discovered = writable<Map<string, Device>>(new Map());
  const overlayOpen = writable(false);

  function upsert(
    store: typeof pairedDevices,
    device: Device,
    online: boolean,
    last_seen: number,
  ) {
    store.update((m) => m.set(device.peer, { device, last_seen, online }));
  }
  function remove(store: typeof pairedDevices, peerId: string) {
    store.update((m) => {
      m.delete(peerId);
      return m;
    });
  }

  onMount(() => {
    refreshDevices();

    listen("device-status", (event: any) => {
      const [device, online, last_seen, paired] = event.payload as [
        Device,
        boolean,
        number,
        boolean,
      ];
      if (paired) {
        upsert(pairedDevices, device, online, last_seen);
      } else {
        remove(pairedDevices, device.peer);
      }
    });

    // discovery events
    listen("peer-found", (event: any) => {
      const device = event.payload as Device;
      discovered.update((m) => m.set(device.peer, device));
    });
    listen("peer-expired", (event: any) => {
      const peerId = event.payload as string;
      discovered.update((m) => {
        m.delete(peerId);
        return m;
      });
    });
  });

  function openOverlay() {
    overlayOpen.set(true);
  }
  function closeOverlay() {
    overlayOpen.set(false);
  }

  async function pairDevice(device: Device) {
    await sendPairingRequest(device);
  }
</script>

<div class="p-6 space-y-6">
  <h2 class="text-center text-3xl font-bold text-gray-900 dark:text-gray-100">
    Paired Devices
  </h2>

  {#if $pairedDevices.size === 0}
    <p class="text-center text-gray-500 dark:text-gray-400 italic">
      Pair some devices to get started
    </p>
  {:else}
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
      {#each Array.from($pairedDevices.values()) as paired (paired.device.peer)}
        <div
          class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow"
        >
          <div class="flex justify-between items-center">
            <h3 class="text-lg font-semibold text-gray-800 dark:text-gray-100">
              {paired.device.deviceName}
            </h3>
            <span
              class="text-sm px-2 py-1 rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300"
            >
              {paired.device.deviceType}
            </span>
          </div>
          <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
            OS: {paired.device.osName}
          </p>
          <p class="text-sm text-gray-500 dark:text-gray-400">
            Version: {paired.device.mcVersion}
          </p>
          <p class="text-xs text-gray-400 dark:text-gray-600 mt-2 break-all">
            Peer ID: {paired.device.peer}
          </p>
        </div>
      {/each}
    </div>
  {/if}

  <div class="mt-8 text-center">
    <button
      class="inline-flex flex-col items-center space-y-2 text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white"
      on:click={openOverlay}
    >
      <span class="font-medium">Add Device</span>
      <MdiPlusIcon class="w-12 h-12" />
    </button>
  </div>
</div>

{#if $overlayOpen}
  <div class="fixed inset-0 backdrop flex items-center justify-center p-4 z-50">
    <div
      class="bg-white dark:bg-gray-800 rounded-2xl w-full max-w-md max-h-full overflow-auto shadow-xl p-6 relative"
    >
      <button
        class="absolute top-4 right-4 p-2 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-full"
        on:click={closeOverlay}
      >
        <MdiCloseIcon class="w-6 h-6 text-gray-600 dark:text-gray-300" />
      </button>

      <h3 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-4">
        Pair a New Device
      </h3>

      {#if $discovered.size === 0}
        <p class="text-gray-500 dark:text-gray-400 italic">
          No devices discovered yet...
        </p>
      {:else}
        <div class="space-y-4">
          {#each Array.from($discovered.values()) as device (device.peer)}
            <div
              class="flex items-center justify-between bg-gray-50 dark:bg-gray-700 rounded-lg p-4"
            >
              <div>
                <div class="font-medium text-gray-800 dark:text-gray-100">
                  {device.deviceName}
                </div>
                <div class="text-sm text-gray-500 dark:text-gray-400">
                  {device.deviceType} &bull; {device.osName}
                </div>
              </div>
              <button
                class="px-4 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors"
                on:click={() => pairDevice(device)}
              >
                Pair
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    background: rgba(0, 0, 0, 0.5);
  }
</style>
