<script lang="ts">
  import { Settings, Trash2, Eye, EyeOff } from "lucide-svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Input from "$lib/components/ui/Input.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { updateProviderConfig, removeProviderConfig } from "$lib/api";

  interface Props {
    open?: boolean;
    providerId: string;
    providerName: string;
    currentApiKey?: string;
    currentBaseUrl?: string;
    supportsOrganization?: boolean;
    currentOrganization?: string;
    currentDefaultModel?: string;
    availableModels?: string[];
    onclose?: () => void;
    onupdate?: () => void;
  }

  let {
    open = $bindable(false),
    providerId,
    providerName,
    currentApiKey = "",
    currentBaseUrl = "",
    supportsOrganization = false,
    currentOrganization = "",
    currentDefaultModel = "",
    availableModels = [],
    onclose,
    onupdate,
  }: Props = $props();

  let apiKey = $state("");
  let baseUrl = $state("");
  let organization = $state("");
  let defaultModel = $state("");
  let saving = $state(false);
  let deleting = $state(false);
  let error = $state("");
  let success = $state("");
  let showApiKey = $state(true);

  $effect(() => {
    if (open) {
      apiKey = currentApiKey;
      baseUrl = currentBaseUrl;
      organization = currentOrganization;
      defaultModel = currentDefaultModel;
      error = "";
      success = "";
      showApiKey = true;
    }
  });

  async function handleSave() {
    error = "";
    success = "";

    if (!apiKey.trim()) {
      error = "API key is required";
      return;
    }

    saving = true;
    try {
      await updateProviderConfig(
        providerId,
        apiKey.trim(),
        baseUrl.trim() || undefined,
        supportsOrganization ? (organization.trim() || undefined) : undefined,
        defaultModel.trim() || undefined
      );
      success = "Provider configuration updated successfully";
      onupdate?.();
      setTimeout(() => {
        onclose?.();
      }, 1000);
    } catch (e) {
      error = e instanceof Error ? e.message : "Failed to update configuration";
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (!confirm(`Remove ${providerName} configuration?`)) return;

    error = "";
    success = "";
    deleting = true;
    try {
      await removeProviderConfig(providerId);
      success = "Provider configuration removed";
      onupdate?.();
      setTimeout(() => {
        onclose?.();
      }, 1000);
    } catch (e) {
      error = e instanceof Error ? e.message : "Failed to remove configuration";
    } finally {
      deleting = false;
    }
  }

  function handleClose() {
    if (!saving && !deleting) {
      onclose?.();
    }
  }
</script>

<Modal open={open} title="Configure {providerName}" onclose={handleClose}>
  <div class="space-y-6">
    <div class="flex items-center gap-3 p-4 bg-bg-tertiary rounded-lg border border-border">
      <div class="w-10 h-10 rounded-lg bg-accent/15 flex items-center justify-center">
        <Settings size={20} class="text-accent" />
      </div>
      <div>
        <p class="font-semibold text-sm">{providerName}</p>
        <p class="text-xs text-slate-500">Provider ID: {providerId}</p>
      </div>
    </div>

    <div class="space-y-4">
      <div>
        <label for="api-key" class="block text-sm font-medium mb-2">
          API Key <span class="text-red-400">*</span>
        </label>
        <div class="relative">
          <input
            id="api-key"
            type={showApiKey ? "text" : "password"}
            bind:value={apiKey}
            placeholder="Enter your API key"
            class="w-full px-3 py-2.5 pr-10 bg-bg-secondary border border-border rounded-lg text-slate-200 text-sm
              placeholder:text-slate-500 focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
              transition-all duration-200"
          />
          <button
            type="button"
            onclick={() => showApiKey = !showApiKey}
            class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-200 transition-colors"
          >
            {#if showApiKey}
              <EyeOff size={18} />
            {:else}
              <Eye size={18} />
            {/if}
          </button>
        </div>
      </div>

      <div>
        <label for="base-url" class="block text-sm font-medium mb-2">
          Base URL <span class="text-slate-500 text-xs font-normal">(optional)</span>
        </label>
        <Input
          id="base-url"
          type="text"
          bind:value={baseUrl}
          placeholder="Leave empty for default"
        />
      </div>

      {#if supportsOrganization}
        <div>
          <label for="organization" class="block text-sm font-medium mb-2">
            Organization <span class="text-slate-500 text-xs font-normal">(optional)</span>
          </label>
          <Input
            id="organization"
            type="text"
            bind:value={organization}
            placeholder="Organization ID"
          />
        </div>
      {/if}

      <div>
        <label for="default-model" class="block text-sm font-medium mb-2">
          Default Model <span class="text-slate-500 text-xs font-normal">(optional)</span>
        </label>
        {#if availableModels.length > 0}
          <select
            id="default-model"
            bind:value={defaultModel}
            style="color: rgb(226, 232, 240); background-color: #1E293B;"
            class="w-full px-3 py-2.5 border border-border rounded-lg text-sm
              focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
              transition-all duration-200 cursor-pointer
              [&>option]:text-slate-200 [&>option]:bg-slate-800"
          >
            <option value="">None (use first available)</option>
            {#each availableModels as model}
              <option value={model}>{model}</option>
            {/each}
          </select>
        {:else}
          <Input
            id="default-model"
            type="text"
            bind:value={defaultModel}
            placeholder="e.g. gpt-4o, claude-3.5-sonnet"
          />
        {/if}
      </div>
    </div>

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
    {#if currentApiKey}
      <Button
        variant="ghost"
        onclick={handleDelete}
        disabled={deleting || saving}
      >
        <Trash2 size={16} />
        {deleting ? "Removing..." : "Remove"}
      </Button>
    {/if}
    <div class="flex-1"></div>
    <Button variant="ghost" onclick={handleClose} disabled={saving || deleting}>
      Cancel
    </Button>
    <Button onclick={handleSave} disabled={saving || deleting}>
      {saving ? "Saving..." : "Save"}
    </Button>
  {/snippet}
</Modal>
