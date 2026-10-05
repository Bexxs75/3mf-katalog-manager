import { FilamentStatusSymbol } from './FilamentStatusSymbol';
import type { ModelFile, FilamentCheck } from '../types';
import type { LastPrinter } from '../lib/api/lastPrinter';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatRelativeTime, formatWeightG } from '../i18n/format';
import { roundG } from '../lib/filamentCheck';
import { Icon } from './Icon';

export function StatusBadges({ model, filament, lastPrinter }: {
  model: ModelFile; filament?: FilamentCheck | null; lastPrinter?: LastPrinter | null;
}) {
  const t = useT();
  const { language } = useLanguage();
  const printed = model.printStatus === 'printed';
  const status = filament?.status;
  const filamentText = status === 'ok' ? t('statusFilamentOk') : status === 'swap' ? t('statusFilamentSwap')
    : status === 'short' ? t('statusFilamentShort').replace('{g}', formatWeightG(roundG(filament!.needs.reduce((sum, need) => sum + need.missingG, 0)), language))
    : t('statusFilamentUnknown');
  const color = model.materials.find(material => material.displayColor)?.displayColor;
  return <div role="list" aria-label={t('statusBadgesLabel')} className="status-badges">
    <span role="listitem" className={`status-badge ${printed ? 'status-good' : ''}`}><Icon name={printed ? 'check' : 'close'} size={14} />{t(printed ? 'printedBadge' : 'notPrintedLabel')}</span>
    {model.materials.length > 0 && <span role="listitem" className="status-badge">
      {color ? <span aria-hidden="true" className="status-material-dot" style={{ backgroundColor: color }} /> : <Icon name="spool" size={14} />}
      {model.materials.map(material => material.name).join(', ')}
    </span>}
    {status && status !== 'no_data' && <span role="listitem" className={`status-badge ${status === 'ok' ? 'status-good' : status === 'swap' ? 'status-warn' : status === 'short' ? 'status-crit' : ''}`}><span aria-hidden="true"><FilamentStatusSymbol status={status} /></span>{filamentText}</span>}
    {model.favorite && <span role="listitem" className="status-badge status-accent"><Icon name="favorite" size={14} />{t('statusFavorite')}</span>}
    {model.queuePosition !== null && <span role="listitem" className="status-badge status-warn"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true" focusable="false"><path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /></svg>{t('statusInQueue')}</span>}
    {lastPrinter && <span role="listitem" className="status-badge"><Icon name="printer" size={14} />{t('statusLastPrinter').replace('{printer}', () => lastPrinter.printerName).replace('{time}', () => formatRelativeTime(new Date(lastPrinter.endedAt * 1000).toISOString(), language))}</span>}
  </div>;
}
