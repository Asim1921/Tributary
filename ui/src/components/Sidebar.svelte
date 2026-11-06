<script lang="ts">
  import type { Filter, FilterEntry } from '../lib/filters';

  interface Props {
    groups: { title: string; entries: FilterEntry[] }[];
    activeKey: string;
    onchange: (filter: Filter) => void;
  }

  let { groups, activeKey, onchange }: Props = $props();
</script>

<nav aria-label="Filter downloads">
  {#each groups as group (group.title)}
    <h2>{group.title}</h2>
    {#each group.entries as entry (entry.key)}
      <button
        type="button"
        class:active={entry.key === activeKey}
        aria-current={entry.key === activeKey ? 'true' : undefined}
        onclick={() => onchange(entry.filter)}
      >
        <span class="label">{entry.label}</span>
        <span class="count">{entry.count}</span>
      </button>
    {/each}
  {/each}
</nav>

<style>
  nav {
    display: grid;
    align-content: start;
    gap: 1px;
    width: 180px;
    flex: none;
    padding: 4px 8px 8px;
    overflow-y: auto;
    background: var(--surface);
    border-right: 1px solid var(--line);
  }

  h2 {
    margin: 10px 10px 3px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  button {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 10px;
    background: transparent;
    border-color: transparent;
    text-align: left;
  }

  button.active {
    background: var(--selected);
    font-weight: 600;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
