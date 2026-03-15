<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "success" | "muted" | "warning" | "error";
    children: Snippet;
  }

  let { variant = "success", children }: Props = $props();

  const variants: Record<string, string> = {
    success: "bg-accent/15 text-accent",
    muted: "bg-slate-500/15 text-slate-400",
    warning: "bg-amber-500/15 text-amber-400",
    error: "bg-red-500/15 text-red-400",
  };
</script>

<span class="px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-wider {variants[variant]}">
  {@render children()}
</span>
