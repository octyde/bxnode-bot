<script lang="ts">
  import { Send } from "lucide-svelte";
  import Button from "$lib/components/ui/Button.svelte";

  interface Props {
    disabled?: boolean;
    onsend?: (message: string) => void;
  }

  let { disabled = false, onsend }: Props = $props();
  let value = $state("");

  function handleSend() {
    const msg = value.trim();
    if (!msg) return;
    onsend?.(msg);
    value = "";
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<div class="flex gap-3 items-end p-4 border-t border-border bg-bg-secondary">
  <textarea
    bind:value
    {disabled}
    placeholder="Type a message..."
    rows={1}
    class="flex-1 px-4 py-3 bg-bg-primary border border-border rounded-xl text-sm text-slate-200
      placeholder:text-slate-500 resize-none focus:outline-none focus:border-accent
      focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)] transition-all duration-200"
    onkeydown={handleKeydown}
  ></textarea>
  <Button onclick={handleSend} {disabled}>
    <Send size={16} />
    Send
  </Button>
</div>
