<script lang="ts">
  import { Wrench, Download, RefreshCw } from "lucide-svelte";
  import SearchBox from "$lib/components/ui/SearchBox.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import SkillCard from "$lib/components/skills/SkillCard.svelte";
  import SkillModal from "$lib/components/skills/SkillModal.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import type { SkillDetail } from "$lib/types";

  let searchQuery = $state("");
  let selectedSkill = $state<SkillDetail | null>(null);
  let modalOpen = $state(false);

  const filtered = $derived(
    skillsStore.skills.filter((s) =>
      s.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.description.toLowerCase().includes(searchQuery.toLowerCase())
    )
  );

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
</script>

<div class="p-6 space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">Skills Manager</h1>
      <p class="text-sm text-slate-500">Manage OpenClaw / Agent Skills</p>
    </div>
    <div class="flex gap-2">
      <Button variant="secondary" onclick={() => skillsStore.refresh()}>
        <RefreshCw size={16} />
        Refresh
      </Button>
      <Button onclick={() => skillsStore.sync()} disabled={skillsStore.loading}>
        <Download size={16} />
        Sync
      </Button>
    </div>
  </div>

  <SearchBox bind:value={searchQuery} placeholder="Search skills..." />

  {#if filtered.length === 0}
    <EmptyState icon={Wrench} title="No skills found" description='Click "Sync" to download from awesome-openclaw-skills' />
  {:else}
    <div class="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
      {#each filtered as skill (skill.name)}
        <SkillCard
          {skill}
          onclick={() => openSkill(skill.name)}
          ontoggle={(active) => skillsStore.toggle(skill.name, active)}
        />
      {/each}
    </div>
  {/if}
</div>

<SkillModal
  skill={selectedSkill}
  open={modalOpen}
  onclose={() => { modalOpen = false; selectedSkill = null; }}
  ontoggle={toggleModal}
/>
