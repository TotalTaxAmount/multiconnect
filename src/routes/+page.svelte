<script lang="ts">
  import DeviceList from "$lib/componets/DeviceList.svelte";
  import { theme } from "$lib/theme";
  import "../app.css";

  function toggleTheme() {
    theme.update((t) => (t === "dark" ? "light" : "dark"));
  }

  let currentTheme: string | null = null;
  $: theme.subscribe((value) => {
    currentTheme = value;
  });
</script>

<main
  class="bg-white min-h-screen text-black p-8 dark:bg-gray-800 dark:text-white"
>
  <button on:click={toggleTheme}>
    Click Toggle theme to {currentTheme === "dark" ? "Light" : "Dark"}
  </button>

  <DeviceList />
</main>
