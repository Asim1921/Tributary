<script lang="ts">
  import type { UpdateInfo } from '../lib/types';

  interface Props {
    update: UpdateInfo;
    onrestart: () => void;
    onlater: () => void;
  }

  let { update, onrestart, onlater }: Props = $props();
</script>

<div class="bar" role="status">
  <span>
    Tributary {update.version} is ready.
    {#if update.notes}<span class="notes" title={update.notes}>What's new</span>{/if}
  </span>
  <span class="actions">
    <button type="button" onclick={onlater}>Later</button>
    <button type="button" class="primary" onclick={onrestart}>Restart to update</button>
  </span>
</div>

<style>
  .bar {
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
    padding: 6px 14px;
    background: var(--selected);
    border-bottom: 1px solid var(--line);
  }

  .notes {
    margin-left: 8px;
    color: var(--muted);
    text-decoration: underline dotted;
    cursor: help;
  }

  .actions {
    display: flex;
    gap: 8px;
  }
</style>
