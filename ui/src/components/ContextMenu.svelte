<script lang="ts">
  export interface MenuItem {
    label: string;
    disabled?: boolean;
    separatorBefore?: boolean;
    run: () => void;
  }

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
  }

  let { x, y, items, onclose }: Props = $props();

  let element: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);

  // Keeps the whole menu on screen near the window's edges.
  $effect(() => {
    const box = element.getBoundingClientRect();
    left = Math.min(x, window.innerWidth - box.width - 4);
    top = Math.min(y, window.innerHeight - box.height - 4);
    element.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  });
</script>

<svelte:window
  onpointerdown={(event) => {
    if (!element.contains(event.target as Node)) onclose();
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') onclose();
  }}
  onblur={onclose}
/>

<div bind:this={element} class="menu" role="menu" style:left="{left}px" style:top="{top}px">
  {#each items as item (item.label)}
    {#if item.separatorBefore}<hr />{/if}
    <button
      type="button"
      role="menuitem"
      disabled={item.disabled}
      onclick={() => {
        onclose();
        item.run();
      }}
    >
      {item.label}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 10;
    display: grid;
    min-width: 180px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }

  button {
    padding: 5px 10px;
    background: transparent;
    border-color: transparent;
    text-align: left;
  }

  hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
</style>
