import type { DownloadRow } from './types';

export type Filter = 'all' | 'active' | 'unfinished' | 'finished';

const LABELS: Record<Filter, string> = {
  all: 'All downloads',
  active: 'Downloading',
  unfinished: 'Unfinished',
  finished: 'Finished',
};

const MATCHES: Record<Filter, (row: DownloadRow) => boolean> = {
  all: () => true,
  active: (row) => row.state === 'running',
  unfinished: (row) => row.state !== 'completed',
  finished: (row) => row.state === 'completed',
};

export function filterRows(rows: DownloadRow[], filter: Filter): DownloadRow[] {
  return rows.filter(MATCHES[filter]);
}

export function filterCounts(rows: DownloadRow[]) {
  return (Object.keys(LABELS) as Filter[]).map((id) => ({
    id,
    label: LABELS[id],
    count: filterRows(rows, id).length,
  }));
}
