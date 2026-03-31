<script lang="ts">
  import { onMount } from "svelte";
  import { RefreshCw, Settings2, X, ChevronDown, ChevronRight, Check } from "lucide-svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import Spinner from "$lib/components/ui/Spinner.svelte";
  import * as api from "$lib/api";
  import { appStore } from "$lib/stores/app.svelte";
  import type { AppDefinition, AppPhase } from "$lib/types";

  interface Props {
    appName: string;
  }

  let { appName }: Props = $props();

  // App definition loaded from backend
  let appDef = $state<AppDefinition | null>(null);
  let loadError = $state<string | null>(null);

  // Runtime state
  let inputValues = $state<Record<string, string>>({});
  let selectedModel = $state("");
  let models = $state<string[]>([]);
  let loading = $state(false);
  let showSettings = $state(false);
  let currentPhaseIndex = $state(0);

  // Phase results: array of JSON arrays (one per phase)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let phaseResults = $state<Record<string, any[]>>({});
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let selectedItem = $state<Record<string, any> | null>(null);
  let checkedSteps = $state<Set<number>>(new Set());
  let userNotes = $state("");
  let rawResponse = $state<string | null>(null);
  let expandedSteps = $state<Set<number>>(new Set());

  const STORAGE_KEY = $derived(`app-${appName}`);

  const currentPhase = $derived<AppPhase | null>(
    appDef ? appDef.phases[currentPhaseIndex] ?? null : null
  );

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const currentResults = $derived<any[]>(
    currentPhase ? phaseResults[currentPhase.name] ?? [] : []
  );

  const hasResults = $derived(currentResults.length > 0);

  onMount(async () => {
    try {
      const { getApp } = await import("$lib/api");
      appDef = await getApp(appName);
      // Set defaults from inputs
      if (appDef) {
        for (const input of appDef.inputs) {
          if (input.default && !inputValues[input.name]) {
            inputValues[input.name] = input.default;
          }
        }
      }
    } catch (e) {
      loadError = `Failed to load app: ${e}`;
    }

    loadState();

    try {
      const [modelList, providers] = await Promise.all([
        api.listModels(),
        api.listProviders(),
      ]);
      models = modelList;
      if (models.length > 0 && !selectedModel) {
        const defaultModel = providers
          .filter(p => p.configured && p.default_model)
          .map(p => p.default_model!)[0];
        selectedModel = (defaultModel && models.includes(defaultModel))
          ? defaultModel
          : models[0];
      }
    } catch (e) {
      appStore.toast("Failed to load models", "error");
    }
  });

  function loadState() {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const state = JSON.parse(saved);
        // Only restore if it's AppRunner format (has phaseResults as object)
        if (typeof state.phaseResults !== "object" || state.phaseResults === null) {
          // Old format from hardcoded components — discard
          localStorage.removeItem(STORAGE_KEY);
          return;
        }
        if (state.inputValues) inputValues = state.inputValues;
        if (state.model) selectedModel = state.model;
        phaseResults = state.phaseResults;
        if (state.currentPhaseIndex !== undefined) currentPhaseIndex = state.currentPhaseIndex;
        if (state.selectedItem) selectedItem = state.selectedItem;
        if (state.checkedSteps) checkedSteps = new Set(state.checkedSteps);
      }
    } catch (e) {
      console.error("Failed to load app state:", e);
      localStorage.removeItem(STORAGE_KEY);
    }
  }

  function saveState() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        inputValues,
        model: selectedModel,
        phaseResults,
        currentPhaseIndex,
        selectedItem,
        checkedSteps: [...checkedSteps],
      }));
    } catch (e) {
      console.error("Failed to save app state:", e);
    }
  }

  function buildPrompt(phase: AppPhase): string {
    let prompt = phase.prompt || "";

    // Replace {{input_name}} variables
    for (const [key, value] of Object.entries(inputValues)) {
      prompt = prompt.replaceAll(`{{${key}}}`, value);
    }

    // Replace {{date}} and {{month_year}}
    const now = new Date();
    const dateStr = now.toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" });
    const monthYear = now.toLocaleDateString("en-US", { year: "numeric", month: "long" });
    prompt = prompt.replaceAll("{{date}}", dateStr);
    prompt = prompt.replaceAll("{{month_year}}", monthYear);

    // Replace {{previous_results}} with results from the previous phase
    if (appDef && currentPhaseIndex > 0) {
      const prevPhase = appDef.phases[currentPhaseIndex - 1];
      const prevResults = phaseResults[prevPhase.name];
      if (prevResults) {
        prompt = prompt.replaceAll("{{previous_results}}", JSON.stringify(prevResults, null, 2));
      }
    }

    // Replace {{selected_item.*}} variables
    if (selectedItem) {
      prompt = prompt.replaceAll("{{selected_item}}", JSON.stringify(selectedItem, null, 2));
      for (const [key, value] of Object.entries(selectedItem)) {
        prompt = prompt.replaceAll(`{{selected_item.${key}}}`, String(value));
      }
    }

    // Replace {{completed_steps}} and {{remaining_steps}}
    if (currentResults.length > 0) {
      const completed = currentResults.filter((_, i) => checkedSteps.has(i));
      const remaining = currentResults.filter((_, i) => !checkedSteps.has(i));
      prompt = prompt.replaceAll("{{completed_steps}}", JSON.stringify(completed, null, 2));
      prompt = prompt.replaceAll("{{remaining_steps}}", JSON.stringify(remaining, null, 2));
    }

    // Replace {{notes}}
    prompt = prompt.replaceAll("{{notes}}", userNotes);

    // Handle {{#if notes}} conditional
    if (userNotes) {
      prompt = prompt.replace(/\{\{#if notes\}\}([\s\S]*?)\{\{\/if\}\}/g, "$1");
    } else {
      prompt = prompt.replace(/\{\{#if notes\}\}[\s\S]*?\{\{\/if\}\}/g, "");
    }

    return prompt;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function parseJsonResponse(response: string): any[] {
    try {
      const jsonMatch = response.match(/\[[\s\S]*\]/);
      if (jsonMatch) {
        const parsed = JSON.parse(jsonMatch[0]);
        if (Array.isArray(parsed)) return parsed;
      }
    } catch {
      // Fall through
    }
    return [];
  }

  async function runPhase() {
    if (!currentPhase || !selectedModel) {
      appStore.toast("Please select a model first", "error");
      return;
    }

    loading = true;
    rawResponse = null;
    try {
      const prompt = buildPrompt(currentPhase);
      const response = await api.sendMessage(prompt, selectedModel);
      const parsed = parseJsonResponse(response);

      if (parsed.length > 0) {
        console.log("[AppRunner] Phase:", currentPhase.name, "output_fields:", JSON.stringify(currentPhase.output_fields), "item keys:", Object.keys(parsed[0]));
        phaseResults = { ...phaseResults, [currentPhase.name]: parsed };
        rawResponse = null;
      } else {
        rawResponse = response;
        phaseResults[currentPhase.name] = [];
      }

      saveState();
      appStore.toast(`${currentPhase.label || currentPhase.name} complete`);

      // Auto-advance to next phase if this phase isn't selectable and has more phases
      if (parsed.length > 0 && !currentPhase.selectable && appDef && currentPhaseIndex < appDef.phases.length - 1) {
        currentPhaseIndex++;
        saveState();
      }
    } catch (e) {
      appStore.toast(`Failed: ${e}`, "error");
    } finally {
      loading = false;
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function selectItem(item: Record<string, any>) {
    selectedItem = item;
    // Move to next phase
    if (appDef && currentPhaseIndex < appDef.phases.length - 1) {
      currentPhaseIndex++;
      saveState();
    }
  }

  function goToPhase(index: number) {
    if (appDef && index >= 0 && index < appDef.phases.length) {
      currentPhaseIndex = index;
    }
  }

  function toggleStep(index: number) {
    const next = new Set(checkedSteps);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    checkedSteps = next;
    saveState();
  }

  function toggleExpandStep(index: number) {
    const next = new Set(expandedSteps);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    expandedSteps = next;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function getField(item: Record<string, any>, fieldName: string | undefined): string {
    if (!fieldName) return "";
    return String(item[fieldName] ?? "");
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function getTagsField(item: Record<string, any>, fieldName: string | undefined): string[] {
    if (!fieldName || !item[fieldName]) return [];
    const val = item[fieldName];
    if (Array.isArray(val)) return val.map(String);
    return String(val).split(",").map(s => s.trim());
  }

  const badgeColors: Record<string, string> = {};
  const colorPool = [
    "bg-blue-500/15 text-blue-400",
    "bg-emerald-500/15 text-emerald-400",
    "bg-purple-500/15 text-purple-400",
    "bg-amber-500/15 text-amber-400",
    "bg-rose-500/15 text-rose-400",
    "bg-cyan-500/15 text-cyan-400",
    "bg-indigo-500/15 text-indigo-400",
    "bg-orange-500/15 text-orange-400",
  ];
  let colorIndex = 0;

  function getBadgeColor(value: string): string {
    const key = value.toLowerCase();
    // Special colors for risk levels
    if (key === "low") return "bg-emerald-500/15 text-emerald-400";
    if (key === "medium") return "bg-amber-500/15 text-amber-400";
    if (key === "high") return "bg-rose-500/15 text-rose-400";
    if (!badgeColors[key]) {
      badgeColors[key] = colorPool[colorIndex % colorPool.length];
      colorIndex++;
    }
    return badgeColors[key];
  }

  function resetApp() {
    phaseResults = {};
    selectedItem = null;
    checkedSteps = new Set();
    currentPhaseIndex = 0;
    rawResponse = null;
    userNotes = "";
    saveState();
  }
</script>

{#if loadError}
  <div class="p-6">
    <EmptyState icon={X} title="Failed to load app" description={loadError} />
  </div>
{:else if !appDef}
  <div class="flex items-center justify-center py-16">
    <Spinner size={24} />
  </div>
{:else}
  <div class="space-y-6">
    <!-- Phase indicator (when multiple phases) -->
    {#if appDef.phases.length > 1}
      <div class="flex items-center gap-2">
        {#each appDef.phases as phase, i (phase.name)}
          <button
            onclick={() => goToPhase(i)}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
              {i === currentPhaseIndex
                ? 'bg-accent/15 text-accent'
                : phaseResults[phase.name]
                  ? 'bg-emerald-500/10 text-emerald-400/80 hover:bg-emerald-500/15'
                  : 'text-slate-500 hover:bg-bg-tertiary hover:text-slate-400'}"
          >
            {#if phaseResults[phase.name]}
              <Check size={12} />
            {/if}
            {phase.label || phase.name}
          </button>
          {#if i < appDef.phases.length - 1}
            <span class="text-slate-600 text-xs">&rarr;</span>
          {/if}
        {/each}

        <div class="flex-1"></div>
        <button
          onclick={resetApp}
          class="text-xs text-slate-500 hover:text-slate-300 transition-colors"
        >
          Reset
        </button>
      </div>
    {/if}

    <!-- Settings toggle + run button -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-base font-semibold">
          {currentPhase?.label || currentPhase?.name || appDef.metadata.name}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <button
          onclick={() => showSettings = !showSettings}
          class="p-2 rounded-lg text-slate-400 hover:bg-bg-tertiary hover:text-slate-200 transition-colors"
          title="Settings"
        >
          {#if showSettings}
            <X size={18} />
          {:else}
            <Settings2 size={18} />
          {/if}
        </button>
        <Button onclick={runPhase} disabled={loading || (currentPhase?.requires_notes && !userNotes)}>
          {#if loading}
            <Spinner />
            Running...
          {:else}
            <RefreshCw size={16} />
            {currentPhase?.button || "Run"}
          {/if}
        </Button>
      </div>
    </div>

    <!-- Settings Panel -->
    {#if showSettings}
      <div class="bg-bg-tertiary border border-border rounded-xl p-4 space-y-4">
        {#each appDef.inputs as input (input.name)}
          <div class="space-y-2">
            <label for="input-{input.name}" class="text-sm font-medium text-slate-300">
              {input.label || input.name}
            </label>
            {#if input.input_type === "select"}
              <select
                id="input-{input.name}"
                bind:value={inputValues[input.name]}
                onchange={saveState}
                style="color: rgb(226, 232, 240); background-color: #1E293B;"
                class="w-full px-3 py-2 border border-border rounded-lg text-sm
                  focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
                  transition-all duration-200 cursor-pointer
                  [&>option]:text-slate-200 [&>option]:bg-slate-800"
              >
                {#each input.options as opt}
                  <option value={opt.value}>{opt.label}</option>
                {/each}
              </select>
            {:else if input.input_type === "number"}
              <input
                id="input-{input.name}"
                type="number"
                bind:value={inputValues[input.name]}
                onchange={saveState}
                min={input.min}
                max={input.max}
                class="w-full px-3 py-2 bg-bg-secondary border border-border rounded-lg text-sm text-slate-200
                  placeholder-slate-500 focus:outline-none focus:border-accent
                  focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)] transition-all duration-200"
              />
            {:else}
              <input
                id="input-{input.name}"
                type="text"
                bind:value={inputValues[input.name]}
                onchange={saveState}
                placeholder={input.placeholder || ""}
                class="w-full px-3 py-2 bg-bg-secondary border border-border rounded-lg text-sm text-slate-200
                  placeholder-slate-500 focus:outline-none focus:border-accent
                  focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)] transition-all duration-200"
              />
            {/if}
          </div>
        {/each}

        <!-- Model selector -->
        <div class="space-y-2">
          <label for="app-model" class="text-sm font-medium text-slate-300">Model</label>
          <select
            id="app-model"
            bind:value={selectedModel}
            onchange={saveState}
            style="color: rgb(226, 232, 240); background-color: #1E293B;"
            class="w-full px-3 py-2 border border-border rounded-lg text-sm
              focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
              transition-all duration-200 cursor-pointer
              [&>option]:text-slate-200 [&>option]:bg-slate-800"
          >
            {#if models.length === 0}
              <option value="">No models available</option>
            {:else}
              {#each models as model}
                <option value={model}>{model}</option>
              {/each}
            {/if}
          </select>
        </div>
      </div>
    {/if}

    <!-- Notes input (for phases that require it) -->
    {#if currentPhase?.requires_notes}
      <div class="space-y-2">
        <label for="user-notes" class="text-sm font-medium text-slate-300">Notes / Progress Update</label>
        <textarea
          id="user-notes"
          bind:value={userNotes}
          placeholder="Describe your progress, results, or any changes..."
          rows={3}
          class="w-full px-3 py-2 bg-bg-secondary border border-border rounded-lg text-sm text-slate-200
            placeholder-slate-500 focus:outline-none focus:border-accent
            focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)] transition-all duration-200 resize-y"
        ></textarea>
      </div>
    {/if}

    <!-- Results display -->
    {#if loading && !hasResults}
      <div class="flex flex-col items-center justify-center py-16 gap-4">
        <Spinner size={24} />
        <p class="text-sm text-slate-500">Running {currentPhase?.label || "phase"}...</p>
      </div>
    {:else if rawResponse}
      <!-- Raw text fallback -->
      <div class="bg-bg-secondary border border-border rounded-xl p-5">
        <pre class="text-sm text-slate-300 whitespace-pre-wrap">{rawResponse}</pre>
      </div>
    {:else if hasResults && currentPhase}
      {console.log("[AppRunner:render]", currentPhase.name, "output_fields:", JSON.stringify(currentPhase.output_fields), "item[0] keys:", Object.keys(currentResults[0] || {})), ""}
      {#if currentPhase.output === "checklist"}
        <!-- Checklist output -->
        <div class="space-y-2">
          {#each currentResults as item, i (i)}
            {@const fields = currentPhase.output_fields}
            {@const title = getField(item, fields?.title)}
            {@const body = getField(item, fields?.body)}
            {@const meta = fields?.meta || []}
            <div class="bg-bg-secondary border border-border rounded-xl overflow-hidden transition-colors hover:border-slate-600">
              <div
                class="flex items-center gap-3 p-4 cursor-pointer"
                role="button"
                tabindex="0"
                onclick={() => toggleExpandStep(i)}
                onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpandStep(i); } }}
              >
                <button
                  onclick={(e) => { e.stopPropagation(); toggleStep(i); }}
                  class="w-5 h-5 rounded border-2 flex items-center justify-center shrink-0 transition-colors
                    {checkedSteps.has(i) ? 'bg-accent border-accent' : 'border-slate-600 hover:border-slate-400'}"
                >
                  {#if checkedSteps.has(i)}
                    <Check size={12} class="text-bg-primary" />
                  {/if}
                </button>
                <div class="flex-1 min-w-0">
                  <span class="text-sm font-medium {checkedSteps.has(i) ? 'text-slate-500 line-through' : 'text-slate-200'}">
                    {item.step ? `${item.step}. ` : `${i + 1}. `}{title}
                  </span>
                </div>
                {#if expandedSteps.has(i)}
                  <ChevronDown size={14} class="text-slate-500 shrink-0" />
                {:else}
                  <ChevronRight size={14} class="text-slate-500 shrink-0" />
                {/if}
              </div>
              {#if expandedSteps.has(i)}
                <div class="px-4 pb-4 pl-12 space-y-2">
                  {#if body}
                    <p class="text-sm text-slate-400">{body}</p>
                  {/if}
                  {#if meta.length > 0}
                    <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                      {#each meta as m}
                        {@const val = getField(item, m.field)}
                        {#if val}
                          <span>
                            <span class="text-slate-500">{m.label}:</span>
                            <span class="text-slate-300 ml-1">{val}</span>
                          </span>
                        {/if}
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <!-- Cards output (default) -->
        <div class="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
          {#each currentResults as item, i (i)}
            {@const fields = currentPhase.output_fields}
            {@const title = getField(item, fields?.title)}
            {@const body = getField(item, fields?.body)}
            {@const subtitle = getField(item, fields?.subtitle)}
            {@const badge = getField(item, fields?.badge)}
            {@const footerLeft = getField(item, fields?.footer_left)}
            {@const footerRight = getField(item, fields?.footer_right)}
            {@const tags = getTagsField(item, fields?.tags)}
            {@const meta = fields?.meta || []}
            <div class="bg-bg-secondary border border-border rounded-xl p-5 space-y-3 hover:border-slate-600 transition-colors">
              <div class="flex items-start justify-between gap-3">
                <h3 class="font-medium text-slate-200 leading-snug">{title}</h3>
                {#if badge}
                  <span class="shrink-0 px-2 py-0.5 rounded-full text-[11px] font-medium {getBadgeColor(badge)}">
                    {badge}
                  </span>
                {/if}
              </div>
              {#if subtitle}
                <p class="text-xs text-slate-500">{subtitle}</p>
              {/if}
              {#if body}
                <p class="text-sm text-slate-400 leading-relaxed">{body}</p>
              {/if}
              {#if meta.length > 0}
                <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                  {#each meta as m}
                    {@const val = getField(item, m.field)}
                    {#if val}
                      <span>
                        <span class="text-slate-500">{m.label}:</span>
                        <span class="text-slate-300 ml-1">{val}</span>
                      </span>
                    {/if}
                  {/each}
                </div>
              {/if}
              {#if tags.length > 0}
                <div class="flex flex-wrap gap-1.5">
                  {#each tags as tag}
                    <span class="px-2 py-0.5 rounded-full text-[10px] bg-slate-500/15 text-slate-400">
                      {tag}
                    </span>
                  {/each}
                </div>
              {/if}
              {#if footerLeft || footerRight}
                <div class="flex items-center justify-between text-xs text-slate-500">
                  <span>{footerLeft}</span>
                  <span>{footerRight}</span>
                </div>
              {/if}
              {#if currentPhase.selectable}
                <Button size="sm" onclick={() => selectItem(item)}>
                  {currentPhase.select_prompt || "Select"}
                </Button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {:else if !loading}
      <EmptyState
        icon={RefreshCw}
        title="No results yet"
        description="Click '{currentPhase?.button || 'Run'}' to start"
      />
    {/if}
  </div>
{/if}
