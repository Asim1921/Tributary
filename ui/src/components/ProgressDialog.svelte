<script lang="ts">
  import {
    displayName,
    formatBytes,
    formatSpeed,
    formatTimeLeft,
    percent,
    statusText,
  } from '../lib/format';
  import type { DownloadRow } from '../lib/types';
  import Modal from './Modal.svelte';
  import SegmentChart from './SegmentChart.svelte';

  interface Props {
    row: DownloadRow;
    onpause: () => void;
    onresume: () => void;
    onclose: () => void;
  }

  let { row, onpause, onresume, onclose }: Props = $props();

  const value = $derived(percent(row));
</script>

<Modal title={displayName(row)} wide {onclose}>
  <p class="note selectable">{row.url}</p>

  <dl>
    <dt>Status</dt>
    <dd>{statusText(row)}</dd>
    <dt>Downloaded</dt>
    <dd>{formatBytes(row.downloaded)} of {formatBytes(row.size)}</dd>
    <dt>Speed</dt>
    <dd>{row.state === 'running' ? formatSpeed(row.speed) : '—'}</dd>
    <dt>Time left</dt>
    <dd>{formatTimeLeft(row) || '—'}</dd>
    <dt>Resume</dt>
    <dd>{row.resumable ? 'Supported' : 'Not supported'}</dd>
  </dl>

  {#if row.error}
    <p class="error-text">{row.error}</p>
  {/if}

  <p class="heading">Connections</p>
  {#if row.segments.length > 0 && row.size}
    <SegmentChart segments={row.segments} size={row.size} />
  {:else}
    <div class="total" role="progressbar" aria-valuenow={value ?? 0} aria-valuemin={0} aria-valuemax={100}>
      <span style:width="{value ?? 0}%"></span>
    </div>
  {/if}

  {#snippet footer()}
    {#if row.state === 'running'}
      <button type="button" onclick={onpause}>Stop</button>
    {:else if row.state !== 'completed'}
      <button type="button" onclick={onresume}>Resume</button>
    {/if}
    <button type="button" class="primary" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<style>
  dl {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 4px 12px;
    margin: 12px 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .heading {
    margin: 12px 0 6px;
    font-weight: 600;
  }

  .total {
    height: 22px;
    overflow: hidden;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 4px;
  }

  .total span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
</style>
