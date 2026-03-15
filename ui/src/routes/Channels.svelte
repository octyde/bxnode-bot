<script lang="ts">
  import { onMount } from "svelte";
  import { MessageSquare, Settings, Play, Square, Loader2, RotateCw, Link, Unlink } from "lucide-svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import ChannelConfigModal from "$lib/components/channels/ChannelConfigModal.svelte";
  import ApprovalPanel from "$lib/components/channels/ApprovalPanel.svelte";
  import { channelsStore } from "$lib/stores/channels.svelte";
  import { serverStore } from "$lib/stores/server.svelte";

  const channelNames: Record<string, string> = {
    telegram: "Telegram",
    discord: "Discord",
    slack: "Slack",
    line: "LINE",
    signal: "Signal",
    feishu: "Feishu",
  };

  const statusVariant = (status: string) => {
    if (status === "running" || status === "connected") return "success" as const;
    if (status === "error") return "error" as const;
    if (status === "starting") return "warning" as const;
    return "muted" as const;
  };

  function getChannelDisplayStatus(channel: { id: string; configured: boolean; status: string }) {
    if (!channel.configured) return "Not configured";
    if (serverStore.running) {
      const live = serverStore.getChannelStatus(channel.id);
      if (live) {
        if (live.error) return "error";
        return live.connected ? "running" : "stopped";
      }
      return "starting";
    }
    return channel.status;
  }

  function getChannelSubtext(channel: { id: string; configured: boolean }) {
    if (!channel.configured) return "Click Configure to set up";
    if (serverStore.running) {
      const live = serverStore.getChannelStatus(channel.id);
      if (live?.connected) return "Connected and receiving messages";
      if (live?.error) return live.error;
      return "Waiting for connection...";
    }
    return "Start the server to go live";
  }

  let configModalOpen = $state(false);
  let selectedChannelId = $state("");
  let selectedChannelName = $state("");
  let selectedChannelConfigured = $state(false);
  let showAttachDialog = $state(false);
  let attachPort = $state("18500");
  let restarting = $state(false);

  function openConfig(channel: { id: string; name: string; configured: boolean }) {
    selectedChannelId = channel.id;
    selectedChannelName = channelNames[channel.id] || channel.name;
    selectedChannelConfigured = channel.configured;
    configModalOpen = true;
  }

  function closeConfig() {
    configModalOpen = false;
  }

  async function handleUpdate() {
    await channelsStore.refresh();
  }

  async function handleRestart() {
    restarting = true;
    try {
      if (serverStore.attached) {
        // Restart external server via RPC, then reconnect
        await serverStore.restartRemote();
      } else {
        await serverStore.restart();
      }
    } finally {
      restarting = false;
    }
  }

  async function handleAttach() {
    const port = parseInt(attachPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) return;
    showAttachDialog = false;
    await serverStore.attach(port);
  }

  onMount(() => {
    channelsStore.refresh();
    serverStore.refresh();
  });
</script>

<div class="p-6 space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">Channels</h1>
      <p class="text-sm text-slate-500">Messaging platform integrations</p>
    </div>

    <div class="flex items-center gap-2">
      {#if serverStore.running}
        <Badge variant="success">
          {serverStore.attached ? "Attached" : "Running"} on :{serverStore.port}
        </Badge>
      {:else if serverStore.state === "starting"}
        <Badge variant="warning">Starting...</Badge>
      {:else}
        <Badge variant="muted">Server stopped</Badge>
      {/if}

      {#if !serverStore.running && serverStore.state !== "starting"}
        <Button variant="primary" size="sm" onclick={() => serverStore.start()}>
          <Play size={14} />
          Start Server
        </Button>
        <Button variant="ghost" size="sm" onclick={() => showAttachDialog = !showAttachDialog}>
          <Link size={14} />
          Attach
        </Button>
      {/if}

      {#if serverStore.running}
        <Button variant="ghost" size="sm" onclick={handleRestart} disabled={restarting || serverStore.state === "stopping"}>
          {#if restarting}
            <Loader2 size={14} class="animate-spin" />
            Restarting...
          {:else}
            <RotateCw size={14} />
            Restart
          {/if}
        </Button>
      {/if}

      {#if serverStore.attached}
        <Button
          variant="ghost"
          size="sm"
          onclick={() => serverStore.detach()}
          disabled={!serverStore.running}
        >
          <Unlink size={14} />
          Detach
        </Button>
      {/if}

      <Button
        variant="danger"
        size="sm"
        onclick={() => serverStore.stop()}
        disabled={!serverStore.running || serverStore.attached || serverStore.state === "stopping" || restarting}
      >
        {#if serverStore.state === "stopping"}
          <Loader2 size={14} class="animate-spin" />
          Stopping...
        {:else}
          <Square size={14} />
          Stop
        {/if}
      </Button>
    </div>
  </div>

  {#if showAttachDialog}
    <div class="flex items-center gap-3 p-4 bg-bg-tertiary rounded-lg border border-border">
      <div class="flex-1">
        <p class="text-sm font-medium mb-2">Attach to existing server</p>
        <p class="text-xs text-slate-500 mb-3">Connect to a BXNode Bot server already running on your machine.</p>
        <div class="flex items-center gap-2">
          <label for="attach-port" class="text-sm text-slate-400 shrink-0">Port:</label>
          <input
            id="attach-port"
            type="text"
            bind:value={attachPort}
            placeholder="18500"
            onkeydown={(e: KeyboardEvent) => e.key === "Enter" && handleAttach()}
            class="w-24 px-2.5 py-1.5 bg-bg-secondary border border-border rounded-lg text-sm text-slate-200
              placeholder:text-slate-500 focus:outline-none focus:border-accent transition-colors"
          />
          <Button variant="primary" size="sm" onclick={handleAttach}>
            Connect
          </Button>
          <Button variant="ghost" size="sm" onclick={() => showAttachDialog = false}>
            Cancel
          </Button>
        </div>
      </div>
    </div>
  {/if}

  {#if serverStore.error}
    <div class="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
      {serverStore.error}
    </div>
  {/if}

  {#if serverStore.running}
    <ApprovalPanel />
  {/if}

  {#if channelsStore.channels.length === 0 && !channelsStore.loading}
    <EmptyState icon={MessageSquare} title="No channels available" description="Channel list could not be loaded" />
  {:else}
    <div class="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
      {#each channelsStore.channels as channel}
        {@const displayStatus = getChannelDisplayStatus(channel)}
        <Card>
          <div class="flex items-center gap-4">
            <div class="w-12 h-12 rounded-xl bg-accent/15 flex items-center justify-center">
              <MessageSquare size={24} class="text-accent" />
            </div>
            <div class="flex-1">
              <div class="flex items-center justify-between">
                <h3 class="font-semibold">{channelNames[channel.id] ?? channel.name}</h3>
                <Badge variant={statusVariant(displayStatus)}>
                  {displayStatus}
                </Badge>
              </div>
              <p class="text-xs text-slate-500 mt-1">
                {getChannelSubtext(channel)}
              </p>
            </div>
            <button
              onclick={() => openConfig(channel)}
              class="p-2 rounded-lg text-slate-400 hover:bg-bg-tertiary hover:text-slate-200 transition-colors"
              title="Configure {channelNames[channel.id] || channel.name}"
            >
              <Settings size={18} />
            </button>
          </div>
        </Card>
      {/each}
    </div>
  {/if}
</div>

<ChannelConfigModal
  bind:open={configModalOpen}
  channelId={selectedChannelId}
  channelName={selectedChannelName}
  configured={selectedChannelConfigured}
  onclose={closeConfig}
  onupdate={handleUpdate}
/>
