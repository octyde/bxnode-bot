<script lang="ts">
  import { onMount, type Component } from "svelte";
  import Sidebar from "$lib/components/layout/Sidebar.svelte";
  import Toast from "$lib/components/ui/Toast.svelte";
  import { appStore } from "$lib/stores/app.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import { configStore } from "$lib/stores/config.svelte";
  import { serverStore } from "$lib/stores/server.svelte";
  import { appsStore } from "$lib/stores/apps.svelte";

  import Dashboard from "./routes/Dashboard.svelte";
  import Skills from "./routes/Skills.svelte";
  import Providers from "./routes/Providers.svelte";
  import Channels from "./routes/Channels.svelte";
  import Activity from "./routes/Activity.svelte";
  import Cron from "./routes/Cron.svelte";
  import Memory from "./routes/Memory.svelte";
  import Chat from "./routes/Chat.svelte";
  import Apps from "./routes/Apps.svelte";
  import Settings from "./routes/Settings.svelte";

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const routes: Record<string, Component<any>> = {
    dashboard: Dashboard,
    skills: Skills,
    providers: Providers,
    channels: Channels,
    activity: Activity,
    cron: Cron,
    memory: Memory,
    chat: Chat,
    apps: Apps,
    settings: Settings,
  };

  let CurrentRoute = $derived(routes[appStore.route] || Dashboard);

  onMount(async () => {
    try {
      const { loadConfig } = await import("$lib/api");
      // Load config.yaml from the project root
      await loadConfig("config.yaml");
    } catch (e) {
      console.error("Failed to load config:", e);
      // Continue even if config loading fails
    }
    appsStore.loadFromLocalStorage();
    await Promise.all([
      skillsStore.init(),
      configStore.refresh(),
      appsStore.init(),
    ]);
    // Auto-detect an externally running server (e.g., started from CLI)
    await serverStore.refresh();
    appStore.initialized = true;
  });
</script>

<div class="h-screen flex bg-bg-primary text-slate-200 overflow-hidden">
  <Sidebar />
  <main class="flex-1 overflow-y-auto">
    <CurrentRoute />
  </main>
  <Toast />
</div>
