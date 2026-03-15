<script lang="ts">
  import { Shield, Check, X, User } from "lucide-svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { serverStore } from "$lib/stores/server.svelte";

  const channelNames: Record<string, string> = {
    telegram: "Telegram",
    discord: "Discord",
    slack: "Slack",
    line: "LINE",
    signal: "Signal",
    feishu: "Feishu",
  };

  function formatTime(ts: string) {
    try {
      return new Date(ts).toLocaleTimeString();
    } catch {
      return ts;
    }
  }
</script>

{#if serverStore.pendingApprovalCount > 0}
  <div class="space-y-3">
    <div class="flex items-center gap-2">
      <Shield size={16} class="text-amber-400" />
      <h3 class="text-sm font-semibold">Pending Approvals</h3>
      <Badge variant="warning">{serverStore.pendingApprovalCount}</Badge>
    </div>
    {#each serverStore.pendingApprovals as approval}
      <Card>
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-lg bg-amber-500/15 flex items-center justify-center shrink-0">
            <User size={20} class="text-amber-400" />
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium">
                {approval.userName || approval.userId}
              </span>
              <span class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded bg-bg-tertiary text-slate-400">
                {channelNames[approval.channel] || approval.channel}
              </span>
              <span class="text-[10px] text-slate-600 ml-auto">{formatTime(approval.timestamp)}</span>
            </div>
            <p class="text-xs text-slate-500 mt-0.5 truncate">
              "{approval.firstMessage}"
            </p>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <Button variant="primary" size="sm" onclick={() => serverStore.approveUser(approval.id)}>
              <Check size={14} />
              Approve
            </Button>
            <Button variant="danger" size="sm" onclick={() => serverStore.rejectUser(approval.id)}>
              <X size={14} />
              Reject
            </Button>
          </div>
        </div>
      </Card>
    {/each}
  </div>
{/if}
