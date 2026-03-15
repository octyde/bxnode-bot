<script lang="ts">
  import type { CronJob } from "$lib/types";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { Play, Trash2 } from "lucide-svelte";

  interface Props {
    job: CronJob;
    onrun?: () => void;
    onremove?: () => void;
  }

  let { job, onrun, onremove }: Props = $props();
</script>

<tr class="border-b border-border hover:bg-bg-tertiary/50 transition-colors">
  <td class="px-4 py-3 font-mono text-sm">{job.id}</td>
  <td class="px-4 py-3 font-mono text-xs text-slate-400">{job.schedule}</td>
  <td class="px-4 py-3 text-sm text-slate-400">{job.description ?? "—"}</td>
  <td class="px-4 py-3">
    <Badge variant={job.enabled ? "success" : "muted"}>
      {job.enabled ? "Enabled" : "Disabled"}
    </Badge>
  </td>
  <td class="px-4 py-3 text-xs text-slate-500">—</td>
  <td class="px-4 py-3">
    <div class="flex gap-1">
      <Button variant="ghost" size="sm" onclick={onrun}><Play size={14} /></Button>
      <Button variant="ghost" size="sm" onclick={onremove}><Trash2 size={14} /></Button>
    </div>
  </td>
</tr>
