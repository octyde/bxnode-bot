<script lang="ts">
  import type { ChatMessage } from "$lib/types";
  import { Bot, User } from "lucide-svelte";

  interface Props {
    message: ChatMessage;
  }

  let { message }: Props = $props();
</script>

<div class="flex gap-3 {message.role === 'user' ? 'flex-row-reverse' : ''}">
  <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0
    {message.role === 'user' ? 'bg-blue-500/15 text-blue-400' : 'bg-accent/15 text-accent'}">
    {#if message.role === "user"}
      <User size={16} />
    {:else}
      <Bot size={16} />
    {/if}
  </div>
  <div class="max-w-[70%] px-4 py-3 rounded-xl text-sm leading-relaxed
    {message.role === 'user'
      ? 'bg-blue-500/10 border border-blue-500/20 text-slate-200'
      : 'bg-bg-tertiary border border-border text-slate-300'}">
    <pre class="whitespace-pre-wrap font-sans">{message.content}</pre>
  </div>
</div>
