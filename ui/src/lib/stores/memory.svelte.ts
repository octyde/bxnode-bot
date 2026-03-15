import type { MemoryRecord, MemorySearchResult, MemoryStats } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _records = $state<MemoryRecord[]>([]);
let _searchResults = $state<MemorySearchResult[]>([]);
let _stats = $state<MemoryStats | null>(null);
let _loading = $state(false);

export const memoryStore = {
  get records() { return _records; },
  get searchResults() { return _searchResults; },
  get stats() { return _stats; },
  get loading() { return _loading; },

  async refresh() {
    _loading = true;
    try {
      _records = await api.listMemories();
      _stats = await api.memoryStats();
    } catch (e) {
      appStore.toast(`Failed to load memories: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },

  async search(query: string) {
    _loading = true;
    try {
      _searchResults = await api.searchMemories(query);
    } catch (e) {
      appStore.toast(`Search failed: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },

  async remove(id: string) {
    try {
      await api.deleteMemory(id);
      await this.refresh();
      appStore.toast("Memory deleted");
    } catch (e) {
      appStore.toast(`Failed to delete memory: ${e}`, "error");
    }
  },

  async compact() {
    _loading = true;
    try {
      await api.compactMemory();
      await this.refresh();
      appStore.toast("Memory compacted");
    } catch (e) {
      appStore.toast(`Compaction failed: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },
};
