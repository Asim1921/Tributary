import { describe, expect, it } from 'vitest';

import { TOOLBAR_BUTTONS, moveItem, resolveToolbar } from './toolbar';

describe('resolveToolbar', () => {
  it('shows every button when nothing is saved', () => {
    expect(resolveToolbar([])).toEqual(TOOLBAR_BUTTONS);
  });

  it('keeps the saved choice and order', () => {
    expect(resolveToolbar(['options', 'add']).map((button) => button.id)).toEqual([
      'options',
      'add',
    ]);
  });

  it('skips ids that no longer exist', () => {
    expect(resolveToolbar(['gone', 'add']).map((button) => button.id)).toEqual(['add']);
  });
});

describe('moveItem', () => {
  it('moves an item either way', () => {
    expect(moveItem(['a', 'b', 'c'], 1, -1)).toEqual(['b', 'a', 'c']);
    expect(moveItem(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'c', 'b']);
  });

  it('does nothing at the ends', () => {
    expect(moveItem(['a', 'b'], 0, -1)).toEqual(['a', 'b']);
    expect(moveItem(['a', 'b'], 1, 1)).toEqual(['a', 'b']);
  });

  it('does not change the original list', () => {
    const items = ['a', 'b'];
    moveItem(items, 0, 1);
    expect(items).toEqual(['a', 'b']);
  });
});
