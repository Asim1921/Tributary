export interface ClickModifiers {
  ctrl: boolean;
  shift: boolean;
}

export interface SelectionState {
  selected: Set<number>;
  /** Where a Shift+click range starts. */
  anchor: number | null;
}

/** Works out the new selection after clicking `id` in a list shown in `order`. */
export function clickRow(
  state: SelectionState,
  order: number[],
  id: number,
  modifiers: ClickModifiers,
): SelectionState {
  if (modifiers.shift && state.anchor !== null) {
    const from = order.indexOf(state.anchor);
    const to = order.indexOf(id);
    if (from !== -1 && to !== -1) {
      const [low, high] = from < to ? [from, to] : [to, from];
      return { selected: new Set(order.slice(low, high + 1)), anchor: state.anchor };
    }
  }

  if (modifiers.ctrl) {
    const selected = new Set(state.selected);
    if (!selected.delete(id)) selected.add(id);
    return { selected, anchor: id };
  }

  return { selected: new Set([id]), anchor: id };
}

/** Right-clicking keeps a selection the row is part of, and otherwise selects that row. */
export function rightClickRow(state: SelectionState, id: number): SelectionState {
  return state.selected.has(id) ? state : { selected: new Set([id]), anchor: id };
}

/** Drops ids that have left the list. */
export function keepExisting(state: SelectionState, ids: number[]): SelectionState {
  const existing = new Set(ids);
  const selected = new Set([...state.selected].filter((id) => existing.has(id)));
  const anchor = state.anchor !== null && existing.has(state.anchor) ? state.anchor : null;
  return { selected, anchor };
}
