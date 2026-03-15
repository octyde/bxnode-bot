<script lang="ts">
  import {
    ArrowDownLeft,
    ArrowUpRight,
    AlertTriangle,
    Radio,
    MessageSquare,
    User,
    Bot,
    Wrench,
    Terminal,
    Info,
    ChevronRight,
    RefreshCw,
  } from "lucide-svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import SearchBox from "$lib/components/ui/SearchBox.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import { serverStore, type ActivityMessage } from "$lib/stores/server.svelte";
  import { sessionsStore } from "$lib/stores/sessions.svelte";
  import type { TranscriptEntry } from "$lib/types";
  import { onMount } from "svelte";

  type ViewMode = "sessions" | "live";

  let viewMode = $state<ViewMode>("sessions");
  let search = $state("");
  let transcriptContainer: HTMLDivElement | undefined = $state();
  let liveContainer: HTMLDivElement | undefined = $state();
  let autoScroll = $state(true);

  // Load sessions when server is running and we switch to sessions view
  onMount(() => {
    if (serverStore.running) {
      sessionsStore.loadSessions();
      sessionsStore.loadProjects();
    }
  });

  // Reload sessions when server state changes to running
  $effect(() => {
    if (serverStore.running && viewMode === "sessions") {
      sessionsStore.loadSessions();
    }
  });

  // Auto-scroll transcript
  $effect(() => {
    if (autoScroll && sessionsStore.transcript.length > 0 && transcriptContainer) {
      requestAnimationFrame(() => {
        if (transcriptContainer) {
          transcriptContainer.scrollTop = transcriptContainer.scrollHeight;
        }
      });
    }
  });

  // Auto-scroll live feed
  $effect(() => {
    if (autoScroll && serverStore.messages.length > 0 && liveContainer) {
      requestAnimationFrame(() => {
        if (liveContainer) {
          liveContainer.scrollTop = liveContainer.scrollHeight;
        }
      });
    }
  });

  // Watch for real-time messages that match the selected session
  let prevMsgCount = $state(0);
  $effect(() => {
    const msgs = serverStore.messages;
    if (msgs.length > prevMsgCount && sessionsStore.selectedSessionId) {
      // Check new messages for matching session
      const newMsgs = msgs.slice(prevMsgCount);
      for (const msg of newMsgs) {
        if (msg.sessionId === sessionsStore.selectedSessionId) {
          const entry: TranscriptEntry = {
            timestamp: msg.timestamp,
            type: msg.type === "incoming" ? "user" : msg.type === "outgoing" ? "assistant" : "system",
            content: msg.type === "error" ? msg.error : msg.content,
            user_id: msg.userId,
          };
          sessionsStore.appendLiveEntry(entry);
        }
      }
    }
    prevMsgCount = msgs.length;
  });

  let filteredSessions = $derived(
    search
      ? sessionsStore.sessions.filter(s =>
          (s.title ?? "").toLowerCase().includes(search.toLowerCase()) ||
          s.channel.toLowerCase().includes(search.toLowerCase())
        )
      : sessionsStore.sessions
  );

  function formatRelativeTime(ts: string) {
    try {
      const d = new Date(ts);
      const now = Date.now();
      const diff = now - d.getTime();
      const secs = Math.floor(diff / 1000);
      if (secs < 60) return "just now";
      const mins = Math.floor(secs / 60);
      if (mins < 60) return `${mins}m ago`;
      const hours = Math.floor(mins / 60);
      if (hours < 24) return `${hours}h ago`;
      const days = Math.floor(hours / 24);
      if (days < 7) return `${days}d ago`;
      return d.toLocaleDateString();
    } catch {
      return ts;
    }
  }

  function formatTime(ts: string) {
    try {
      return new Date(ts).toLocaleTimeString();
    } catch {
      return ts;
    }
  }

  function getEntryIcon(type: string) {
    switch (type) {
      case "user": return User;
      case "assistant": return Bot;
      case "tool_use": return Wrench;
      case "tool_result": return Terminal;
      case "system": return Info;
      default: return MessageSquare;
    }
  }

  function getLiveIcon(msg: ActivityMessage) {
    if (msg.type === "incoming") return ArrowDownLeft;
    if (msg.type === "outgoing") return ArrowUpRight;
    return AlertTriangle;
  }

  function getLiveIconColor(msg: ActivityMessage) {
    if (msg.type === "incoming") return "text-blue-400";
    if (msg.type === "outgoing") return "text-accent";
    return "text-red-400";
  }

  function getLiveBgColor(msg: ActivityMessage) {
    if (msg.type === "incoming") return "bg-blue-500/5 border-blue-500/10";
    if (msg.type === "outgoing") return "bg-accent/5 border-accent/10";
    return "bg-red-500/5 border-red-500/10";
  }

  function channelBadgeColor(channel: string): string {
    const colors: Record<string, string> = {
      telegram: "bg-blue-500/20 text-blue-400",
      discord: "bg-indigo-500/20 text-indigo-400",
      slack: "bg-purple-500/20 text-purple-400",
      line: "bg-green-500/20 text-green-400",
      signal: "bg-sky-500/20 text-sky-400",
    };
    return colors[channel.toLowerCase()] ?? "bg-slate-500/20 text-slate-400";
  }
</script>

<div class="h-full flex flex-col">
  <!-- Header -->
  <div class="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
    <div>
      <h1 class="text-lg font-semibold">Activity</h1>
      <p class="text-sm text-slate-500">
        {#if viewMode === "sessions"}
          Session history and transcripts
        {:else}
          Live message feed from all channels
        {/if}
      </p>
    </div>
    <div class="flex items-center gap-3">
      <!-- View mode toggle -->
      <div class="flex bg-bg-tertiary rounded-lg p-0.5">
        <button
          class="px-3 py-1.5 text-xs font-medium rounded-md transition-all {viewMode === 'sessions' ? 'bg-bg-secondary text-slate-200 shadow-sm' : 'text-slate-500 hover:text-slate-300'}"
          onclick={() => { viewMode = "sessions"; }}
        >
          Sessions
        </button>
        <button
          class="px-3 py-1.5 text-xs font-medium rounded-md transition-all {viewMode === 'live' ? 'bg-bg-secondary text-slate-200 shadow-sm' : 'text-slate-500 hover:text-slate-300'}"
          onclick={() => { viewMode = "live"; }}
        >
          Live Feed
        </button>
      </div>
      {#if serverStore.running}
        <Badge variant="success">
          <span class="flex items-center gap-1.5">
            <span class="w-1.5 h-1.5 rounded-full bg-accent animate-pulse"></span>
            Live
          </span>
        </Badge>
      {:else}
        <Badge variant="muted">Offline</Badge>
      {/if}
    </div>
  </div>

  <!-- Content -->
  {#if viewMode === "sessions"}
    <!-- Sessions View: Two-panel layout -->
    <div class="flex-1 flex overflow-hidden">
      <!-- Left panel: Session list -->
      <div class="w-72 border-r border-border flex flex-col shrink-0 bg-bg-secondary/30">
        <div class="p-3 border-b border-border space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-xs font-semibold text-slate-400 uppercase tracking-wider">Sessions</span>
            <button
              class="p-1 rounded hover:bg-bg-tertiary text-slate-500 hover:text-slate-300 transition-colors"
              onclick={() => sessionsStore.loadSessions()}
              title="Refresh sessions"
            >
              <RefreshCw size={14} />
            </button>
          </div>
          <SearchBox bind:value={search} placeholder="Filter sessions..." />
        </div>

        <div class="flex-1 overflow-y-auto">
          {#if sessionsStore.loading}
            <div class="flex items-center justify-center py-12">
              <Spinner size={20} />
            </div>
          {:else if filteredSessions.length === 0}
            <div class="px-4 py-8 text-center">
              <p class="text-sm text-slate-500">
                {#if search}
                  No sessions matching "{search}"
                {:else if !serverStore.running}
                  Start the server to see sessions
                {:else}
                  No sessions yet
                {/if}
              </p>
            </div>
          {:else}
            {#each filteredSessions as session}
              <button
                class="w-full text-left px-3 py-3 border-b border-border/50 transition-colors
                  {sessionsStore.selectedSessionId === session.id
                    ? 'bg-accent/10 border-l-2 border-l-accent'
                    : 'hover:bg-bg-tertiary/50 border-l-2 border-l-transparent'}"
                onclick={() => sessionsStore.selectSession(session.id)}
              >
                <div class="flex items-start gap-2">
                  <div class="flex-1 min-w-0">
                    <p class="text-sm font-medium text-slate-200 truncate">
                      {session.title ?? "Untitled session"}
                    </p>
                    <div class="flex items-center gap-2 mt-1">
                      <span class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded {channelBadgeColor(session.channel)}">
                        {session.channel}
                      </span>
                      <span class="text-[10px] text-slate-600">{session.project}</span>
                    </div>
                    <p class="text-[11px] text-slate-600 mt-1">{formatRelativeTime(session.updatedAt)}</p>
                  </div>
                  <ChevronRight size={14} class="text-slate-600 mt-1 shrink-0 {sessionsStore.selectedSessionId === session.id ? 'text-accent' : ''}" />
                </div>
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <!-- Right panel: Transcript -->
      <div class="flex-1 flex flex-col min-w-0">
        {#if sessionsStore.selectedSession}
          <!-- Session header -->
          <div class="px-5 py-3 border-b border-border shrink-0 bg-bg-secondary/20">
            <div class="flex items-center justify-between">
              <div>
                <h2 class="text-sm font-semibold text-slate-200">
                  {sessionsStore.selectedSession.title ?? "Untitled session"}
                </h2>
                <div class="flex items-center gap-2 mt-0.5">
                  <span class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded {channelBadgeColor(sessionsStore.selectedSession.channel)}">
                    {sessionsStore.selectedSession.channel}
                  </span>
                  <span class="text-xs text-slate-500">
                    Chat: {sessionsStore.selectedSession.chatId}
                  </span>
                  <span class="text-xs text-slate-600">
                    {formatRelativeTime(sessionsStore.selectedSession.updatedAt)}
                  </span>
                </div>
              </div>
              <button
                class="p-1.5 rounded hover:bg-bg-tertiary text-slate-500 hover:text-slate-300 transition-colors"
                onclick={() => sessionsStore.loadTranscript(sessionsStore.selectedSessionId!)}
                title="Reload transcript"
              >
                <RefreshCw size={14} />
              </button>
            </div>
          </div>

          <!-- Transcript entries -->
          <div
            bind:this={transcriptContainer}
            class="flex-1 overflow-y-auto px-5 py-4 space-y-3"
          >
            {#if sessionsStore.transcriptLoading}
              <div class="flex items-center justify-center py-12">
                <Spinner size={20} />
              </div>
            {:else if sessionsStore.transcript.length === 0}
              <div class="flex items-center justify-center py-12">
                <p class="text-sm text-slate-500">No messages in this session yet</p>
              </div>
            {:else}
              {#each sessionsStore.transcript as entry}
                {#if entry.type === "user"}
                  <!-- User message: right-aligned bubble -->
                  <div class="flex justify-end">
                    <div class="max-w-[80%]">
                      <div class="bg-blue-600/20 border border-blue-500/20 rounded-2xl rounded-br-md px-4 py-2.5">
                        <p class="text-sm text-slate-200 whitespace-pre-wrap break-words">{entry.content ?? ""}</p>
                      </div>
                      <div class="flex items-center justify-end gap-2 mt-1 px-1">
                        {#if entry.user_id}
                          <span class="text-[10px] text-slate-600">{entry.user_id}</span>
                        {/if}
                        <span class="text-[10px] text-slate-600">{formatTime(entry.timestamp)}</span>
                      </div>
                    </div>
                  </div>
                {:else if entry.type === "assistant"}
                  <!-- Assistant message: left-aligned bubble -->
                  <div class="flex justify-start">
                    <div class="max-w-[80%]">
                      <div class="bg-bg-tertiary border border-border rounded-2xl rounded-bl-md px-4 py-2.5">
                        <p class="text-sm text-slate-300 whitespace-pre-wrap break-words">{entry.content ?? ""}</p>
                      </div>
                      <div class="flex items-center gap-2 mt-1 px-1">
                        <span class="text-[10px] text-accent">Bot</span>
                        {#if entry.model}
                          <span class="text-[10px] text-slate-600">{entry.model}</span>
                        {/if}
                        <span class="text-[10px] text-slate-600">{formatTime(entry.timestamp)}</span>
                      </div>
                    </div>
                  </div>
                {:else if entry.type === "tool_use"}
                  <!-- Tool use: compact inline -->
                  <div class="flex justify-start">
                    <div class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-amber-500/5 border border-amber-500/10 text-xs">
                      <Wrench size={12} class="text-amber-400 shrink-0" />
                      <span class="text-amber-400 font-medium">{entry.tool}</span>
                      <span class="text-slate-600">{formatTime(entry.timestamp)}</span>
                    </div>
                  </div>
                {:else if entry.type === "tool_result"}
                  <!-- Tool result: compact inline -->
                  <div class="flex justify-start">
                    <div class="max-w-[80%]">
                      <div class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-slate-500/5 border border-slate-500/10 text-xs">
                        <Terminal size={12} class="shrink-0 {entry.success ? 'text-accent' : 'text-red-400'}" />
                        <span class="font-medium {entry.success ? 'text-accent' : 'text-red-400'}">{entry.tool}</span>
                        {#if entry.output}
                          <span class="text-slate-500 truncate">{entry.output.slice(0, 80)}</span>
                        {/if}
                      </div>
                    </div>
                  </div>
                {:else if entry.type === "system"}
                  <!-- System message: centered -->
                  <div class="flex justify-center">
                    <div class="flex items-center gap-2 px-3 py-1 rounded-full bg-bg-tertiary text-xs text-slate-500">
                      <Info size={12} />
                      <span>{entry.message ?? entry.content ?? ""}</span>
                    </div>
                  </div>
                {/if}
              {/each}
            {/if}
          </div>
        {:else}
          <!-- No session selected -->
          <div class="flex-1 flex items-center justify-center">
            <EmptyState
              icon={MessageSquare}
              title="Select a session"
              description="Choose a session from the left panel to view its chat history"
            />
          </div>
        {/if}
      </div>
    </div>

  {:else}
    <!-- Live Feed View (existing behavior) -->
    <div class="flex-1 flex flex-col px-6 py-4">
      {#if !serverStore.running && serverStore.messages.length === 0}
        <div class="flex-1 flex items-center justify-center">
          <EmptyState
            icon={Radio}
            title="No activity yet"
            description="Start the server from the Channels page to see live messages"
          />
        </div>
      {:else if serverStore.messages.length === 0}
        <div class="flex-1 flex items-center justify-center">
          <EmptyState
            icon={Radio}
            title="Waiting for messages"
            description="Messages from connected channels will appear here in real-time"
          />
        </div>
      {:else}
        <div class="flex items-center justify-between mb-3 shrink-0">
          <span class="text-xs text-slate-500">{serverStore.messages.length} messages</span>
          <button
            class="text-xs text-slate-500 hover:text-slate-300 transition-colors"
            onclick={() => serverStore.clearMessages()}
          >
            Clear all
          </button>
        </div>
        <div
          bind:this={liveContainer}
          class="flex-1 overflow-y-auto space-y-1.5 pr-1"
        >
          {#each serverStore.messages as msg}
            <div class="flex items-start gap-3 px-3 py-2.5 rounded-lg border {getLiveBgColor(msg)} transition-colors">
              <div class="mt-0.5 shrink-0">
                <svelte:component this={getLiveIcon(msg)} size={16} class={getLiveIconColor(msg)} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-0.5">
                  <span class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded bg-bg-tertiary text-slate-400">
                    {msg.channel}
                  </span>
                  {#if msg.type === "incoming" && (msg.userName || msg.userId)}
                    <span class="text-xs font-medium text-slate-300">
                      {msg.userName || msg.userId}
                    </span>
                  {:else if msg.type === "outgoing"}
                    <span class="text-xs font-medium text-accent">Bot</span>
                  {:else if msg.type === "error"}
                    <span class="text-xs font-medium text-red-400">Error</span>
                  {/if}
                  <span class="text-[10px] text-slate-600 ml-auto shrink-0">{formatTime(msg.timestamp)}</span>
                </div>
                {#if msg.type === "error" && msg.error}
                  <p class="text-sm text-red-400 break-words">{msg.error}</p>
                {:else}
                  <p class="text-sm text-slate-300 break-words whitespace-pre-wrap">{msg.content}</p>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
