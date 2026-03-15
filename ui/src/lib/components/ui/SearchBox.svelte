<script lang="ts">
  import { Search } from "lucide-svelte";

  interface Props {
    value?: string;
    placeholder?: string;
  }

  let { value = $bindable(""), placeholder = "Search..." }: Props = $props();
</script>

<div class="relative flex-1">
  <span class="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500">
    <Search size={16} />
  </span>
  <input
    type="text"
    bind:value
    {placeholder}
    class="w-full pl-10 pr-3 py-2.5 bg-bg-secondary border border-border rounded-lg text-slate-200 text-sm
      placeholder:text-slate-500 focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
      transition-all duration-200"
  />
</div>
