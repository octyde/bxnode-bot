<script lang="ts">
  import { onMount } from "svelte";
  import { Wrench, Server, MessageSquare, Brain, Activity, RefreshCw, Download } from "lucide-svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import SearchBox from "$lib/components/ui/SearchBox.svelte";
  import SkillCard from "$lib/components/skills/SkillCard.svelte";
  import SkillModal from "$lib/components/skills/SkillModal.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import { configStore } from "$lib/stores/config.svelte";
  import { providersStore } from "$lib/stores/providers.svelte";
  import { channelsStore } from "$lib/stores/channels.svelte";
  import { appStore } from "$lib/stores/app.svelte";
  import type { SkillDetail } from "$lib/types";

  let searchQuery = $state("");
  let selectedSkill = $state<SkillDetail | null>(null);
  let modalOpen = $state(false);

  const filteredSkills = $derived(
    skillsStore.skills.filter((s) =>
      s.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.description.toLowerCase().includes(searchQuery.toLowerCase())
    )
  );

  const stats = $derived([
    { label: "Total Skills", value: skillsStore.totalSkills, icon: Wrench, color: "text-accent" },
    { label: "Active", value: skillsStore.activeSkills, icon: Activity, color: "text-accent" },
    { label: "Providers", value: providersStore.count, icon: Server, color: "text-accent" },
    { label: "Channels", value: channelsStore.configuredCount, icon: MessageSquare, color: "text-accent" },
  ]);

  async function openSkill(name: string) {
    selectedSkill = await skillsStore.getDetail(name);
    if (selectedSkill) modalOpen = true;
  }

  async function toggleModal() {
    if (!selectedSkill) return;
    await skillsStore.toggle(selectedSkill.name, !selectedSkill.is_active);
    modalOpen = false;
    selectedSkill = null;
  }

  onMount(() => {
    providersStore.refresh();
    channelsStore.refresh();
  });
</script>

<div class="p-6 space-y-6">
  <!-- Stats -->
  <div class="grid grid-cols-4 gap-4">
    {#each stats as stat}
      <Card>
        <div class="flex items-center gap-4">
          <div class="w-11 h-11 rounded-lg bg-accent/15 flex items-center justify-center">
            <svelte:component this={stat.icon} size={22} class={stat.color} />
          </div>
          <div>
            <h3 class="text-2xl font-semibold font-mono">{stat.value}</h3>
            <p class="text-sm text-slate-500">{stat.label}</p>
          </div>
        </div>
      </Card>
    {/each}
  </div>

  <!-- Toolbar -->
  <div class="flex gap-3">
    <SearchBox bind:value={searchQuery} placeholder="Search skills..." />
    <Button variant="secondary" onclick={() => skillsStore.refresh()}>
      <RefreshCw size={16} />
      Refresh
    </Button>
    <Button onclick={() => skillsStore.sync()} disabled={skillsStore.loading}>
      <Download size={16} />
      Sync Skills
    </Button>
  </div>

  <!-- Skills Grid -->
  <div class="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
    {#each filteredSkills as skill (skill.name)}
      <SkillCard
        {skill}
        onclick={() => openSkill(skill.name)}
        ontoggle={(active) => skillsStore.toggle(skill.name, active)}
      />
    {:else}
      <div class="col-span-full text-center py-16 text-slate-500">
        {#if skillsStore.loading}
          <p>Loading skills...</p>
        {:else}
          <p>No skills found. Click "Sync Skills" to download.</p>
        {/if}
      </div>
    {/each}
  </div>
</div>

<SkillModal
  skill={selectedSkill}
  open={modalOpen}
  onclose={() => { modalOpen = false; selectedSkill = null; }}
  ontoggle={toggleModal}
/>
