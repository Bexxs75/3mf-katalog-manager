import { useEffect, useState } from 'react';

interface Position { x: number; y: number }

export function DragGhost({ name, image, initialPosition = null }: { name: string; image?: string | null; initialPosition?: Position | null }) {
  const [position, setPosition] = useState<Position | null>(initialPosition);
  useEffect(() => {
    const move = (event: MouseEvent) => setPosition({ x: event.clientX, y: event.clientY });
    window.addEventListener('mousemove', move);
    return () => window.removeEventListener('mousemove', move);
  }, []);
  return (
    <div aria-hidden="true"
      style={{ position: 'fixed', pointerEvents: 'none', left: position ? position.x + 10 : 0, top: position ? position.y + 10 : 0, visibility: position ? 'visible' : 'hidden' }}
      className="z-50 flex items-center gap-2 max-w-64 p-2 rounded-[6px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)] -rotate-2 motion-reduce:rotate-0 motion-reduce:transition-none">
      {image && <img src={image} alt="" className="w-10 h-10 object-cover rounded" />}
      <span className="truncate text-small font-medium">{name}</span>
    </div>
  );
}
