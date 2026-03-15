<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    type?: string;
    oninput?: (e: Event) => void;
  }

  let { value = $bindable(""), placeholder = "", type = "text", oninput }: Props = $props();
</script>

<input
  {type}
  bind:value
  {placeholder}
  {oninput}
  class="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-lg text-slate-200 text-sm
    placeholder:text-slate-500 focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_rgba(34,197,94,0.15)]
    transition-all duration-200"
/>
