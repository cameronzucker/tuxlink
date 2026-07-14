export function isTipSeen(tipsSeen: readonly string[], id: string): boolean {
  return tipsSeen.includes('*') || tipsSeen.includes(id);
}

export function markTipSeen(tipsSeen: readonly string[], id: string): string[] {
  if (isTipSeen(tipsSeen, id)) return [...tipsSeen];
  return [...tipsSeen, id];
}
