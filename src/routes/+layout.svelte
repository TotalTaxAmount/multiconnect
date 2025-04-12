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

{@render children()}
