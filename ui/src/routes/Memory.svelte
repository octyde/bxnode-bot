<script lang="ts">
  import { onMount } from "svelte";
  import { Brain, Archive } from "lucide-svelte";
  import SearchBox from "$lib/components/ui/SearchBox.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import MemoryCard from "$lib/components/memory/MemoryCard.svelte";
  import { memoryStore } from "$lib/stores/memory.svelte";
  import { formatBytes } from "$lib/utils/format";
  import type { MemoryRecord } from "$lib/types";

  let searchQuery = $state("");

  async function handleSearch() {
    if (searchQuery.trim()) {
      await memoryStore.search(searchQuery);
    } else {
      await memoryStore.refresh();
    }
  }

  const displayRecords: MemoryRecord[] = $derived(
    searchQuery.trim()
      ? memoryStore.searchResults.map((r) => ({
          id: r.id,
          scope: { agent_id: "default" },
          content: r.content_preview,
          summary: r.summary,
          tags: r.tags,
          importance: r.importance,
          created_at: r.created_at,
          updated_at: r.created_at,
        }))
      : memoryStore.records
  );

  onMount(() => memoryStore.refresh());
</script>

<div class="p-6 space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">Memory Browser</h1>
      <p class="text-sm text-slate-500">Long-term memory storage</p>
    </div>
    <Button variant="secondary" onclick={() => memoryStore.compact()} disabled={memoryStore.loading}>
      <Archive size={16} />
      Compact
    </Button>
  </div>

  <!-- Stats -->
  {#if memoryStore.stats}
    <div class="grid grid-cols-2 gap-4">
      <Card>
        <div class="text-center">
          <p class="text-2xl font-mono font-semibold">{memoryStore.stats.total_records}</p>
          <p class="text-xs text-slate-500">Total Records</p>
        </div>
      </Card>
      <Card>
        <div class="text-center">
          <p class="text-2xl font-mono font-semibold">{formatBytes(memoryStore.stats.total_size_bytes)}</p>
          <p class="text-xs text-slate-500">Storage Size</p>
        </div>
      </Card>
    </div>
  {/if}

  <!-- Search -->
  <form class="flex gap-3" onsubmit={(e) => { e.preventDefault(); handleSearch(); }}>
    <SearchBox bind:value={searchQuery} placeholder="Search memories..." />
    <Button variant="secondary" onclick={handleSearch}>Search</Button>
  </form>

  <!-- Records -->
  {#if displayRecords.length === 0}
    <EmptyState icon={Brain} title="No memories" description="Memories will appear here as the agent stores information" />
  {:else}
    <div class="grid grid-cols-[repeat(auto-fill,minmax(380px,1fr))] gap-4">
      {#each displayRecords as record (record.id)}
        <MemoryCard {record} ondelete={() => memoryStore.remove(record.id)} />
      {/each}
    </div>
  {/if}
</div>
