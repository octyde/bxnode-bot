<script lang="ts">
  import { Settings, Trash2, Eye, EyeOff, Wifi } from "lucide-svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { updateChannelConfig, removeChannelConfig, getChannelConfig, testChannelConnection } from "$lib/api";
  import { serverStore } from "$lib/stores/server.svelte";

  interface FieldDef {
    key: string;
    label: string;
    required?: boolean;
    secret?: boolean;
    toggle?: boolean;
    placeholder?: string;
  }

  interface Props {
    open?: boolean;
    channelId: string;
    channelName: string;
    configured?: boolean;
    onclose?: () => void;
    onupdate?: () => void;
  }

  let {
    open = $bindable(false),
    channelId,
    channelName,
    configured = false,
    onclose,
    onupdate,
  }: Props = $props();

  let fields = $state<Record<string, string>>({});
  let saving = $state(false);
  let deleting = $state(false);
  let loading = $state(false);
  let testing = $state(false);
  let error = $state("");
  let success = $state("");
  let testResult = $state<{ success: boolean; message: string } | null>(null);
  let showSecrets = $state<Record<string, boolean>>({});

  // Define fields per channel type
  const channelFields: Record<string, FieldDef[]> = {
    telegram: [
      { key: "token", label: "Bot Token", required: true, secret: true, placeholder: "123456:ABC-DEF..." },
      { key: "allowed_users", label: "Allowed User IDs", placeholder: "Comma-separated IDs (empty = allow all)" },
      { key: "approval_required", label: "Require Approval for Unknown Users", toggle: true },
    ],
    discord: [
      { key: "token", label: "Bot Token", required: true, secret: true, placeholder: "Your Discord bot token" },
      { key: "allowed_guilds", label: "Allowed Guild IDs", placeholder: "Comma-separated IDs (empty = allow all)" },
    ],
    slack: [
      { key: "bot_token", label: "Bot Token", required: true, secret: true, placeholder: "xoxb-YOUR-BOT-TOKEN" },
      { key: "app_token", label: "App Token", required: true, secret: true, placeholder: "xapp-YOUR-APP-TOKEN" },
    ],
    line: [
      { key: "channel_access_token", label: "Channel Access Token", required: true, secret: true, placeholder: "Your LINE channel access token" },
      { key: "channel_secret", label: "Channel Secret", required: true, secret: true, placeholder: "Your LINE channel secret" },
      { key: "allowed_users", label: "Allowed User IDs", placeholder: "Comma-separated IDs (empty = allow all)" },
    ],
    signal: [
      { key: "phone_number", label: "Phone Number", required: true, placeholder: "+1234567890" },
      { key: "api_url", label: "API URL", placeholder: "http://localhost:8080 (default)" },
      { key: "allowed_numbers", label: "Allowed Numbers", placeholder: "Comma-separated numbers (empty = allow all)" },
    ],
    feishu: [
      { key: "app_id", label: "App ID", required: true, placeholder: "Your Feishu App ID" },
      { key: "app_secret", label: "App Secret", required: true, secret: true, placeholder: "Your Feishu App Secret" },
      { key: "verification_token", label: "Verification Token", required: true, secret: true, placeholder: "Your verification token" },
      { key: "allowed_users", label: "Allowed User IDs", placeholder: "Comma-separated IDs (empty = allow all)" },
    ],
  };

  const currentFields = $derived(channelFields[channelId] || []);

  $effect(() => {
    if (open) {
      error = "";
      success = "";
      testResult = null;
      // Initialize all fields to empty strings to avoid undefined values
      const initialFields: Record<string, string> = {};
      const initialSecrets: Record<string, boolean> = {};
      for (const f of currentFields) {
        initialFields[f.key] = "";
        if (f.secret) initialSecrets[f.key] = true;
      }
      fields = initialFields;
      showSecrets = initialSecrets;
      // Load existing config
      if (configured) {
        loadConfig();
      }
    }
  });

  async function loadConfig() {
    loading = true;
    try {
      const config = await getChannelConfig(channelId);
      // Build the full object before assigning to avoid partial state
      const loaded: Record<string, string> = {};
      for (const f of currentFields) {
        loaded[f.key] = String((config as Record<string, string>)[f.key] ?? "");
      }
      fields = loaded;
    } catch (e) {
      // Ignore errors - just show empty fields
    } finally {
      loading = false;
    }
  }

  async function handleSave() {
    error = "";
    success = "";

    // Validate required fields
    for (const f of currentFields) {
      if (f.required && !(fields[f.key] || "").trim()) {
        error = `${f.label} is required`;
        return;
      }
    }

    saving = true;
    try {
      // Build a plain object with string values only — no Svelte proxy, no undefined
      const cleanFields: Record<string, string> = {};
      for (const f of currentFields) {
        cleanFields[f.key] = String(fields[f.key] ?? "");
      }
      // Deep-clone to strip any remaining proxy wrappers before sending to Tauri IPC
      const safeFields = JSON.parse(JSON.stringify(cleanFields)) as Record<string, string>;
      await updateChannelConfig(channelId, safeFields);
      success = `${channelName} configuration saved`;
      onupdate?.();
      setTimeout(() => {
        onclose?.();
      }, 1000);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (!confirm(`Remove ${channelName} configuration?`)) return;

    error = "";
    success = "";
    deleting = true;
    try {
      await removeChannelConfig(channelId);
      success = `${channelName} configuration removed`;
      onupdate?.();
      setTimeout(() => {
        onclose?.();
      }, 1000);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      deleting = false;
    }
  }

  async function handleTest() {
    testResult = null;
    error = "";
    success = "";
    testing = true;
    try {
      const cleanFields: Record<string, string> = {};
      for (const f of currentFields) {
        cleanFields[f.key] = String(fields[f.key] ?? "");
      }
      const safeFields = JSON.parse(JSON.stringify(cleanFields)) as Record<string, string>;
      const result = await testChannelConnection(channelId, safeFields);
      testResult = result;
    } catch (e) {
      testResult = { success: false, message: e instanceof Error ? e.message : String(e) };
    } finally {
      testing = false;
    }
  }

  function handleClose() {
    if (!saving && !deleting && !testing) {
      onclose?.();
    }
  }
</script>

<Modal open={open} title="Configure {channelName}" onclose={handleClose}>
  <div class="space-y-6">
    <div class="flex items-center gap-3 p-4 bg-bg-tertiary rounded-lg border border-border">
      <div class="w-10 h-10 rounded-lg bg-accent/15 flex items-center justify-center">
        <Settings size={20} class="text-accent" />
      </div>
      <div class="flex-1">
        <p class="font-semibold text-sm">{channelName}</p>
        <p class="text-xs text-slate-500">Channel ID: {channelId}</p>
      </div>
      {#if serverStore.running && configured}
        {@const liveStatus = serverStore.getChannelStatus(channelId)}
        {#if liveStatus?.connected}
          <span class="flex items-center gap-1.5 text-xs text-accent">
            <span class="w-2 h-2 rounded-full bg-accent animate-pulse"></span>
            Live
          </span>
        {:else if liveStatus?.error}
          <span class="flex items-center gap-1.5 text-xs text-red-400">
            <span class="w-2 h-2 rounded-full bg-red-400"></span>
            Error
          </span>
        {:else}
          <span class="flex items-center gap-1.5 text-xs text-amber-400">
            <span class="w-2 h-2 rounded-full bg-amber-400 animate-pulse"></span>
            Connecting
          </span>
        {/if}
      {/if}
    </div>

    {#if loading}
      <div class="text-center text-slate-500 py-4">Loading configuration...</div>
    {:else}
      <div class="space-y-4">
        {#each currentFields as fieldDef}
          <div>
            <label for="field-{fieldDef.key}" class="block text-sm font-medium mb-2">
              {fieldDef.label}
              {#if fieldDef.required}
                <span class="text-red-400">*</span>
              {:else}
                <span class="text-slate-500 text-xs font-normal">(optional)</span>
              {/if}
            </label>
            {#if fieldDef.toggle}
              <button
                type="button"
                role="switch"
                aria-checked={fields[fieldDef.key] === "true"}
                aria-label={fieldDef.label}
                onclick={() => fields[fieldDef.key] = fields[fieldDef.key] === "true" ? "false" : "true"}
                class="relative w-11 h-6 rounded-full transition-colors duration-200
                  {fields[fieldDef.key] === 'true' ? 'bg-accent' : 'bg-bg-tertiary border border-border'}"
              >
                <span class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform duration-200
                  {fields[fieldDef.key] === 'true' ? 'translate-x-5' : 'translate-x-0'}"></span>
              </button>
            {:else if fieldDef.secret}
              <div class="relative">
                <input
                  id="field-{fieldDef.key}"
                  type={showSecrets[fieldDef.key] ? "text" : "password"}
                  bind:value={fields[fieldDef.key]}
                  placeholder={fieldDef.placeholder || ""}
                  class="w-full px-3 py-2.5 pr-10 bg-bg-secondary border border-border rounded-lg text-slate-200 text-sm
                    placeholder:text-slate-500 focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
                    transition-all duration-200"
                />
                <button
                  type="button"
                  onclick={() => showSecrets[fieldDef.key] = !showSecrets[fieldDef.key]}
                  class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-200 transition-colors"
                >
                  {#if showSecrets[fieldDef.key]}
                    <EyeOff size={18} />
                  {:else}
                    <Eye size={18} />
                  {/if}
                </button>
              </div>
            {:else}
              <input
                id="field-{fieldDef.key}"
                type="text"
                bind:value={fields[fieldDef.key]}
                placeholder={fieldDef.placeholder || ""}
                class="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-lg text-slate-200 text-sm
                  placeholder:text-slate-500 focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
                  transition-all duration-200"
              />
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <button
      type="button"
      onclick={handleTest}
      disabled={testing || saving || deleting || loading}
      class="flex items-center gap-2 px-3 py-2 text-sm rounded-lg border transition-all duration-200
        {testing
          ? 'border-blue-500/30 bg-blue-500/10 text-blue-400 cursor-wait'
          : 'border-border text-slate-400 hover:border-accent hover:text-accent hover:bg-accent/5 cursor-pointer'}
        disabled:opacity-40 disabled:cursor-not-allowed"
    >
      <Wifi size={16} class={testing ? 'animate-pulse' : ''} />
      {testing ? "Testing..." : "Test Connection"}
    </button>

    {#if testResult}
      <div class="p-3 rounded-lg text-sm {testResult.success
        ? 'bg-accent/10 border border-accent/30 text-accent'
        : 'bg-red-500/10 border border-red-500/30 text-red-400'}">
        {testResult.message}
      </div>
    {/if}

    {#if error}
      <div class="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
        {error}
      </div>
    {/if}

    {#if success}
      <div class="p-3 bg-accent/10 border border-accent/30 rounded-lg text-accent text-sm">
        {success}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    {#if configured}
      <Button
        variant="ghost"
        onclick={handleDelete}
        disabled={deleting || saving || testing}
      >
        <Trash2 size={16} />
        {deleting ? "Removing..." : "Remove"}
      </Button>
    {/if}
    <div class="flex-1"></div>
    <Button variant="ghost" onclick={handleClose} disabled={saving || deleting || testing}>
      Cancel
    </Button>
    <Button onclick={handleSave} disabled={saving || deleting || loading || testing}>
      {saving ? "Saving..." : "Save"}
    </Button>
  {/snippet}
</Modal>
