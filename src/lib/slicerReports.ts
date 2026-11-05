/** Reports of sliced files that a slicer sent to the program (see `katalog_core::slicer_report`). */

export interface SlicerCommand {
  slicer: string;
  /** `null` when the program path cannot be written as a command line. */
  commandLine: string | null;
}

export interface SlicerReportSetup {
  enabled: boolean;
  programPath: string | null;
  /** The command lines were made for another program path than the running one. */
  pathChanged: boolean;
  commands: SlicerCommand[];
  openCount: number;
  lastError: string | null;
}

export interface ReportModel {
  fileId: string;
  fileName: string;
  /** Suggestions only: the normalized names are equal. */
  sure: boolean;
}

export type SlicerReportState = 'open' | 'linked' | 'dismissed';

export interface SlicerReport {
  id: string;
  reportedAt: string;
  reportedPath: string;
  fileName: string;
  fileSizeBytes: number;
  slicer: string | null;
  printSeconds: number | null;
  weightG: number | null;
  filamentTypes: string[];
  state: SlicerReportState;
  isTest: boolean;
  suggestion: ReportModel | null;
  model: ReportModel | null;
  decidedAt: string | null;
}

const str = (v: unknown): string | null => (typeof v === 'string' ? v : null);
const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

function parseModel(raw: unknown): ReportModel | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const { fileId, fileName, sure } = raw as Record<string, unknown>;
  const id = str(fileId);
  const name = str(fileName);
  return id !== null && name !== null ? { fileId: id, fileName: name, sure: sure === true } : null;
}

/** Reads the setup answer; a damaged answer counts as "off, nothing to show". */
export function parseSlicerReportSetup(raw: unknown): SlicerReportSetup {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const commands = Array.isArray(o.commands)
    ? o.commands.flatMap((c): SlicerCommand[] => {
        const c2 = (typeof c === 'object' && c !== null ? c : {}) as Record<string, unknown>;
        const slicer = str(c2.slicer);
        return slicer === null ? [] : [{ slicer, commandLine: str(c2.commandLine) }];
      })
    : [];
  return {
    enabled: o.enabled === true,
    programPath: str(o.programPath),
    pathChanged: o.pathChanged === true,
    commands,
    openCount: num(o.openCount) ?? 0,
    lastError: str(o.lastError),
  };
}

export function parseSlicerReports(raw: unknown): SlicerReport[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item): SlicerReport[] => {
    if (typeof item !== 'object' || item === null) return [];
    const o = item as Record<string, unknown>;
    const id = str(o.id);
    const fileName = str(o.fileName);
    const reportedPath = str(o.reportedPath);
    const state = o.state;
    if (id === null || fileName === null || reportedPath === null) return [];
    if (state !== 'open' && state !== 'linked' && state !== 'dismissed') return [];
    return [{
      id,
      reportedAt: str(o.reportedAt) ?? '',
      reportedPath,
      fileName,
      fileSizeBytes: num(o.fileSizeBytes) ?? 0,
      slicer: str(o.slicer),
      printSeconds: num(o.printSeconds),
      weightG: num(o.weightG),
      filamentTypes: Array.isArray(o.filamentTypes) ? o.filamentTypes.filter((t): t is string => typeof t === 'string') : [],
      state,
      isTest: o.isTest === true,
      suggestion: parseModel(o.suggestion),
      model: parseModel(o.model),
      decidedAt: str(o.decidedAt),
    }];
  });
}

/** Only models can be chosen: a G-code or resin entry of the catalog is no model. */
export function isChoosableModel(fileType: string | undefined): boolean {
  return fileType !== 'gcode' && fileType !== 'resin';
}
