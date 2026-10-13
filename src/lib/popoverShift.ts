/**
 * Horizontal offset (px) that moves a box with the given natural left edge and
 * width back into the viewport. The right edge wins over the left one only if
 * the box fits; otherwise the left edge stays visible, since nothing can
 * scroll to the left of the document.
 */
export function popoverShift(left: number, width: number, viewportWidth: number, margin: number): number {
  const overflowRight = left + width - (viewportWidth - margin);
  const shifted = overflowRight > 0 ? -overflowRight : 0;
  const underflowLeft = margin - (left + shifted);
  return underflowLeft > 0 ? shifted + underflowLeft : shifted;
}
