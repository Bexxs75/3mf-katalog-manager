import type { Translations } from '../i18n/types';
import type { AppError } from './errors';
import errorCodes from './errorCodes.json';
import { displayName } from './displayName';

const REGISTERED_CODES: ReadonlySet<string> = new Set(errorCodes.registered);

/** The translate function of `useT`. */
type Translate = <K extends keyof Translations>(key: K) => Translations[K];

/**
 * Codes that predate the registry in `errorCodes.json`; they travel as the bare
 * `message` and keep the dictionary key they always had.
 */
const LEGACY_KEYS = {
  container: 'containerUpdates',
  containerExternalUnavailable: 'containerExternalUnavailable',
  containerSlicerUnavailable: 'containerSlicerUnavailable',
  containerFileManagerUnavailable: 'containerFileManagerUnavailable',
  companionOpenContainer: 'companionOpenContainer',
  linuxMenuForeignFile: 'linuxMenuForeignFile',
  linuxMenuThirdParty: 'linuxMenuThirdParty',
  imageUploadTooLarge: 'imageUploadTooLarge',
  imageUploadUnsupported: 'imageUploadUnsupported',
  imageUploadUnreadable: 'imageUploadUnreadable',
  importActive: 'importActiveError',
  // Server answer to a repeated write whose stored reply was too large to keep.
  request_id_result_unavailable: 'requestResultLost',
} as const satisfies Record<string, keyof Translations>;

/** Dictionary key of a registered backend code: `folderNameEmpty` becomes `errFolderNameEmpty`. */
export function registeredCodeKey(code: string): keyof Translations {
  return `err${code.charAt(0).toUpperCase()}${code.slice(1)}` as keyof Translations;
}

/** Dictionary key for a legacy code, or null when `code` is not one. */
export function legacyCodeKey(code: string): keyof Translations | null {
  return Object.prototype.hasOwnProperty.call(LEGACY_KEYS, code) ? LEGACY_KEYS[code as keyof typeof LEGACY_KEYS] : null;
}

/** Replaces `{0}`, `{1}`, ... with the params; a placeholder without a value stays visible. */
export function fillParams(text: string, params: readonly string[] = []): string {
  return text.replace(/\{(\d+)\}/g, (match, index: string) => params[Number(index)] ?? match);
}

/**
 * The text shown for an error in the chosen language. Registered and legacy codes
 * are translated (with `params` filled in); anything else is technical text from
 * an unexpected error and is shown as it is.
 */
export function formatAppError(t: Translate, error: Pick<AppError, 'message' | 'params'>): string {
  const legacy = legacyCodeKey(error.message);
  if (legacy) return String(t(legacy));
  if (REGISTERED_CODES.has(error.message)) return fillParams(String(t(registeredCodeKey(error.message))), error.params?.map(displayName));
  return error.message;
}
