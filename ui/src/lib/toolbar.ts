/** Inner SVG for each icon, drawn on a 24 by 24 grid with a stroke. */
export const ICONS = {
  add: '<path d="M12 5v14M5 12h14"/>',
  resume: '<path d="M8 5.5v13l10-6.5z"/>',
  stop: '<path d="M8 5v14M16 5v14"/>',
  stopAll: '<path d="M6 6h12v12H6z"/>',
  delete: '<path d="M5 7h14M10 7V4.5h4V7M7 7l1 13h8l1-13M10.5 11v5M13.5 11v5"/>',
  deleteCompleted: '<path d="M5 12.5l4.5 4.5L19 7.5"/><path d="M5 20h14"/>',
  options:
    '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  folder: '<path d="M3.5 18.5v-13h6l2 2.5h9v10.5z"/>',
  scheduler: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7v5.5l3.5 2"/>',
  startQueue: '<path d="M4 6h7M4 12h7M4 18h7"/><path d="M14 7.5v9l6-4.5z"/>',
  stopQueue: '<path d="M4 6h7M4 12h7M4 18h7"/><path d="M14.5 8h5v8h-5z"/>',
  grabber: '<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5M8.5 11h5M11 8.5v5"/>',
} as const;

export type IconName = keyof typeof ICONS;

export type ToolbarAction =
  | 'add'
  | 'resume'
  | 'stop'
  | 'stopAll'
  | 'delete'
  | 'deleteCompleted'
  | 'scheduler'
  | 'startQueue'
  | 'stopQueue'
  | 'grabber'
  | 'options';

export interface ToolbarButton {
  id: ToolbarAction;
  label: string;
  icon: IconName;
}

export const TOOLBAR_BUTTONS: ToolbarButton[] = [
  { id: 'add', label: 'Add URL', icon: 'add' },
  { id: 'resume', label: 'Resume', icon: 'resume' },
  { id: 'stop', label: 'Stop', icon: 'stop' },
  { id: 'stopAll', label: 'Stop all', icon: 'stopAll' },
  { id: 'delete', label: 'Delete', icon: 'delete' },
  { id: 'deleteCompleted', label: 'Clear finished', icon: 'deleteCompleted' },
  { id: 'scheduler', label: 'Scheduler', icon: 'scheduler' },
  { id: 'startQueue', label: 'Start queue', icon: 'startQueue' },
  { id: 'stopQueue', label: 'Stop queue', icon: 'stopQueue' },
  { id: 'grabber', label: 'Grabber', icon: 'grabber' },
  { id: 'options', label: 'Options', icon: 'options' },
];

const BY_ID = new Map(TOOLBAR_BUTTONS.map((button) => [button.id as string, button]));

/** Turns saved button ids into buttons. Nothing saved means all of them. */
export function resolveToolbar(saved: string[]): ToolbarButton[] {
  if (saved.length === 0) return TOOLBAR_BUTTONS;
  return saved.flatMap((id) => BY_ID.get(id) ?? []);
}

/** Moves the item at `index` up or down, keeping it within the list. */
export function moveItem<T>(items: T[], index: number, offset: -1 | 1): T[] {
  const target = index + offset;
  if (index < 0 || index >= items.length || target < 0 || target >= items.length) return items;

  const moved = [...items];
  const [item] = moved.splice(index, 1);
  moved.splice(target, 0, item as T);
  return moved;
}
