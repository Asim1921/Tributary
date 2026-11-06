export const DAY_NAMES = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

/** Turns minutes after midnight into the text a time input uses. */
export function minutesToTime(minutes: number | null): string {
  if (minutes === null) return '';
  const hours = String(Math.floor(minutes / 60)).padStart(2, '0');
  const rest = String(minutes % 60).padStart(2, '0');
  return `${hours}:${rest}`;
}

/** Reads a time input. An empty or broken value means no time is set. */
export function timeToMinutes(text: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!match) return null;

  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  return hours < 24 && minutes < 60 ? hours * 60 + minutes : null;
}

export function dayEnabled(days: number, index: number): boolean {
  return (days & (1 << index)) !== 0;
}

export function toggleDay(days: number, index: number): number {
  return days ^ (1 << index);
}
