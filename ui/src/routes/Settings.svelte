<script lang="ts">
  import { onMount } from "svelte";
  import { Settings as SettingsIcon } from "lucide-svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import { configStore } from "$lib/stores/config.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";

  onMount(() => configStore.refresh());
</script>

<div class="p-6 space-y-6">
  <div>
    <h1 class="text-lg font-semibold">Settings</h1>
    <p class="text-sm text-slate-500">Application configuration (read-only)</p>
  </div>

  <!-- Server -->
  <Card>
    <h3 class="font-semibold text-sm mb-4">Server</h3>
    <div class="space-y-3">
      <div class="flex justify-between items-center text-sm">
        <span class="text-slate-500">Host</span>
        <span class="font-mono text-xs">{configStore.config?.server.host ?? "—"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Port</span>
        <span class="font-mono text-xs">{configStore.config?.server.port ?? "—"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">CORS</span>
        <span class="font-mono text-xs">{configStore.config?.server.cors ? "Enabled" : "Disabled"}</span>
      </div>
    </div>
  </Card>

  <!-- Skills -->
  <Card>
    <h3 class="font-semibold text-sm mb-4">Skills</h3>
    <div class="space-y-3">
      <div class="flex justify-between items-center text-sm">
        <span class="text-slate-500">System</span>
        <span class="font-mono text-xs">{configStore.config?.skills.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Total Skills</span>
        <span class="font-mono text-xs">{skillsStore.totalSkills}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Active Skills</span>
        <span class="font-mono text-xs">{skillsStore.activeSkills}</span>
      </div>
    </div>
    {#if skillsStore.config?.directories && skillsStore.config.directories.length > 0}
      <div class="mt-4">
        <h4 class="text-xs text-slate-500 mb-2">Directories</h4>
        <ul class="space-y-2">
          {#each skillsStore.config.directories as dir}
            <li class="px-3 py-2 bg-bg-primary border border-border rounded-lg text-xs font-mono text-slate-400">{dir}</li>
          {/each}
        </ul>
      </div>
    {/if}
  </Card>

  <!-- Memory -->
  <Card>
    <h3 class="font-semibold text-sm mb-4">Memory</h3>
    <div class="space-y-3">
      <div class="flex justify-between items-center text-sm">
        <span class="text-slate-500">System</span>
        <span class="font-mono text-xs">{configStore.config?.memory.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Store Path</span>
        <span class="font-mono text-xs text-slate-400">{configStore.config?.memory.store_path ?? "—"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Max Results</span>
        <span class="font-mono text-xs">{configStore.config?.memory.max_results ?? "—"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">TTL (days)</span>
        <span class="font-mono text-xs">{configStore.config?.memory.ttl_days ?? "—"}</span>
      </div>
    </div>
  </Card>

  <!-- Cron -->
  <Card>
    <h3 class="font-semibold text-sm mb-4">Cron Scheduler</h3>
    <div class="space-y-3">
      <div class="flex justify-between items-center text-sm">
        <span class="text-slate-500">System</span>
        <span class="font-mono text-xs">{configStore.config?.cron.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Store Path</span>
        <span class="font-mono text-xs text-slate-400">{configStore.config?.cron.store_path ?? "—"}</span>
      </div>
    </div>
  </Card>

  <!-- About -->
  <Card>
    <h3 class="font-semibold text-sm mb-4">About</h3>
    <div class="space-y-3">
      <div class="flex justify-between items-center text-sm">
        <span class="text-slate-500">Version</span>
        <span class="font-mono text-xs">{configStore.version}</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">License</span>
        <span class="font-mono text-xs">MIT</span>
      </div>
      <div class="flex justify-between items-center text-sm border-t border-border pt-3">
        <span class="text-slate-500">Maintained by</span>
        <span class="text-xs">Octyde</span>
      </div>
    </div>
  </Card>
</div>
