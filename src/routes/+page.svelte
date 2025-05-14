<script lang="ts">
  import { theme } from "$lib/theme";
  import "../app.css";
  import { writable } from "svelte/store";
  import { listen } from "@tauri-apps/api/event";
  import {
    refreshDevices,
    sendPairingRequest,
    sendPairingResponse,
  } from "$lib/commands";
  import MdiPlusIcon from "~icons/mdi/plus";
  import MdiCloseIcon from "~icons/mdi/close";
  import { onMount } from "svelte";
  import type { Device } from "$lib/types";
  import { pairedDevices } from "$lib/stores";

  const discovered = writable<Map<string, Device>>(new Map());
  const overlayOpen = writable(false);
  const peerPairRequest = writable<{ device: Device; uuid: string } | null>(
    null,
  );

  const pairingStatus = writable<Record<string, "idle" | "pending" | "denied">>(
    {},
  );
  const pairingRequests = writable<Record<string, string>>({});

  function toggleTheme() {
    theme.update((t) => (t === "dark" ? "light" : "dark"));
  }

  let currentTheme: string | null = null;
  $: theme.subscribe((value) => {
    currentTheme = value;
  });

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

  onMount(async () => {
    listen("device-status", (event: any) => {
      const [device, online, last_seen] = event.payload as [
        Device,
        boolean,
        number,
      ];
      discovered.update((d) => {
        d.delete(device.peer);
        return d;
      });
      upsert(pairedDevices, device, online, last_seen);
    });

    listen("peer-pair-request", (event: any) => {
      const [device, uuid] = event.payload as [Device, string];
      console.debug(`Pair request: device = ${device}, uuid = ${uuid}`);
      peerPairRequest.set({ device, uuid });
    });

    listen("peer-found", (event: any) => {
      const device = event.payload as Device;
      console.debug(`Device found: device = ${device}`);

      discovered.update((m) => m.set(device.peer, device));
    });

    listen("peer-expired", (event: any) => {
      const peerId = event.payload as string;
      console.debug(`Device expired: id = ${peerId}`);

      discovered.update((m) => {
        m.delete(peerId);
        return m;
      });
    });

    listen("pair-response", async (event: any) => {
      const [uuid, accepted] = event.payload as [string, boolean];

      pairingRequests.update((map) => {
        const peerId = map[uuid];
        delete map[uuid];

        if (peerId) {
          pairingStatus.update((s) => {
            s[peerId] = accepted ? "idle" : "denied";
            return { ...s };
          });

          if (accepted) {
            console.log("wtf");
            discovered.update((d) => {
              d.delete(peerId);
              return d;
            });
          }
        }

        return map;
      });
    });

    await refreshDevices();
  });

  function openOverlay() {
    overlayOpen.set(true);
  }
  function closeOverlay() {
    overlayOpen.set(false);
  }

  async function pairDevice(device: Device) {
    pairingStatus.update((s) => ({ ...s, [device.peer]: "pending" }));

    const uuid = await sendPairingRequest(device);

    pairingRequests.update((r) => ({ ...r, [uuid]: device.peer }));
  }
</script>

<div class="container mx-auto">
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
          <a
            href={`/device/${paired.device.peer}`}
            class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-2xl p-5 shadow-sm hover:shadow-md transition-shadow"
          >
            <div class="flex justify-between items-center">
              <div class="flex items-center gap-2">
                <span
                  class="inline-block w-2.5 h-2.5 rounded-full"
                  class:bg-green-500={paired.online}
                  class:bg-gray-400={!paired.online}
                ></span>
                <h3
                  class="text-lg font-semibold text-gray-800 dark:text-gray-100"
                >
                  {paired.device.deviceName}
                </h3>
              </div>
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
          </a>
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
    <div
      class="fixed inset-0 bg-black/70 backdrop-blur-lm flex items-center justify-center p-4 z-50"
    >
      <div
        class="bg-white dark:bg-gray-800 rounded-2xl w-full max-w-md max-h-full overflow-auto shadow-xl p-6 relative"
      >
        <button
          class="absolute top-4 right-4 p-2 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-full"
          on:click={closeOverlay}
        >
          <MdiCloseIcon class="w-6 h-6 text-gray-600 dark:text-gray-300" />
        </button>

        <h3
          class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-4"
        >
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
                  {#if $pairingStatus[device.peer] === "denied"}
                    <p class="text-sm text-red-500 mt-1">Request denied</p>
                  {/if}
                </div>
                <button
                  class="px-4 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50 flex items-center gap-2"
                  on:click={() => pairDevice(device)}
                  disabled={$pairingStatus[device.peer] === "pending"}
                >
                  {#if $pairingStatus[device.peer] === "pending"}
                    <svg
                      class="animate-spin h-4 w-4 text-white"
                      xmlns="http://www.w3.org/2000/svg"
                      fill="none"
                      viewBox="0 0 24 24"
                    >
                      <circle
                        class="opacity-25"
                        cx="12"
                        cy="12"
                        r="10"
                        stroke="currentColor"
                        stroke-width="4"
                      ></circle>
                      <path
                        class="opacity-75"
                        fill="currentColor"
                        d="M4 12a8 8 0 018-8v4l3-3-3-3v4a8 8 0 00-8 8h4z"
                      ></path>
                    </svg>
                    Pairing...
                  {:else}
                    Pair
                  {/if}
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if $peerPairRequest}
    <div
      class="fixed inset-0 bg-black/70 backdrop-blur-lm flex items-center justify-center z-50 p-4"
    >
      <div
        class="bg-white dark:bg-gray-800 rounded-2xl w-full max-w-md p-6 shadow-xl relative"
      >
        <h3 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">
          Incoming Pair Request
        </h3>

        <div class="space-y-2">
          <p class="text-gray-700 dark:text-gray-300">
            <strong>Device:</strong>
            {$peerPairRequest.device.deviceName}
          </p>
          <p class="text-gray-600 dark:text-gray-400">
            <strong>Type:</strong>
            {$peerPairRequest.device.deviceType}
          </p>
          <p class="text-gray-600 dark:text-gray-400">
            <strong>OS:</strong>
            {$peerPairRequest.device.osName}
          </p>
          <p class="text-xs text-gray-500 dark:text-gray-500 break-all">
            <strong>Peer ID:</strong>
            {$peerPairRequest.device.peer}
          </p>
        </div>

        <div class="flex justify-end gap-3 mt-6">
          <button
            class="px-4 py-2 rounded-lg bg-gray-300 dark:bg-gray-600 text-gray-900 dark:text-gray-100 hover:bg-gray-400 dark:hover:bg-gray-500"
            on:click={async () => {
              sendPairingResponse(false, $peerPairRequest.uuid);
              peerPairRequest.set(null);
              await refreshDevices();
            }}
          >
            Deny
          </button>
          <button
            class="px-4 py-2 rounded-lg bg-green-600 text-white hover:bg-green-700"
            on:click={async () => {
              sendPairingResponse(true, $peerPairRequest.uuid);
              peerPairRequest.set(null);
              await refreshDevices();
            }}
          >
            Accept
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
