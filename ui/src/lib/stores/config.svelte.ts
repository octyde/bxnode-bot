import type { AppConfig, StatsResponse } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _config = $state<AppConfig | null>(null);
let _appStats = $state<StatsResponse | null>(null);
let _loading = $state(false);

export const configStore = {
  get config() { return _config; },
  get stats() { return _appStats; },
  get loading() { return _loading; },
  get version() { return _appStats?.version ?? "0.1.0"; },

  async refresh() {
    _loading = true;
    try {
      [_config, _appStats] = await Promise.all([
        api.getConfig(),
        api.getStats(),
      ]);
    } catch (e) {
      appStore.toast(`Failed to load config: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },
};
