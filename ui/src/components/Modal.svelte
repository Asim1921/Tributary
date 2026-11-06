<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    wide?: boolean;
    onclose: () => void;
    children: Snippet;
    footer: Snippet;
  }

  let { title, wide = false, onclose, children, footer }: Props = $props();

  let element: HTMLDialogElement;

  $effect(() => {
    element.showModal();
  });
</script>

<dialog
  bind:this={element}
  class:wide
  aria-label={title}
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <h2>{title}</h2>
  <div class="content">{@render children()}</div>
  <div class="footer">{@render footer()}</div>
</dialog>

<style>
  dialog {
    width: 460px;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 48px);
    padding: 0;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }

  dialog[open] {
    display: flex;
    flex-direction: column;
  }

  dialog.wide {
    width: 620px;
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 35%);
  }

  h2 {
    margin: 0;
    padding: 16px 20px 8px;
    font-size: 15px;
  }

  .content {
    flex: 1;
    min-height: 0;
    padding: 8px 20px;
    overflow: auto;
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px 16px;
  }
</style>
