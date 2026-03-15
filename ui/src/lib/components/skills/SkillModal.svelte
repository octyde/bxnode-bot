<script lang="ts">
  import type { SkillDetail } from "$lib/types";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";

  interface Props {
    skill: SkillDetail | null;
    open?: boolean;
    onclose?: () => void;
    ontoggle?: () => void;
  }

  let { skill, open = false, onclose, ontoggle }: Props = $props();
</script>

<Modal {open} title={skill?.name ?? ""} {onclose}>
  {#if skill}
    <div class="space-y-6">
      <div>
        <h4 class="text-[11px] font-semibold uppercase tracking-widest text-slate-500 mb-2">Description</h4>
        <p class="text-slate-300 text-sm">{skill.description}</p>
      </div>

      <div>
        <h4 class="text-[11px] font-semibold uppercase tracking-widest text-slate-500 mb-2">Status</h4>
        <Badge variant={skill.is_active ? "success" : "muted"}>
          {skill.is_active ? "Active" : "Inactive"}
        </Badge>
      </div>

      {#if skill.license}
        <div>
          <h4 class="text-[11px] font-semibold uppercase tracking-widest text-slate-500 mb-2">License</h4>
          <p class="text-slate-300 text-sm">{skill.license}</p>
        </div>
      {/if}

      {#if skill.instructions}
        <div>
          <h4 class="text-[11px] font-semibold uppercase tracking-widest text-slate-500 mb-2">Instructions</h4>
          <pre class="bg-bg-primary border border-border rounded-lg p-4 text-sm text-slate-400 overflow-auto max-h-[300px] whitespace-pre-wrap">{skill.instructions}</pre>
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    <Button variant="secondary" onclick={ontoggle}>
      {skill?.is_active ? "Disable" : "Enable"}
    </Button>
    <Button onclick={onclose}>Done</Button>
  {/snippet}
</Modal>
