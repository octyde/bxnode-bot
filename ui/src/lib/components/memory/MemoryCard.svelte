<script lang="ts">
  import type { MemoryRecord } from "$lib/types";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { Trash2, Tag } from "lucide-svelte";
  import { timeAgo } from "$lib/utils/format";

  interface Props {
    record: MemoryRecord;
    ondelete?: () => void;
  }

  let { record, ondelete }: Props = $props();
</script>

<div class="bg-bg-secondary border border-border rounded-xl p-5 space-y-3">
  <div class="flex items-start justify-between gap-3">
    <p class="text-sm text-slate-300 leading-relaxed line-clamp-3">{record.content}</p>
    <Button variant="ghost" size="sm" onclick={ondelete}>
      <Trash2 size={14} />
    </Button>
  </div>

  {#if record.summary}
    <p class="text-xs text-slate-500 italic">{record.summary}</p>
  {/if}

  <div class="flex items-center gap-3 flex-wrap">
    {#each record.tags as tag}
      <span class="flex items-center gap-1 text-[11px] text-slate-500 bg-bg-tertiary px-2 py-0.5 rounded">
        <Tag size={10} />
        {tag}
      </span>
    {/each}
  </div>

  <div class="flex items-center justify-between text-[11px] text-slate-500 pt-2 border-t border-border">
    <span>Importance: {record.importance}/10</span>
    <span>{timeAgo(record.created_at)}</span>
  </div>
</div>
