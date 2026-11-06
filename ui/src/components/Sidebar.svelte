<script lang="ts">
  import type { Filter } from '../lib/filters';

  interface Props {
    filters: { id: Filter; label: string; count: number }[];
    active: Filter;
    onchange: (filter: Filter) => void;
  }

  let { filters, active, onchange }: Props = $props();
</script>

<nav aria-label="Filter downloads">
  {#each filters as filter (filter.id)}
    <button
      type="button"
      class:active={filter.id === active}
      aria-current={filter.id === active ? 'true' : undefined}
      onclick={() => onchange(filter.id)}
    >
      <span>{filter.label}</span>
      <span class="count">{filter.count}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    display: grid;
    align-content: start;
    gap: 2px;
    width: 170px;
    flex: none;
    padding: 8px;
    background: var(--surface);
    border-right: 1px solid var(--line);
  }

  button {
    display: flex;
    justify-content: space-between;
    padding: 6px 10px;
    background: transparent;
    border-color: transparent;
    text-align: left;
  }

  button.active {
    background: var(--selected);
    font-weight: 600;
  }

  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
