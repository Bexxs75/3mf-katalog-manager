/**
 * Horizontal offset (px) that moves a box with the given natural left edge and
 * width back into the viewport. All inputs use viewport coordinates. The right
 * edge wins over the left one only if the box fits; otherwise the left edge
 * stays visible, so the start of oversized content remains accessible.
 */
export function popoverShift(left: number, width: number, viewportWidth: number, margin: number): number {
  const overflowRight = left + width - (viewportWidth - margin);
  const shifted = overflowRight > 0 ? -overflowRight : 0;
  const underflowLeft = margin - (left + shifted);
  return underflowLeft > 0 ? shifted + underflowLeft : shifted;
}
