import { describe, expect, it } from 'vitest';

import { clickRow, keepExisting, rightClickRow, type SelectionState } from './selection';

const order = [10, 20, 30, 40];
const none: SelectionState = { selected: new Set(), anchor: null };
const plain = { ctrl: false, shift: false };
const ctrl = { ctrl: true, shift: false };
const shift = { ctrl: false, shift: true };

const ids = (state: SelectionState) => [...state.selected].sort((a, b) => a - b);

describe('clickRow', () => {
  it('selects only the clicked row', () => {
    const first = clickRow(none, order, 20, plain);
    expect(ids(clickRow(first, order, 30, plain))).toEqual([30]);
  });

  it('adds and removes rows with Ctrl', () => {
    const one = clickRow(none, order, 20, plain);
    const two = clickRow(one, order, 40, ctrl);
    expect(ids(two)).toEqual([20, 40]);
    expect(ids(clickRow(two, order, 20, ctrl))).toEqual([40]);
  });

  it('selects a range with Shift, in either direction', () => {
    const start = clickRow(none, order, 20, plain);
    expect(ids(clickRow(start, order, 40, shift))).toEqual([20, 30, 40]);
    expect(ids(clickRow(start, order, 10, shift))).toEqual([10, 20]);
  });

  it('keeps the range start through repeated Shift clicks', () => {
    const start = clickRow(none, order, 20, plain);
    const wide = clickRow(start, order, 40, shift);
    expect(ids(clickRow(wide, order, 30, shift))).toEqual([20, 30]);
  });

  it('treats Shift as a plain click when there is no start', () => {
    expect(ids(clickRow(none, order, 30, shift))).toEqual([30]);
  });

  it('copes when the range start has left the list', () => {
    const state: SelectionState = { selected: new Set([99]), anchor: 99 };
    expect(ids(clickRow(state, order, 30, shift))).toEqual([30]);
  });
});

describe('rightClickRow', () => {
  it('keeps a selection that includes the row', () => {
    const state: SelectionState = { selected: new Set([10, 20]), anchor: 10 };
    expect(rightClickRow(state, 20)).toBe(state);
  });

  it('selects the row otherwise', () => {
    const state: SelectionState = { selected: new Set([10]), anchor: 10 };
    expect(ids(rightClickRow(state, 30))).toEqual([30]);
  });
});

describe('keepExisting', () => {
  it('forgets rows that were removed', () => {
    const state: SelectionState = { selected: new Set([10, 20]), anchor: 20 };
    const kept = keepExisting(state, [10, 30]);
    expect(ids(kept)).toEqual([10]);
    expect(kept.anchor).toBeNull();
  });
});
