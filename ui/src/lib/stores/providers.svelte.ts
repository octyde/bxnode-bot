import type { ProviderInfo } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _providers = $state<ProviderInfo[]>([]);
let _loading = $state(false);

export const providersStore = {
  get providers() { return _providers; },
  get loading() { return _loading; },
  get count() { return _providers.filter((p) => p.configured).length; },

  async refresh() {
    _loading = true;
    try {
      _providers = await api.listProviders();
    } catch (e) {
      appStore.toast(`Failed to load providers: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },
};
