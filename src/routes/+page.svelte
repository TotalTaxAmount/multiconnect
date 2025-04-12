<script lang="ts">
  import { theme } from "$lib/theme";
  import "../app.css";

  function toggleTheme() {
    theme.update((t) => (t === "dark" ? "light" : "dark"));
  }
</script>

<main
  class="bg-white min-h-screen text-black p-8 dark:bg-gray-800 dark:text-white"
></main>

<style lang="postcss">
</style>
