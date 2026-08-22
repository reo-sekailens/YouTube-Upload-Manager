export const comparisonPositionMaximum = 86_400;

export function clampComparisonPosition(seconds: number): number {
  return Math.min(comparisonPositionMaximum, Math.max(0, seconds));
}

export function moveComparisonPosition(position: number, seconds: number): number {
  return clampComparisonPosition(position + seconds);
}
