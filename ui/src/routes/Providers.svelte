<script lang="ts">
  import { onMount } from "svelte";
  import { Server, Check, X, Settings } from "lucide-svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import ProviderConfigModal from "$lib/components/ProviderConfigModal.svelte";
  import { providersStore } from "$lib/stores/providers.svelte";
  import * as api from "$lib/api";

  const providerMeta: Record<string, { label: string; description: string }> = {
    anthropic: { label: "Anthropic (Claude)", description: "Claude models with streaming and tool calling" },
    openai: { label: "OpenAI (GPT)", description: "GPT models with streaming and tool calling" },
    ollama: { label: "Ollama (Local)", description: "Local models via Ollama API" },
    zai: { label: "Z.AI (GLM)", description: "Zhipu GLM models via OpenAI-compatible API" },
    groq: { label: "Groq", description: "Fast inference for open models" },
    deepseek: { label: "DeepSeek", description: "DeepSeek Chat and Reasoner models" },
    mistral: { label: "Mistral", description: "Mistral and Codestral models" },
    venice: { label: "Venice.ai", description: "Privacy-focused open model inference" },
    qwen: { label: "Qwen (DashScope)", description: "Alibaba Qwen models via DashScope" },
    gemini: { label: "Google Gemini", description: "Google Gemini models via OpenAI-compatible API" },
  };

  let configModalOpen = $state(false);
  let selectedProvider = $state<any>(null);
  let availableModels = $state<string[]>([]);

  function openConfig(provider: any) {
    selectedProvider = provider;
    configModalOpen = true;
  }

  function closeConfig() {
    configModalOpen = false;
    selectedProvider = null;
  }

  function handleUpdate() {
    providersStore.refresh();
  }

  onMount(async () => {
    providersStore.refresh();
    try {
      availableModels = await api.listModels();
    } catch {
      // Models may not be available if no providers configured yet
    }
  });
</script>

<div class="p-6 space-y-6">
  <div>
    <h1 class="text-lg font-semibold">Providers</h1>
    <p class="text-sm text-slate-500">LLM provider configuration</p>
  </div>

  {#if providersStore.providers.length === 0 && !providersStore.loading}
    <EmptyState icon={Server} title="No providers configured" description="Add provider API keys in config.yaml" />
  {:else}
    <div class="grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-4">
      {#each providersStore.providers as provider}
        <Card>
          <div class="space-y-4">
            <div class="flex items-start justify-between">
              <div class="flex items-center gap-3">
                <div class="w-10 h-10 rounded-lg bg-accent/15 flex items-center justify-center">
                  <Server size={20} class="text-accent" />
                </div>
                <div>
                  <h3 class="font-semibold text-sm">{providerMeta[provider.id]?.label ?? provider.name}</h3>
                  <p class="text-xs text-slate-500">{providerMeta[provider.id]?.description ?? ""}</p>
                </div>
              </div>
              <Badge variant={provider.configured ? "success" : "muted"}>
                {provider.configured ? "Configured" : "Not Set"}
              </Badge>
            </div>

            <div class="space-y-2 text-sm">
              <div class="flex justify-between items-center">
                <span class="text-slate-500">API Key</span>
                <span class="font-mono text-xs flex items-center gap-1">
                  {#if provider.has_api_key}
                    <Check size={12} class="text-accent" /> Set
                  {:else}
                    <X size={12} class="text-slate-500" /> Not set
                  {/if}
                </span>
              </div>
              {#if provider.base_url}
                <div class="flex justify-between items-center">
                  <span class="text-slate-500">Base URL</span>
                  <span class="font-mono text-xs text-slate-400">{provider.base_url}</span>
                </div>
              {/if}
              {#if provider.default_model}
                <div class="flex justify-between items-center">
                  <span class="text-slate-500">Default Model</span>
                  <span class="font-mono text-xs text-slate-400">{provider.default_model}</span>
                </div>
              {/if}
            </div>

            <div class="pt-2">
              <Button variant="secondary" size="sm" onclick={() => openConfig(provider)}>
                <Settings size={14} />
                Configure
              </Button>
            </div>
          </div>
        </Card>
      {/each}
    </div>
  {/if}
</div>

{#if selectedProvider}
  <ProviderConfigModal
    bind:open={configModalOpen}
    providerId={selectedProvider.id}
    providerName={providerMeta[selectedProvider.id]?.label ?? selectedProvider.name}
    currentApiKey={selectedProvider.has_api_key ? "••••••••" : ""}
    currentBaseUrl={selectedProvider.base_url ?? ""}
    supportsOrganization={selectedProvider.id === "openai"}
    currentDefaultModel={selectedProvider.default_model ?? ""}
    {availableModels}
    onclose={closeConfig}
    onupdate={handleUpdate}
  />
{/if}
