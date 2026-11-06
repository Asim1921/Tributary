import { describe, expect, it } from 'vitest';

import { COLUMNS, resolveColumns, sortRows } from './columns';
import type { DownloadRow } from './types';

function row(id: number, changes: Partial<DownloadRow> = {}): DownloadRow {
  return {
    id,
    url: `https://x.test/${id}.zip`,
    fileName: `${id}.zip`,
    destDir: 'C:\\Downloads',
    finalPath: null,
    size: 100,
    downloaded: 0,
    state: 'paused',
    error: null,
    description: null,
    resumable: true,
    addedAt: id,
    completedAt: null,
    queueId: null,
    category: null,
    speed: 0,
    segments: [],
    ...changes,
  };
}

const ids = (rows: DownloadRow[]) => rows.map((item) => item.id);

describe('resolveColumns', () => {
  it('uses the defaults when nothing is saved', () => {
    const columns = resolveColumns([]);
    expect(columns.map((column) => column.id)).toEqual(COLUMNS.map((column) => column.id));
  });

  it('keeps the saved order, width and visibility', () => {
    const columns = resolveColumns([{ id: 'size', visible: false, width: 77 }]);
    expect(columns[0]).toEqual({ id: 'size', visible: false, width: 77 });
  });

  it('drops columns that no longer exist and adds new ones', () => {
    const columns = resolveColumns([{ id: 'removed', visible: true, width: 10 }]);
    expect(columns.some((column) => column.id === 'removed')).toBe(false);
    expect(columns).toHaveLength(COLUMNS.length);
  });
});

describe('sortRows', () => {
  it('sorts numbers by value, not as text', () => {
    const rows = [row(1, { size: 900 }), row(2, { size: 10_000 }), row(3, { size: 5 })];
    expect(ids(sortRows(rows, { column: 'size', ascending: true }))).toEqual([3, 1, 2]);
    expect(ids(sortRows(rows, { column: 'size', ascending: false }))).toEqual([2, 1, 3]);
  });

  it('ignores case when sorting names', () => {
    const rows = [row(1, { fileName: 'banana' }), row(2, { fileName: 'Apple' })];
    expect(ids(sortRows(rows, { column: 'name', ascending: true }))).toEqual([2, 1]);
  });

  it('keeps unknown values last in either direction', () => {
    const rows = [row(1, { size: null }), row(2, { size: 5 }), row(3, { size: 9 })];
    expect(ids(sortRows(rows, { column: 'size', ascending: true }))).toEqual([2, 3, 1]);
    expect(ids(sortRows(rows, { column: 'size', ascending: false }))).toEqual([3, 2, 1]);
  });

  it('breaks ties by id, so rows do not jump around during updates', () => {
    const rows = [row(3), row(1), row(2)];
    expect(ids(sortRows(rows, { column: 'size', ascending: true }))).toEqual([1, 2, 3]);
  });

  it('leaves the original list alone', () => {
    const rows = [row(2), row(1)];
    sortRows(rows, { column: 'added', ascending: true });
    expect(ids(rows)).toEqual([2, 1]);
  });

  it('ignores an unknown column', () => {
    const rows = [row(2), row(1)];
    expect(ids(sortRows(rows, { column: 'missing', ascending: true }))).toEqual([2, 1]);
  });
});
