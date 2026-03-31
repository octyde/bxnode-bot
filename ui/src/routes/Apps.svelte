<script lang="ts">
  import { ArrowLeft, Download, Trash2, ExternalLink, Package } from "lucide-svelte";
  import SearchBox from "$lib/components/ui/SearchBox.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import { appsStore } from "$lib/stores/apps.svelte";
  import { appStore } from "$lib/stores/app.svelte";
  import { getAppIcon } from "$lib/apps/icons";
  import AppRunner from "$lib/apps/AppRunner.svelte";

  let searchQuery = $state("");

  const filtered = $derived(
    appsStore.availableApps.filter((app) =>
      app.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      app.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (app.category || "").toLowerCase().includes(searchQuery.toLowerCase())
    )
  );

  const activeApp = $derived(
    appsStore.activeAppId
      ? appsStore.availableApps.find(a => a.name === appsStore.activeAppId) ?? null
      : null
  );

  const ActiveIcon = $derived(getAppIcon(activeApp?.icon));

  function handleInstall(appName: string) {
    appsStore.install(appName);
    const app = appsStore.availableApps.find(a => a.name === appName);
    appStore.toast(`${app?.name || appName} installed`);
  }

  function handleUninstall(appName: string) {
    const app = appsStore.availableApps.find(a => a.name === appName);
    if (confirm(`Uninstall ${app?.name || appName}? App data will be deleted.`)) {
      appsStore.uninstall(appName);
      appStore.toast(`${app?.name || appName} uninstalled`);
    }
  }

  function handleOpen(appName: string) {
    appsStore.openApp(appName);
  }

  function handleBack() {
    appsStore.closeApp();
  }

  const categoryColors: Record<string, string> = {
    Productivity: "bg-blue-500/15 text-blue-400",
    Finance: "bg-emerald-500/15 text-emerald-400",
    Social: "bg-purple-500/15 text-purple-400",
    Tools: "bg-amber-500/15 text-amber-400",
    Entertainment: "bg-rose-500/15 text-rose-400",
  };

  function getCategoryColor(category: string): string {
    return categoryColors[category] || "bg-slate-500/15 text-slate-400";
  }
</script>

{#if activeApp}
  <!-- Active App View -->
  <div class="flex flex-col h-full">
    <div class="px-6 py-4 border-b border-border flex items-center gap-4">
      <button
        onclick={handleBack}
        class="p-1.5 rounded-lg text-slate-400 hover:bg-bg-tertiary hover:text-slate-200 transition-colors"
      >
        <ArrowLeft size={18} />
      </button>
      <div class="flex items-center gap-3">
        <div class="w-8 h-8 rounded-lg bg-bg-tertiary flex items-center justify-center">
          <ActiveIcon size={16} class="text-accent" />
        </div>
        <div>
          <h1 class="text-sm font-semibold">{activeApp.name}</h1>
          <p class="text-[11px] text-slate-500">v{activeApp.version || "1.0.0"} by {activeApp.author || "Unknown"}</p>
        </div>
      </div>
    </div>
    <div class="flex-1 overflow-y-auto p-6">
      <AppRunner appName={activeApp.name} />
    </div>
  </div>
{:else}
  <!-- Store Listing -->
  <div class="p-6 space-y-6">
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-lg font-semibold">App Store</h1>
        <p class="text-sm text-slate-500">
          {#if appsStore.loading}
            Loading apps...
          {:else}
            {appsStore.availableApps.length} applications available
          {/if}
        </p>
      </div>
    </div>

    <SearchBox bind:value={searchQuery} placeholder="Search apps..." />

    {#if appsStore.loading}
      <div class="flex items-center justify-center py-16">
        <Spinner size={24} />
      </div>
    {:else if filtered.length === 0}
      <EmptyState icon={Package} title="No apps found" description="Try a different search term" />
    {:else}
      <div class="grid grid-cols-[repeat(auto-fill,minmax(380px,1fr))] gap-4">
        {#each filtered as app (app.name)}
          {@const installed = appsStore.isInstalled(app.name)}
          {@const EntryIcon = getAppIcon(app.icon)}
          <div class="bg-bg-secondary border border-border rounded-xl p-5 space-y-4 hover:border-slate-600 transition-colors">
            <!-- Header -->
            <div class="flex items-start gap-4">
              <div class="w-12 h-12 rounded-xl bg-bg-tertiary flex items-center justify-center shrink-0">
                <EntryIcon size={22} class="text-accent" />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <h3 class="font-medium text-slate-200 truncate">{app.name}</h3>
                  <span class="text-[10px] text-slate-500 shrink-0">v{app.version || "1.0.0"}</span>
                </div>
                <div class="flex items-center gap-2 mt-1">
                  {#if app.category}
                    <span class="px-2 py-0.5 rounded-full text-[10px] font-medium {getCategoryColor(app.category)}">
                      {app.category}
                    </span>
                  {/if}
                  <span class="text-[11px] text-slate-500">by {app.author || "Unknown"}</span>
                </div>
              </div>
            </div>

            <!-- Description -->
            <p class="text-sm text-slate-400 leading-relaxed">{app.description}</p>

            <!-- Actions -->
            <div class="flex items-center gap-2">
              {#if installed}
                <Button onclick={() => handleOpen(app.name)}>
                  <ExternalLink size={14} />
                  Open
                </Button>
                <Button variant="danger" size="sm" onclick={() => handleUninstall(app.name)}>
                  <Trash2 size={14} />
                  Uninstall
                </Button>
              {:else}
                <Button onclick={() => handleInstall(app.name)}>
                  <Download size={14} />
                  Install
                </Button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
