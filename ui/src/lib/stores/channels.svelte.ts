import type { ChannelInfo } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _channels = $state<ChannelInfo[]>([]);
let _loading = $state(false);

export const channelsStore = {
  get channels() { return _channels; },
  get loading() { return _loading; },
  get configuredCount() { return _channels.filter((c) => c.configured).length; },

  async refresh() {
    _loading = true;
    try {
      _channels = await api.listChannels();
    } catch (e) {
      appStore.toast(`Failed to load channels: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },
};
