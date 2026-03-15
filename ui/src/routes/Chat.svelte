<script lang="ts">
  import { onMount } from "svelte";
  import { MessagesSquare, Trash2, Plus } from "lucide-svelte";
  import MessageBubble from "$lib/components/chat/MessageBubble.svelte";
  import MessageInput from "$lib/components/chat/MessageInput.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import type { ChatMessage } from "$lib/types";
  import { appStore } from "$lib/stores/app.svelte";
  import { chatStore } from "$lib/stores/chat.svelte";
  import * as api from "$lib/api";

  let sending = $state(false);
  let messagesEnd: HTMLDivElement;
  let models = $state<string[]>([]);
  let loadingModels = $state(true);

  onMount(async () => {
    // Load saved chat sessions
    chatStore.loadFromLocalStorage();

    // Scroll to bottom if there are restored messages
    if (chatStore.messages.length > 0) {
      setTimeout(() => messagesEnd?.scrollIntoView({ behavior: "smooth" }), 50);
    }

    // Load available models and provider default models
    try {
      const [modelList, providers] = await Promise.all([
        api.listModels(),
        api.listProviders(),
      ]);
      models = modelList;
      if (models.length > 0 && !chatStore.selectedModel) {
        // Prefer a configured provider's default model
        const defaultModel = providers
          .filter(p => p.configured && p.default_model)
          .map(p => p.default_model!)[0];
        chatStore.selectedModel = (defaultModel && models.includes(defaultModel))
          ? defaultModel
          : models[0];
      }
    } catch (e) {
      appStore.toast("Failed to load models", "error");
    } finally {
      loadingModels = false;
    }
  });

  function handleNewSession() {
    chatStore.createSession();
    appStore.toast("New chat session created", "success");
  }

  function handleSwitchSession(e: Event) {
    const target = e.target as HTMLSelectElement;
    chatStore.switchSession(target.value);
    // Scroll to bottom to show latest messages in restored session
    setTimeout(() => messagesEnd?.scrollIntoView({ behavior: "smooth" }), 50);
  }

  function handleClearChat() {
    if (confirm("Clear all messages in this session?")) {
      chatStore.clearMessages();
    }
  }

  function handleDeleteSession() {
    if (chatStore.sessions.length <= 1) {
      appStore.toast("Cannot delete the last session", "error");
      return;
    }
    if (confirm("Delete this session? This cannot be undone.")) {
      if (chatStore.currentSessionId) {
        chatStore.deleteSession(chatStore.currentSessionId);
        appStore.toast("Session deleted", "success");
      }
    }
  }

  async function handleSend(content: string) {
    if (!chatStore.selectedModel) {
      appStore.toast("Please select a model first", "error");
      return;
    }

    const userMsg: ChatMessage = {
      role: "user",
      content,
      timestamp: Date.now(),
    };
    chatStore.addMessage(userMsg);
    sending = true;

    // Scroll to bottom
    setTimeout(() => messagesEnd?.scrollIntoView({ behavior: "smooth" }), 50);

    try {
      const response = await api.sendMessage(content, chatStore.selectedModel);
      const assistantMsg: ChatMessage = {
        role: "assistant",
        content: response,
        timestamp: Date.now(),
      };
      chatStore.addMessage(assistantMsg);
    } catch (e) {
      const errorMsg = String(e);

      // Parse user-friendly error messages
      let displayError = errorMsg;
      if (errorMsg.includes("Insufficient balance")) {
        displayError = "API Error: Insufficient credits. Please recharge your account.";
      } else if (errorMsg.includes("API error:")) {
        // Extract just the API error part
        const match = errorMsg.match(/API error: (.+)/);
        if (match) displayError = `API Error: ${match[1]}`;
      }

      appStore.toast(displayError, "error");

      // Also add error message to chat for context
      const errorChatMsg: ChatMessage = {
        role: "assistant",
        content: `⚠️ Error: ${displayError}`,
        timestamp: Date.now(),
      };
      chatStore.addMessage(errorChatMsg);
    } finally {
      sending = false;
      setTimeout(() => messagesEnd?.scrollIntoView({ behavior: "smooth" }), 50);
    }
  }
</script>

<div class="flex flex-col h-full">
  <div class="px-6 py-4 border-b border-border space-y-3">
    <div class="flex items-center justify-between">
      <h1 class="text-lg font-semibold">Chat Playground</h1>
      <div class="flex items-center gap-2">
        {#if chatStore.messages.length > 0}
          <button onclick={handleClearChat} class="p-1.5 rounded-lg text-slate-400 hover:bg-bg-tertiary hover:text-slate-200 transition-colors" title="Clear messages">
            <Trash2 size={18} />
          </button>
        {/if}
        {#if !loadingModels}
          <select
            bind:value={chatStore.selectedModel}
            style="color: rgb(226, 232, 240); background-color: #1E293B;"
            class="px-3 py-1.5 border border-border rounded-lg text-sm
              focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
              transition-all duration-200 cursor-pointer
              [&>option]:text-slate-200 [&>option]:bg-slate-800"
            disabled={sending}
          >
            {#if models.length === 0}
              <option value="">No models available</option>
            {:else}
              {#each models as model}
                <option value={model}>{model}</option>
              {/each}
            {/if}
          </select>
        {/if}
      </div>
    </div>

    <!-- Session Selector -->
    <div class="flex items-center gap-2">
      <select
        value={chatStore.currentSessionId || ""}
        onchange={handleSwitchSession}
        style="color: rgb(226, 232, 240); background-color: #1E293B;"
        class="flex-1 px-3 py-1.5 border border-border rounded-lg text-sm
          focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
          transition-all duration-200 cursor-pointer
          [&>option]:text-slate-200 [&>option]:bg-slate-800"
        disabled={sending}
      >
        {#each chatStore.sessions as session}
          <option value={session.id}>{session.name}</option>
        {/each}
      </select>
      <button
        onclick={handleNewSession}
        class="px-3 py-1.5 bg-accent text-bg-primary rounded-lg text-sm font-medium
          hover:bg-accent-hover hover:shadow-[0_0_20px_rgba(34,197,94,0.15)]
          transition-all duration-200 flex items-center gap-1.5"
        title="New session"
      >
        <Plus size={16} />
        New
      </button>
      {#if chatStore.sessions.length > 1}
        <button
          onclick={handleDeleteSession}
          class="p-1.5 rounded-lg text-slate-400 hover:bg-red-500/10 hover:text-red-400 transition-colors"
          title="Delete session"
        >
          <Trash2 size={18} />
        </button>
      {/if}
    </div>

    <p class="text-sm text-slate-500">Test the agent with direct messages • {chatStore.messages.length} messages</p>
  </div>

  <div class="flex-1 overflow-y-auto p-6 space-y-4">
    {#if chatStore.messages.length === 0}
      <EmptyState icon={MessagesSquare} title="Start a conversation" description="Send a message below to test the agent" />
    {:else}
      {#each chatStore.messages as message (message.timestamp)}
        <MessageBubble {message} />
      {/each}
      {#if sending}
        <div class="flex gap-3">
          <div class="w-8 h-8 rounded-lg bg-accent/15 flex items-center justify-center shrink-0">
            <span class="inline-block w-4 h-4 border-2 border-bg-tertiary border-t-accent rounded-full animate-spin"></span>
          </div>
          <div class="px-4 py-3 bg-bg-tertiary border border-border rounded-xl text-sm text-slate-500">Thinking...</div>
        </div>
      {/if}
    {/if}
    <div bind:this={messagesEnd}></div>
  </div>

  <MessageInput onsend={handleSend} disabled={sending} />
</div>
