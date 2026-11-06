import type { Category, DownloadRow, Queue } from './types';

export type StatusFilter = 'all' | 'active' | 'unfinished' | 'finished';

export type Filter =
  | { kind: 'status'; status: StatusFilter }
  | { kind: 'category'; name: string }
  | { kind: 'queue'; id: number };

export interface FilterEntry {
  key: string;
  label: string;
  count: number;
  filter: Filter;
}

export const ALL: Filter = { kind: 'status', status: 'all' };

const STATUS_LABELS: Record<StatusFilter, string> = {
  all: 'All downloads',
  active: 'Downloading',
  unfinished: 'Unfinished',
  finished: 'Finished',
};

const STATUS_MATCHES: Record<StatusFilter, (row: DownloadRow) => boolean> = {
  all: () => true,
  active: (row) => row.state === 'running',
  unfinished: (row) => row.state !== 'completed',
  finished: (row) => row.state === 'completed',
};

export function filterKey(filter: Filter): string {
  switch (filter.kind) {
    case 'status':
      return `status:${filter.status}`;
    case 'category':
      return `category:${filter.name}`;
    case 'queue':
      return `queue:${filter.id}`;
  }
}

export function filterRows(rows: DownloadRow[], filter: Filter): DownloadRow[] {
  switch (filter.kind) {
    case 'status':
      return rows.filter(STATUS_MATCHES[filter.status]);
    case 'category':
      return rows.filter((row) => row.category === filter.name);
    case 'queue':
      return rows.filter((row) => row.queueId === filter.id);
  }
}

function entry(rows: DownloadRow[], label: string, filter: Filter): FilterEntry {
  return { key: filterKey(filter), label, count: filterRows(rows, filter).length, filter };
}

export function statusEntries(rows: DownloadRow[]): FilterEntry[] {
  return (Object.keys(STATUS_LABELS) as StatusFilter[]).map((status) =>
    entry(rows, STATUS_LABELS[status], { kind: 'status', status }),
  );
}

export function categoryEntries(rows: DownloadRow[], categories: Category[]): FilterEntry[] {
  return categories.map(({ name }) => entry(rows, name, { kind: 'category', name }));
}

export function queueEntries(rows: DownloadRow[], queues: Queue[]): FilterEntry[] {
  return queues.map(({ id, name }) => entry(rows, name, { kind: 'queue', id }));
}
