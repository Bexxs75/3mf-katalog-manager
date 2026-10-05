import { icons } from './icons.generated';

export type IconName =
  | 'ams' | 'archive' | 'calibrate' | 'catalog' | 'check' | 'close'
  | 'collapse-all' | 'expand-all' | 'export' | 'favorite' | 'filter' | 'folder' | 'fullscreen' | 'help' | 'import'
  | 'info' | 'layers' | 'model' | 'next' | 'nozzle' | 'plate' | 'previous'
  | 'panel' | 'printer' | 'reset-view' | 'resin' | 'restore' | 'search' | 'settings'
  | 'slicer' | 'sort' | 'spool' | 'supports' | 'tag' | 'temperature' | 'trash' | 'zoom';

export function Icon({ name, size = 24, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={className}
      // Only the versioned, approved SVG sources supply this markup.
      dangerouslySetInnerHTML={{ __html: icons[name] }}
    />
  );
}
