<script lang="ts">
  interface Props {
    size?: number;
  }

  let { size = 16 }: Props = $props();
</script>

<span
  class="inline-block border-2 border-bg-tertiary border-t-current rounded-full animate-spin"
  style="width: {size}px; height: {size}px;"
></span>
