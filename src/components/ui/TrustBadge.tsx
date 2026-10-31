import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Shield, ShieldCheck, ShieldAlert, ArrowRight } from 'lucide-react';
import { VoucherTrustDto } from '../../types';

export interface TrustBadgeProps {
  trust?: VoucherTrustDto | null;
  size?: 'sm' | 'md';
  className?: string;
}

export const TrustBadge: React.FC<TrustBadgeProps> = ({
  trust,
  size = 'md',
  className = '',
}) => {
  const { t } = useTranslation();
  const [showPopover, setShowPopover] = useState(false);

  if (!trust) {
    return null;
  }

  let colorClasses = '';
  let Icon = Shield;
  let label = '';

  if (trust.blockedByOffender) {
    colorClasses = 'bg-rose-100 text-rose-800 border-rose-200';
    Icon = ShieldAlert;
    label = t('voucher.trustOffender');
  } else if (trust.isOwnVoucher) {
    colorClasses = 'bg-sky-50 text-sky-700 border-sky-200';
    Icon = ShieldCheck;
    label = t('voucher.trustOwn');
  } else if (trust.trustPercent === 0) {
    colorClasses = 'bg-slate-100 text-slate-600 border-slate-200';
    Icon = Shield;
    label = t('voucher.trustUnknown');
  } else {
    // Score colors when trustPercent > 0
    if (trust.trustPercent >= 60) {
      colorClasses = 'bg-emerald-50 text-emerald-800 border-emerald-200';
      Icon = ShieldCheck;
    } else if (trust.trustPercent >= 25) {
      colorClasses = 'bg-amber-50 text-amber-800 border-amber-200';
      Icon = Shield;
    } else {
      colorClasses = 'bg-slate-100 text-slate-600 border-slate-200';
      Icon = Shield;
    }

    const extra =
      trust.additionalPathsCount && trust.additionalPathsCount > 0
        ? ` ${t('voucher.trustAdditionalPaths', { count: trust.additionalPathsCount })}`
        : '';

    if (trust.shortestHops === 1 && !trust.viaGuarantor) {
      label = t('voucher.trustDirect', { percent: trust.trustPercent }) + extra;
    } else if (trust.viaGuarantor && trust.primaryBridgeName) {
      label =
        t('voucher.trustViaGuarantor', {
          percent: trust.trustPercent,
          name: trust.primaryBridgeName,
        }) + extra;
    } else if (trust.primaryBridgeName) {
      label =
        t('voucher.trustViaContact', {
          percent: trust.trustPercent,
          name: trust.primaryBridgeName,
        }) + extra;
    } else {
      label =
        t('voucher.trustViaNetwork', { percent: trust.trustPercent }) + extra;
    }
  }

  const sizeClasses =
    size === 'sm'
      ? 'text-[10px] px-2 py-0.5 gap-1'
      : 'text-xs px-2.5 py-1 gap-1.5';
  const iconSize = size === 'sm' ? 12 : 14;

  return (
    <div className="relative inline-flex items-center">
      <span
        data-testid="trust-badge"
        onClick={(e) => {
          e.stopPropagation();
          setShowPopover(!showPopover);
        }}
        onMouseEnter={() => setShowPopover(true)}
        onMouseLeave={() => setShowPopover(false)}
        className={`inline-flex items-center font-bold rounded-full border select-none transition-colors cursor-pointer ${sizeClasses} ${colorClasses} ${className}`}
        role="button"
        tabIndex={0}
      >
        <Icon size={iconSize} className="shrink-0" />
        <span className="truncate">{label}</span>
      </span>

      {showPopover && (
        <div
          data-testid="trust-badge-popover"
          onClick={(e) => e.stopPropagation()}
          onMouseEnter={() => setShowPopover(true)}
          onMouseLeave={() => setShowPopover(false)}
          className="absolute left-0 bottom-full mb-2 z-50 w-72 p-3 bg-slate-900/95 backdrop-blur-md text-white rounded-2xl shadow-2xl border border-white/20 text-xs space-y-2 pointer-events-auto"
        >
          <div className="font-bold border-b border-white/10 pb-1 flex items-center justify-between text-white/90">
            <span>{t('voucher.trustBadgeModalTitle')}</span>
            <span className="font-mono text-emerald-400">{trust.trustPercent}%</span>
          </div>

          <p className="text-white/80 leading-relaxed text-[11px]">
            {t('voucher.trustBadgeExplanation')}
          </p>

          {trust.blockedByOffender && (
            <div className="p-2 bg-rose-500/20 border border-rose-500/30 rounded-xl text-rose-300 text-[11px]">
              {t('voucher.trustBadgeBlockedWarning')}
            </div>
          )}

          {trust.bestPathNames && trust.bestPathNames.length > 0 && (
            <div className="pt-1 border-t border-white/10 space-y-1">
              <span className="text-[10px] uppercase font-bold text-white/50 tracking-wider">
                {t('voucher.trustPathLabel')}:
              </span>
              <div className="flex flex-wrap items-center gap-1 font-mono text-[11px] text-sky-300">
                <span>{t('voucher.trustPathYou')}</span>
                {trust.bestPathNames.map((name, i) => (
                  <React.Fragment key={i}>
                    <ArrowRight size={10} className="text-white/40" />
                    <span>{name}</span>
                  </React.Fragment>
                ))}
              </div>
            </div>
          )}

          {trust.additionalPathsCount > 0 && (
            <div className="text-[10px] text-white/60 font-medium">
              {t('voucher.trustAdditionalPathsCount', { count: trust.additionalPathsCount })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
