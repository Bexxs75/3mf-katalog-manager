import type { FilamentCheckStatus } from '../types';
import { Icon } from './Icon';

/** Decorative status geometry; the surrounding chip supplies the accessible status. */
export function FilamentStatusSymbol({ status }: { status: FilamentCheckStatus }) {
  if (status === 'unknown') return <span aria-hidden="true">?</span>;
  return <Icon name={status === 'ok' ? 'check' : status === 'swap' ? 'swap' : status === 'short' ? 'close' : 'minus'} size={14} />;
}
