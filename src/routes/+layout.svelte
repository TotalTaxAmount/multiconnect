<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  let { children } = $props();

  let isDark: boolean = false;
  onMount(() => {
    const theme = localStorage.getItem("theme");
    if (theme === "dark") {
      document.documentElement.classList.add("dark");
      isDark = true;
    }
  });
</script>

<main
  class="bg-white min-h-screen text-black p-8 dark:bg-gray-800 dark:text-white"
>
  {@render children()}
</main>
