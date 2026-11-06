<script lang="ts">
  import type { ToolbarAction, ToolbarButton } from '../lib/toolbar';
  import type { ToolbarSize } from '../lib/types';
  import Icon from './Icon.svelte';

  interface Props {
    buttons: ToolbarButton[];
    size: ToolbarSize;
    enabled: (action: ToolbarAction) => boolean;
    onaction: (action: ToolbarAction) => void;
  }

  let { buttons, size, enabled, onaction }: Props = $props();
</script>

<div class="toolbar" class:small={size === 'small'} role="toolbar" aria-label="Downloads">
  {#each buttons as button (button.id)}
    <button
      type="button"
      title={button.label}
      aria-label={button.label}
      disabled={!enabled(button.id)}
      onclick={() => onaction(button.id)}
    >
      <Icon name={button.icon} size={size === 'small' ? 18 : 22} />
      {#if size === 'large'}<span>{button.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .toolbar {
    display: flex;
    gap: 2px;
    padding: 6px 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }

  button {
    display: grid;
    justify-items: center;
    gap: 2px;
    min-width: 76px;
    padding: 6px 8px;
    background: transparent;
    border-color: transparent;
    font-size: 12px;
  }

  .small button {
    min-width: 0;
    padding: 6px;
  }
</style>
