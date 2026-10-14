/**
 * Fits a menu vertically using viewport coordinates. The gap is shared by both
 * directions so flipping does not move the menu against its anchor button.
 */
export function popoverFit(top: number, anchorTop: number, height: number, viewportHeight: number,
  margin = 8, minimumHeight = 176, maxHeight = Infinity, gap = 0): { top: number; maxHeight: number } {
  const edge = Math.min(margin, Math.max(0, viewportHeight / 2));
  const end = Math.max(edge, viewportHeight - edge);
  const belowTop = Math.max(edge, Math.min(top, end));
  const below = end - belowTop;
  const aboveEnd = Math.max(edge, Math.min(anchorTop - gap, end));
  const above = aboveEnd - edge;
  const cap = Math.max(0, maxHeight);
  const wanted = Math.min(Math.max(0, height), cap);
  if (below < wanted && below < minimumHeight && above > below) {
    const limit = Math.min(above, cap);
    return { top: aboveEnd - Math.min(wanted, limit), maxHeight: limit };
  }
  return { top: belowTop, maxHeight: Math.min(below, cap) };
}
