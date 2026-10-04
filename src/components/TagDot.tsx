/** Colored dot of a tag; the hue is stored per tag and shown the same everywhere. */
export function TagDot({ hue }: { hue: number | undefined }) {
  if (hue === undefined) return null;
  return (
    <span
      aria-hidden="true"
      className="w-[6px] h-[6px] rounded-full flex-none"
      style={{ background: `oklch(0.62 0.14 ${hue})` }}
    />
  );
}
