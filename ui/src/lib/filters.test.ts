import { describe, expect, it } from 'vitest';

import { categoryEntries, filterKey, filterRows, queueEntries, statusEntries, type Filter } from './filters';
import type { Category, DownloadRow, DownloadState, Queue } from './types';

function row(id: number, state: DownloadState, changes: Partial<DownloadRow> = {}): DownloadRow {
  return {
    id,
    url: '',
    fileName: null,
    destDir: '',
    finalPath: null,
    size: null,
    downloaded: 0,
    state,
    error: null,
    description: null,
    resumable: false,
    addedAt: 0,
    completedAt: null,
    queueId: null,
    category: null,
    speed: 0,
    segments: [],
    ...changes,
  };
}

const rows = [
  row(1, 'running', { category: 'Video' }),
  row(2, 'paused', { queueId: 1 }),
  row(3, 'failed', { queueId: 1, category: 'Video' }),
  row(4, 'completed', { category: 'Music' }),
];

const ids = (filter: Filter) => filterRows(rows, filter).map((item) => item.id);

describe('filterRows', () => {
  it('sorts downloads into status groups', () => {
    expect(ids({ kind: 'status', status: 'all' })).toEqual([1, 2, 3, 4]);
    expect(ids({ kind: 'status', status: 'active' })).toEqual([1]);
    expect(ids({ kind: 'status', status: 'unfinished' })).toEqual([1, 2, 3]);
    expect(ids({ kind: 'status', status: 'finished' })).toEqual([4]);
  });

  it('filters by category and queue', () => {
    expect(ids({ kind: 'category', name: 'Video' })).toEqual([1, 3]);
    expect(ids({ kind: 'queue', id: 1 })).toEqual([2, 3]);
    expect(ids({ kind: 'queue', id: 9 })).toEqual([]);
  });
});

describe('sidebar entries', () => {
  it('counts each status', () => {
    expect(statusEntries(rows).map((filter) => filter.count)).toEqual([4, 1, 3, 1]);
  });

  it('lists categories and queues with counts', () => {
    const categories = [{ name: 'Video' }, { name: 'Music' }] as Category[];
    expect(categoryEntries(rows, categories).map((item) => item.count)).toEqual([2, 1]);

    const queues = [{ id: 1, name: 'Main queue' }] as Queue[];
    expect(queueEntries(rows, queues)[0]?.label).toBe('Main queue');
    expect(queueEntries(rows, queues)[0]?.count).toBe(2);
  });

  it('gives every filter a distinct key', () => {
    const keys = [
      filterKey({ kind: 'status', status: 'all' }),
      filterKey({ kind: 'category', name: 'all' }),
      filterKey({ kind: 'queue', id: 1 }),
    ];
    expect(new Set(keys).size).toBe(3);
  });
});
