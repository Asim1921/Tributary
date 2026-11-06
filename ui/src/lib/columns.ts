import { displayName, formatBytes, formatDate, formatSpeed, formatTimeLeft, percent, secondsLeft, statusText } from './format';
import type { ColumnSetting, DownloadRow } from './types';

export interface ColumnDefinition {
  id: string;
  label: string;
  width: number;
  visible: boolean;
  align?: 'end';
  text: (row: DownloadRow) => string;
  /** What the column sorts by. Null values always sort last. */
  sortValue: (row: DownloadRow) => string | number | null;
}

export const COLUMNS: ColumnDefinition[] = [
  {
    id: 'name',
    label: 'File name',
    width: 300,
    visible: true,
    text: displayName,
    sortValue: (row) => displayName(row).toLowerCase(),
  },
  {
    id: 'size',
    label: 'Size',
    width: 90,
    visible: true,
    align: 'end',
    text: (row) => (row.size === null ? '' : formatBytes(row.size)),
    sortValue: (row) => row.size,
  },
  {
    id: 'status',
    label: 'Status',
    width: 150,
    visible: true,
    text: statusText,
    sortValue: (row) => percent(row),
  },
  {
    id: 'timeLeft',
    label: 'Time left',
    width: 90,
    visible: true,
    align: 'end',
    text: formatTimeLeft,
    sortValue: secondsLeft,
  },
  {
    id: 'speed',
    label: 'Speed',
    width: 100,
    visible: true,
    align: 'end',
    text: (row) => (row.state === 'running' ? formatSpeed(row.speed) : ''),
    sortValue: (row) => (row.state === 'running' ? row.speed : null),
  },
  {
    id: 'added',
    label: 'Added',
    width: 150,
    visible: true,
    text: (row) => formatDate(row.addedAt),
    sortValue: (row) => row.addedAt,
  },
  {
    id: 'category',
    label: 'Category',
    width: 110,
    visible: false,
    text: (row) => row.category ?? '',
    sortValue: (row) => row.category?.toLowerCase() ?? null,
  },
  {
    id: 'description',
    label: 'Description',
    width: 200,
    visible: false,
    text: (row) => row.description ?? '',
    sortValue: (row) => row.description?.toLowerCase() ?? null,
  },
  {
    id: 'folder',
    label: 'Folder',
    width: 220,
    visible: false,
    text: (row) => row.destDir,
    sortValue: (row) => row.destDir.toLowerCase(),
  },
];

const BY_ID = new Map(COLUMNS.map((column) => [column.id, column]));

export function columnById(id: string): ColumnDefinition | undefined {
  return BY_ID.get(id);
}

export const MIN_COLUMN_WIDTH = 50;

/**
 * Merges saved column choices with the known columns. Saved order wins, columns
 * that no longer exist are dropped, and new columns join at the end.
 */
export function resolveColumns(saved: ColumnSetting[]): ColumnSetting[] {
  const known = saved.filter((column) => BY_ID.has(column.id));
  const seen = new Set(known.map((column) => column.id));
  const added = COLUMNS.filter((column) => !seen.has(column.id)).map(({ id, visible, width }) => ({
    id,
    visible,
    width,
  }));
  return [...known, ...added];
}

export interface SortOrder {
  column: string;
  ascending: boolean;
}

export function sortRows(rows: DownloadRow[], order: SortOrder): DownloadRow[] {
  const definition = BY_ID.get(order.column);
  if (!definition) return rows;

  const direction = order.ascending ? 1 : -1;
  return [...rows].sort((first, second) => {
    const a = definition.sortValue(first);
    const b = definition.sortValue(second);

    if (a === null && b === null) return first.id - second.id;
    if (a === null) return 1;
    if (b === null) return -1;

    const compared =
      typeof a === 'number' && typeof b === 'number' ? a - b : String(a).localeCompare(String(b));
    return compared === 0 ? first.id - second.id : compared * direction;
  });
}
