<script lang="ts">
  interface Props {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
  }

  let { checked = $bindable(false), onchange }: Props = $props();

  function handleChange() {
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  class="relative w-10 h-[22px] rounded-full transition-all duration-200 {checked ? 'bg-accent' : 'bg-bg-elevated'}"
  onclick={handleChange}
  role="switch"
  aria-checked={checked}
>
  <span
    class="absolute w-4 h-4 rounded-full bg-slate-100 transition-all duration-200 top-[3px]
      {checked ? 'left-[21px]' : 'left-[3px]'}"
  ></span>
</button>
