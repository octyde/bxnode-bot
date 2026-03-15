<script lang="ts">
  interface Props {
    title: string;
    subtitle?: string;
  }

  let { title, subtitle }: Props = $props();
</script>

<header class="flex items-center justify-between px-6 py-4 border-b border-border bg-bg-secondary/50">
  <div>
    <h1 class="text-lg font-semibold">{title}</h1>
    {#if subtitle}
      <p class="text-sm text-slate-500">{subtitle}</p>
    {/if}
  </div>
  <slot />
</header>
