<script lang="ts">
  import { columnById, MIN_COLUMN_WIDTH, type SortOrder } from '../lib/columns';
  import { percent } from '../lib/format';
  import type { ColumnSetting, DownloadRow } from '../lib/types';

  interface Props {
    rows: DownloadRow[];
    columns: ColumnSetting[];
    order: SortOrder;
    selected: Set<number>;
    onsort: (column: string) => void;
    onrowclick: (id: number, event: MouseEvent) => void;
    onrowopen: (id: number) => void;
    onrowdrag: (id: number) => void;
    onrowmenu: (id: number, event: MouseEvent) => void;
    onresize: (column: string, width: number) => void;
    onresized: () => void;
    onclear: () => void;
  }

  let {
    rows,
    columns,
    order,
    selected,
    onsort,
    onrowclick,
    onrowopen,
    onrowdrag,
    onrowmenu,
    onresize,
    onresized,
    onclear,
  }: Props = $props();

  const visible = $derived(columns.filter((column) => column.visible));
  const totalWidth = $derived(visible.reduce((sum, column) => sum + column.width, 0));

  function startResize(event: PointerEvent, column: ColumnSetting) {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startWidth = column.width;

    handle.setPointerCapture(event.pointerId);
    const move = (moved: PointerEvent) => {
      // Positions are fractional on a scaled display, but widths are saved as whole numbers.
      const width = Math.round(startWidth + moved.clientX - startX);
      onresize(column.id, Math.max(MIN_COLUMN_WIDTH, width));
    };
    const finish = () => {
      handle.removeEventListener('pointermove', move);
      onresized();
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', finish, { once: true });
    handle.addEventListener('pointercancel', finish, { once: true });
  }
</script>

<div class="scroller" role="presentation" onclick={onclear}>
  <table style:width="{totalWidth}px">
    <colgroup>
      {#each visible as column (column.id)}
        <col style:width="{column.width}px" />
      {/each}
    </colgroup>
    <thead>
      <tr>
        {#each visible as column (column.id)}
          {@const definition = columnById(column.id)}
          <th
            class:end={definition?.align === 'end'}
            aria-sort={order.column === column.id
              ? order.ascending
                ? 'ascending'
                : 'descending'
              : undefined}
          >
            <button
              type="button"
              class="sort"
              onclick={(event) => {
                event.stopPropagation();
                onsort(column.id);
              }}
            >
              {definition?.label}
              {#if order.column === column.id}
                <span aria-hidden="true">{order.ascending ? '▲' : '▼'}</span>
              {/if}
            </button>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="resize" onpointerdown={(event) => startResize(event, column)}></span>
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.id)}
        <tr
          class:selected={selected.has(row.id)}
          aria-selected={selected.has(row.id)}
          data-state={row.state}
          onclick={(event) => {
            event.stopPropagation();
            onrowclick(row.id, event);
          }}
          ondblclick={() => onrowopen(row.id)}
          draggable={row.state === 'completed'}
          ondragstart={(event) => {
            // Windows runs the drag, because a web page can't carry real files out.
            event.preventDefault();
            onrowdrag(row.id);
          }}
          oncontextmenu={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onrowmenu(row.id, event);
          }}
        >
          {#each visible as column (column.id)}
            {@const definition = columnById(column.id)}
            <td class:end={definition?.align === 'end'} title={definition?.text(row)}>
              {definition?.text(row)}
              {#if column.id === 'status' && row.state !== 'completed'}
                <span class="bar" aria-hidden="true">
                  <span class="fill" style:width="{percent(row) ?? 0}%"></span>
                </span>
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .scroller {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
  }

  table {
    min-width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
  }

  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    font-weight: 600;
    text-align: left;
  }

  th.end,
  td.end {
    text-align: right;
  }

  .sort {
    width: 100%;
    padding: 6px 10px;
    overflow: hidden;
    background: transparent;
    border: 0;
    border-radius: 0;
    font-weight: 600;
    text-align: inherit;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sort span {
    font-size: 9px;
  }

  .resize {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 7px;
    border-right: 1px solid var(--line);
    cursor: col-resize;
    touch-action: none;
  }

  td {
    position: relative;
    padding: 5px 10px 8px;
    overflow: hidden;
    border-bottom: 1px solid var(--line);
    font-variant-numeric: tabular-nums;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  tbody tr:hover {
    background: var(--hover);
  }

  tbody tr.selected {
    background: var(--selected);
  }

  tr[data-state='failed'] td {
    color: var(--danger);
  }

  .bar {
    position: absolute;
    right: 10px;
    bottom: 2px;
    left: 10px;
    height: 3px;
    overflow: hidden;
    background: var(--line);
    border-radius: 2px;
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s linear;
  }

  tr[data-state='paused'] .fill {
    background: var(--muted);
  }

  tr[data-state='failed'] .fill {
    background: var(--danger);
  }
</style>
