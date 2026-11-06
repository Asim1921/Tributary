<script lang="ts">
  import { addDroppedLink } from './lib/api';
  import { droppedLink } from './lib/format';

  let over = $state(false);

  function drop(event: DragEvent) {
    event.preventDefault();
    over = false;
    const url = droppedLink(event.dataTransfer);
    if (url) void addDroppedLink(url);
  }
</script>

<div
  class="target"
  class:over
  role="region"
  aria-label="Drop a link here"
  data-tauri-drag-region
  ondragover={(event) => {
    event.preventDefault();
    over = true;
  }}
  ondragleave={() => (over = false)}
  ondrop={drop}
>
  <svg viewBox="0 0 24 24" width="40" height="40" aria-hidden="true" data-tauri-drag-region>
    <path
      d="M12 4v11M7 11l5 5 5-5M6 20h12"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
</div>

<style>
  .target {
    display: grid;
    place-items: center;
    height: 100vh;
    color: #fff;
    background: var(--accent);
    cursor: move;
  }

  .target.over {
    filter: brightness(1.25);
  }
</style>
