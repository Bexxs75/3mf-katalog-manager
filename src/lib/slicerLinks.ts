/** Slicers that accept a download URL through a custom link scheme. */
export interface WebSlicer {
  id: string;
  name: string;
  /** Everything before the percent-encoded download URL. */
  prefix: string;
  /** Catalog file types (`stp` covers both .stp and .step) the slicer can open. */
  types: readonly string[];
}

const ALL_TYPES = ['3mf', 'stl', 'obj', 'stp'] as const;

export const WEB_SLICERS: readonly WebSlicer[] = [
  { id: 'orca', name: 'OrcaSlicer', prefix: 'orcaslicer://open?file=', types: ALL_TYPES },
  // Bambu Studio imports only 3MF through this link; STL does not open.
  { id: 'bambu', name: 'Bambu Studio', prefix: 'bambustudio://open?file=', types: ['3mf'] },
  { id: 'creality', name: 'Creality Print', prefix: 'crealityprintlink://open?file=', types: ALL_TYPES },
  { id: 'elegoo', name: 'ElegooSlicer', prefix: 'elegooslicer://open?file=', types: ALL_TYPES },
  { id: 'cura', name: 'Cura', prefix: 'cura://open?file=', types: ['3mf', 'stl', 'obj'] },
  { id: 'lychee', name: 'Lychee', prefix: 'lycheeslicer://open/', types: ['3mf', 'stl', 'obj'] },
  { id: 'prusa', name: 'SuperSlicer', prefix: 'prusaslicer://open?file=', types: ALL_TYPES },
];

export const slicersFor = (fileType: string): WebSlicer[] =>
  WEB_SLICERS.filter(slicer => slicer.types.includes(fileType.toLowerCase()));

/** The slicer fetches the file itself, so the link must name the address the user's browser used. */
export const buildSlicerUrl = (slicer: WebSlicer, origin: string, path: string): string =>
  slicer.prefix + encodeURIComponent(origin + path);

/** Catalog file type derived from a file name; `.step` and `.stp` share one type. */
export const fileTypeOfName = (name: string): string | null => {
  const extension = /\.([A-Za-z0-9]+)$/.exec(name)?.[1]?.toLowerCase();
  if (extension === 'step') return 'stp';
  return extension && ALL_TYPES.includes(extension as (typeof ALL_TYPES)[number]) ? extension : null;
};
