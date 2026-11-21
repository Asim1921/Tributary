<script lang="ts">
  import { t } from '../lib/locale.svelte';
  import type { Segment } from '../lib/types';

  let { segments, size }: { segments: Segment[]; size: number } = $props();
</script>

<div class="chart" role="img" aria-label={t('Progress of each connection')}>
  {#each segments as segment (segment.start)}
    <span
      class="part"
      style:inset-inline-start="{(segment.start / size) * 100}%"
      style:width="{((segment.end - segment.start) / size) * 100}%"
    >
      <span
        class="done"
        style:width="{segment.end > segment.start
          ? (segment.written / (segment.end - segment.start)) * 100
          : 100}%"
      ></span>
    </span>
  {/each}
</div>

<style>
  .chart {
    position: relative;
    height: 22px;
    overflow: hidden;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 4px;
  }

  .part {
    position: absolute;
    top: 0;
    bottom: 0;
    border-inline-end: 1px solid var(--surface);
  }

  .done {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s linear;
  }
</style>
