<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ComponentType } from "svelte";

  interface Props {
    icon?: ComponentType;
    title: string;
    description?: string;
    children?: Snippet;
  }

  let { icon, title, description, children }: Props = $props();
</script>

<div class="text-center py-16 px-8 text-slate-500">
  {#if icon}
    <span class="inline-block mb-4 opacity-40">
      <svelte:component this={icon} size={56} />
    </span>
  {/if}
  <h3 class="text-base font-medium text-slate-400 mb-2">{title}</h3>
  {#if description}
    <p class="text-sm">{description}</p>
  {/if}
  {#if children}
    <div class="mt-4">
      {@render children()}
    </div>
  {/if}
</div>
