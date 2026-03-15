<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    disabled?: boolean;
    onclick?: () => void;
    children: Snippet;
  }

  let { variant = "primary", size = "md", disabled = false, onclick, children }: Props = $props();

  const base = "inline-flex items-center gap-1.5 font-medium rounded-lg transition-all duration-200 cursor-pointer whitespace-nowrap disabled:opacity-50 disabled:cursor-not-allowed";
  const variants: Record<string, string> = {
    primary: "bg-accent text-bg-primary hover:bg-accent-hover hover:shadow-[0_0_20px_rgba(34,197,94,0.15)]",
    secondary: "bg-bg-tertiary text-slate-200 border border-border hover:bg-bg-elevated hover:border-border-hover",
    ghost: "bg-transparent text-slate-400 hover:bg-bg-tertiary hover:text-slate-200",
    danger: "bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20",
  };
  const sizes: Record<string, string> = {
    sm: "px-3 py-1.5 text-xs",
    md: "px-4 py-2 text-sm",
  };
</script>

<button
  class="{base} {variants[variant]} {sizes[size]}"
  {disabled}
  {onclick}
>
  {@render children()}
</button>
