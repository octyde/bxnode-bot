import type { SkillInfo, SkillDetail, SkillsConfigSummary } from "$lib/types";
import * as api from "$lib/api";
import { appStore } from "./app.svelte";

let _skills = $state<SkillInfo[]>([]);
let _config = $state<SkillsConfigSummary | null>(null);
let _loading = $state(false);

export const skillsStore = {
  get skills() { return _skills; },
  get config() { return _config; },
  get loading() { return _loading; },
  get totalSkills() { return _config?.total_skills ?? 0; },
  get activeSkills() { return _config?.active_skills ?? 0; },

  async init() {
    _loading = true;
    try {
      _config = await api.initSkills();
      await this.refresh();
    } catch (e) {
      appStore.toast(`Failed to initialize skills: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },

  async refresh() {
    try {
      _skills = await api.listSkills();
      _config = await api.getSkillsConfig();
    } catch (e) {
      appStore.toast(`Failed to refresh skills: ${e}`, "error");
    }
  },

  async enable(name: string) {
    try {
      await api.enableSkill(name);
      await this.refresh();
      appStore.toast(`Skill "${name}" enabled`);
    } catch (e) {
      appStore.toast(`Failed to enable skill: ${e}`, "error");
    }
  },

  async disable(name: string) {
    try {
      await api.disableSkill(name);
      await this.refresh();
      appStore.toast(`Skill "${name}" disabled`);
    } catch (e) {
      appStore.toast(`Failed to disable skill: ${e}`, "error");
    }
  },

  async toggle(name: string, active: boolean) {
    if (active) {
      await this.enable(name);
    } else {
      await this.disable(name);
    }
  },

  async sync() {
    _loading = true;
    try {
      const result = await api.syncSkills();
      if (result.errors.length > 0) {
        appStore.toast(`Sync completed with ${result.errors.length} errors`, "warning");
      } else if (result.synced.length > 0) {
        appStore.toast(`Synced ${result.synced.length} new skills (${result.skipped.length} already existed)`);
      } else if (result.skipped.length > 0) {
        appStore.toast(`All ${result.skipped.length} skills already up to date`);
      } else {
        appStore.toast("No skills found to sync", "warning");
      }
      await this.refresh();
    } catch (e) {
      appStore.toast(`Sync failed: ${e}`, "error");
    } finally {
      _loading = false;
    }
  },

  async getDetail(name: string): Promise<SkillDetail | null> {
    try {
      return await api.getSkill(name);
    } catch (e) {
      appStore.toast(`Failed to load skill details: ${e}`, "error");
      return null;
    }
  },
};
