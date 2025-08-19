<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { writable } from "svelte/store";
  import { sendFile } from "$lib/commands";

  export let peerId: string;

  type TransferDirection = "inbound" | "outbound";

  type Transfer = {
    uuid: string;
    direction: TransferDirection;
    fileName: string;
    done: number;
    total: number;
    status: string;
  };

  const transfers = writable<Map<string, Transfer>>(new Map());

  function updateTransfer(
    uuid: string,
    direction: TransferDirection,
    fileName: string,
    partial: Partial<Transfer>,
  ) {
    transfers.update((map) => {
      const existing = map.get(uuid) ?? {
        uuid,
        direction,
        fileName,
        done: 0,
        total: 0,
        status: "Pending",
      };
      map.set(uuid, { ...existing, ...partial });
      return new Map(map); // force reactivity
    });
  }

  async function pickAndSendFile() {
    const selected = await open({
      multiple: false,
      title: "Select file to send",
    });

    if (typeof selected === "string") {
      const path = selected;
      const name = path.split(/[\\/]/).pop();
      if (name) {
        // // Optimistically create outbound transfer
        // // The backend will later emit the uuid + progress
        // updateTransfer(crypto.randomUUID(), "outbound", name, {
        //   status: "Preparing",
        // });
        await sendFile(peerId, path);
      }
    }
  }

  onMount(() => {
    const unsubs: Array<() => void> = [];

    // Progress listener
    const listenToProgress = async (direction: TransferDirection) => {
      const un = await listen<[string, string, number, number]>(
        `file_transfer/${direction}_progress`,
        (event) => {
          const [uuid, fileName, total, done] = event.payload;
          console.log(
            `File transfer progress for ${fileName} (${direction}): ${done}/${total} (uuid=${uuid})`,
          );
          updateTransfer(uuid, direction, fileName, { done, total });
        },
      );
      unsubs.push(un);
    };

    // Status listener
    const listenToStatus = async () => {
      const un = await listen<[string, string]>(
        "file_transfer/status",
        (event) => {
          const [uuid, status] = event.payload;
          transfers.update((map) => {
            const existing = map.get(uuid);
            if (existing) {
              map.set(uuid, { ...existing, status });
            }
            return new Map(map);
          });
        },
      );
      unsubs.push(un);
    };

    listenToProgress("inbound");
    listenToProgress("outbound");
    listenToStatus();

    return () => unsubs.forEach((un) => un());
  });
</script>

<button
  type="button"
  class="w-full text-left p-4 rounded-2xl shadow-md bg-white dark:bg-gray-800 transition hover:shadow-lg focus:outline-none focus:ring-2 focus:ring-blue-500 mb-4"
  on:click={pickAndSendFile}
>
  <h3 class="text-lg font-semibold mb-2">File Transfer</h3>
  <p class="text-gray-600 dark:text-gray-300 text-sm">Click to send a file</p>
</button>

{#each Array.from($transfers.values()) as transfer (transfer.uuid)}
  <div class="mb-3 p-3 rounded-xl bg-gray-100 dark:bg-gray-700 shadow-inner">
    <div class="text-sm text-gray-800 dark:text-gray-200">
      <div>
        <strong
          >{transfer.direction === "outbound"
            ? "Sending"
            : "Receiving"}:</strong
        >
        {transfer.fileName}
      </div>

      <div class="w-full bg-gray-300 dark:bg-gray-600 rounded-full h-2 mt-1">
        <div
          class="bg-blue-500 h-2 rounded-full transition-all"
          style="width: {transfer.total > 0
            ? (transfer.done / transfer.total) * 100
            : 0}%"
        ></div>
      </div>

      <div class="text-xs mt-1">
        {transfer.done} / {transfer.total} bytes
      </div>
      <div class="text-xs">
        <strong>Status:</strong>
        {transfer.status}
      </div>
    </div>
  </div>
{/each}
