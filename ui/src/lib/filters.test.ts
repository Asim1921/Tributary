import { describe, expect, it } from 'vitest';

import { filterCounts, filterRows } from './filters';
import type { DownloadRow, DownloadState } from './types';

function row(id: number, state: DownloadState): DownloadRow {
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
    speed: 0,
    segments: [],
  };
}

const rows = [row(1, 'running'), row(2, 'paused'), row(3, 'failed'), row(4, 'completed')];

describe('filterRows', () => {
  it('sorts downloads into their groups', () => {
    const ids = (filter: Parameters<typeof filterRows>[1]) =>
      filterRows(rows, filter).map((item) => item.id);

    expect(ids('all')).toEqual([1, 2, 3, 4]);
    expect(ids('active')).toEqual([1]);
    expect(ids('unfinished')).toEqual([1, 2, 3]);
    expect(ids('finished')).toEqual([4]);
  });
});

describe('filterCounts', () => {
  it('counts each group', () => {
    expect(filterCounts(rows).map((filter) => filter.count)).toEqual([4, 1, 3, 1]);
  });
});
