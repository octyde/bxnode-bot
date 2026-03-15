<script lang="ts">
  import type { SkillInfo } from "$lib/types";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import { FileText, Code } from "lucide-svelte";

  interface Props {
    skill: SkillInfo;
    onclick?: () => void;
    ontoggle?: (active: boolean) => void;
  }

  let { skill, onclick, ontoggle }: Props = $props();
</script>

<div
  class="bg-bg-secondary border border-border rounded-xl p-5 cursor-pointer transition-all duration-200
    hover:border-accent hover:shadow-[0_0_20px_rgba(34,197,94,0.15)] flex flex-col gap-3"
  onclick={onclick}
  role="button"
  tabindex={0}
  onkeydown={(e) => { if (e.key === 'Enter') onclick?.() }}
>
  <div class="flex items-start justify-between gap-4">
    <span class="font-semibold text-sm font-mono text-slate-200">{skill.name}</span>
    <Badge variant={skill.is_active ? "success" : "muted"}>
      {skill.is_active ? "Active" : "Inactive"}
    </Badge>
  </div>

  <p class="text-slate-400 text-[13px] leading-relaxed line-clamp-2">{skill.description}</p>

  <div class="flex items-center gap-4 mt-auto pt-3 border-t border-border">
    {#if skill.license}
      <span class="flex items-center gap-1 text-[11px] text-slate-500">
        <FileText size={12} />
        {skill.license}
      </span>
    {/if}
    <span class="flex items-center gap-1 text-[11px] text-slate-500">
      <FileText size={12} />
      {skill.reference_count} refs
    </span>
    <span class="flex items-center gap-1 text-[11px] text-slate-500">
      <Code size={12} />
      {skill.script_count} scripts
    </span>
    <div class="ml-auto" onclick={(e) => e.stopPropagation()} role="none">
      <Toggle checked={skill.is_active} onchange={(v) => ontoggle?.(v)} />
    </div>
  </div>
</div>
