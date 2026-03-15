<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "lucide-svelte";

  interface Props {
    open?: boolean;
    title?: string;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  }

  let { open = false, title = "", onclose, children, footer }: Props = $props();

  function handleBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) onclose?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose?.();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4 z-50 animate-fade-in"
    onclick={handleBackdrop}
    role="dialog"
    aria-modal="true"
  >
    <div class="bg-bg-secondary border border-border rounded-2xl max-w-2xl w-full max-h-[85vh] overflow-y-auto animate-scale-in">
      <div class="flex items-center justify-between px-6 py-4 border-b border-border sticky top-0 bg-bg-secondary z-10">
        <h3 class="font-semibold text-lg font-mono">{title}</h3>
        <button class="p-1.5 rounded-lg text-slate-400 hover:bg-bg-tertiary hover:text-slate-200 transition-colors" onclick={onclose}>
          <X size={18} />
        </button>
      </div>
      <div class="p-6">
        {@render children()}
      </div>
      {#if footer}
        <div class="flex justify-end gap-3 px-6 py-4 border-t border-border sticky bottom-0 bg-bg-secondary">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .animate-fade-in {
    animation: fadeIn 200ms ease;
  }
  .animate-scale-in {
    animation: scaleIn 200ms ease;
  }
  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes scaleIn {
    from { opacity: 0; transform: scale(0.95) translateY(10px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }
</style>
