import { isReadOnly } from '../lib/readOnly';

/** The build target is immutable; no subscription or provider is required. */
export const useReadOnly = isReadOnly;
