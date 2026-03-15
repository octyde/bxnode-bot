import type { CronJob } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _jobs = $state<CronJob[]>([]);
let _loading = $state(false);

export const cronStore = {
  get jobs() { return _jobs; },
  get loading() { return _loading; },
  get count() { return _jobs.length; },

  async refresh() {
    _loading = true;
    try {
      _jobs = await api.listCronJobs();
    } catch (e) {
      appStore.toast(`Failed to load cron jobs: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },

  async add(id: string, schedule: string, payload: unknown, description?: string) {
    try {
      await api.addCronJob(id, schedule, payload, description);
      await this.refresh();
      appStore.toast(`Job "${id}" added`);
    } catch (e) {
      appStore.toast(`Failed to add job: ${e}`, "error");
    }
  },

  async remove(id: string) {
    try {
      await api.removeCronJob(id);
      await this.refresh();
      appStore.toast(`Job "${id}" removed`);
    } catch (e) {
      appStore.toast(`Failed to remove job: ${e}`, "error");
    }
  },

  async run(id: string) {
    try {
      await api.runCronJob(id);
      appStore.toast(`Job "${id}" triggered`);
    } catch (e) {
      appStore.toast(`Failed to run job: ${e}`, "error");
    }
  },
};
