<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    hover?: boolean;
    onclick?: () => void;
    children: Snippet;
  }

  let { hover = false, onclick, children }: Props = $props();
</script>

<div
  class="bg-bg-secondary border border-border rounded-xl p-5 transition-all duration-200
    {hover ? 'cursor-pointer hover:border-accent hover:shadow-[0_0_20px_rgba(34,197,94,0.15)]' : ''}"
  onclick={onclick}
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onkeydown={onclick ? (e: KeyboardEvent) => { if (e.key === 'Enter') onclick?.() } : undefined}
>
  {@render children()}
</div>
