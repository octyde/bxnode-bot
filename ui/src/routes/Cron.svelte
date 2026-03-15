<script lang="ts">
  import { onMount } from "svelte";
  import { Clock, Plus } from "lucide-svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Input from "$lib/components/ui/Input.svelte";
  import CronJobRow from "$lib/components/cron/CronJobRow.svelte";
  import { cronStore } from "$lib/stores/cron.svelte";

  let showAddModal = $state(false);
  let newId = $state("");
  let newSchedule = $state("");
  let newDescription = $state("");

  async function handleAdd() {
    if (!newId || !newSchedule) return;
    await cronStore.add(newId, newSchedule, {}, newDescription || undefined);
    showAddModal = false;
    newId = "";
    newSchedule = "";
    newDescription = "";
  }

  onMount(() => cronStore.refresh());
</script>

<div class="p-6 space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">Cron Jobs</h1>
      <p class="text-sm text-slate-500">Scheduled job management</p>
    </div>
    <Button onclick={() => { showAddModal = true; }}>
      <Plus size={16} />
      Add Job
    </Button>
  </div>

  {#if cronStore.jobs.length === 0}
    <EmptyState icon={Clock} title="No cron jobs" description="Add a job using the button above or define jobs in config.yaml" />
  {:else}
    <div class="bg-bg-secondary border border-border rounded-xl overflow-hidden">
      <table class="w-full text-left">
        <thead>
          <tr class="border-b border-border text-xs text-slate-500 uppercase tracking-wider">
            <th class="px-4 py-3 font-medium">ID</th>
            <th class="px-4 py-3 font-medium">Schedule</th>
            <th class="px-4 py-3 font-medium">Description</th>
            <th class="px-4 py-3 font-medium">Status</th>
            <th class="px-4 py-3 font-medium">Last Run</th>
            <th class="px-4 py-3 font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each cronStore.jobs as job (job.id)}
            <CronJobRow
              {job}
              onrun={() => cronStore.run(job.id)}
              onremove={() => cronStore.remove(job.id)}
            />
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<Modal open={showAddModal} title="Add Cron Job" onclose={() => { showAddModal = false; }}>
  <div class="space-y-4">
    <div>
      <label class="block text-xs text-slate-500 mb-1.5">Job ID</label>
      <Input bind:value={newId} placeholder="daily-summary" />
    </div>
    <div>
      <label class="block text-xs text-slate-500 mb-1.5">Schedule (6-field cron)</label>
      <Input bind:value={newSchedule} placeholder="0 0 9 * * *" />
    </div>
    <div>
      <label class="block text-xs text-slate-500 mb-1.5">Description</label>
      <Input bind:value={newDescription} placeholder="Daily morning summary" />
    </div>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => { showAddModal = false; }}>Cancel</Button>
    <Button onclick={handleAdd} disabled={!newId || !newSchedule}>Add Job</Button>
  {/snippet}
</Modal>
