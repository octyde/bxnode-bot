<script lang="ts">
  import { CheckCircle, XCircle, AlertTriangle } from "lucide-svelte";
  import { appStore } from "$lib/stores/app.svelte";

  const icons = {
    success: CheckCircle,
    error: XCircle,
    warning: AlertTriangle,
  };

  const borderColors = {
    success: "border-l-accent",
    error: "border-l-red-500",
    warning: "border-l-amber-500",
  };

  const iconColors = {
    success: "text-accent",
    error: "text-red-400",
    warning: "text-amber-400",
  };
</script>

{#if appStore.toasts.length > 0}
  <div class="fixed bottom-6 right-6 flex flex-col gap-2 z-[300]">
    {#each appStore.toasts as toast (toast.id)}
      <div class="bg-bg-secondary border border-border border-l-[3px] {borderColors[toast.type]} rounded-lg px-4 py-3 flex items-center gap-3 min-w-[280px] shadow-xl animate-slide-in">
        <span class="{iconColors[toast.type]}">
          <svelte:component this={icons[toast.type]} size={18} />
        </span>
        <span class="text-sm flex-1">{toast.message}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .animate-slide-in {
    animation: slideIn 300ms ease;
  }
  @keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>
