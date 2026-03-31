<script lang="ts">
  import {
    LayoutDashboard,
    Wrench,
    Server,
    MessageSquare as MessageSquareIcon,
    Radio,
    Clock,
    Brain,
    MessagesSquare,
    Settings,
    Layers,
    Store,
  } from "lucide-svelte";
  import { appStore } from "$lib/stores/app.svelte";
  import { serverStore } from "$lib/stores/server.svelte";
  import type { Route } from "$lib/types";

  const navItems: { id: Route; label: string; icon: typeof LayoutDashboard }[] = [
    { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
    { id: "skills", label: "Skills", icon: Wrench },
    { id: "providers", label: "Providers", icon: Server },
    { id: "channels", label: "Channels", icon: MessageSquareIcon },
    { id: "activity", label: "Activity", icon: Radio },
    { id: "cron", label: "Cron Jobs", icon: Clock },
    { id: "memory", label: "Memory", icon: Brain },
    { id: "chat", label: "Chat", icon: MessagesSquare },
    { id: "apps", label: "App Store", icon: Store },
    { id: "settings", label: "Settings", icon: Settings },
  ];
</script>

<aside class="w-56 bg-bg-secondary border-r border-border flex flex-col h-full shrink-0">
  <!-- Brand -->
  <div class="flex items-center gap-3 px-4 h-14 border-b border-border">
    <div class="w-7 h-7 rounded-md bg-gradient-to-br from-accent to-emerald-500 flex items-center justify-center shadow-[0_0_20px_rgba(34,197,94,0.15)]">
      <Layers size={14} class="text-bg-primary" />
    </div>
    <span class="font-semibold text-sm">BXNode Bot</span>
  </div>

  <!-- Navigation -->
  <nav class="flex-1 py-3 px-2 space-y-0.5 overflow-y-auto">
    {#each navItems as item}
      <button
        class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-200
          {appStore.route === item.id
            ? 'bg-accent text-bg-primary'
            : 'text-slate-400 hover:bg-bg-tertiary hover:text-slate-200'}"
        onclick={() => appStore.navigate(item.id)}
      >
        <svelte:component this={item.icon} size={16} />
        {item.label}
        {#if item.id === "channels" && serverStore.pendingApprovalCount > 0}
          <span class="ml-auto w-5 h-5 rounded-full bg-amber-500 text-[10px] font-bold flex items-center justify-center text-bg-primary">
            {serverStore.pendingApprovalCount}
          </span>
        {/if}
        {#if item.id === "activity" && serverStore.running}
          <span class="ml-auto w-2 h-2 rounded-full bg-accent animate-pulse"></span>
        {/if}
      </button>
    {/each}
  </nav>

  <!-- Status -->
  <div class="px-4 py-3 border-t border-border">
    <div class="flex items-center gap-2 text-xs text-accent">
      <span class="w-1.5 h-1.5 rounded-full bg-accent animate-pulse"></span>
      Ready
    </div>
  </div>
</aside>
