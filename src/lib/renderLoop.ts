// OrbitControls may briefly report no movement while damping crosses its EPS
// threshold. Keep drawing through those gaps before returning to idle.
export const RENDER_TAIL_FRAMES = 30;

export function createRenderLoop({ update, render }: {
  update: () => boolean;
  render: () => void;
}): { invalidate(): void; stop(): void } {
  let invalidated = false;
  let remaining = 0;
  let stopped = false;

  // Preserve the initial canvas frame; subsequent frames are demand-driven.
  render();
  const animate = () => {
    if (stopped) return;
    const moved = update();
    if (stopped) return;
    // update() itself can dispatch a change event, so read invalidated afterwards.
    const changed = moved || invalidated;
    invalidated = false;
    if (changed) remaining = RENDER_TAIL_FRAMES;
    if (changed || remaining > 0) {
      if (!changed) remaining--;
      render();
    }
    // Keep inexpensive control updates running, including auto-rotation at rest.
    if (!stopped) frame = requestAnimationFrame(animate);
  };
  let frame = requestAnimationFrame(animate);
  return {
    invalidate() { if (!stopped) invalidated = true; },
    stop() {
      if (stopped) return;
      stopped = true;
      cancelAnimationFrame(frame);
    },
  };
}
