import React from 'react';
import { useTranslation } from 'react-i18next';
import { Shield, ShieldCheck, ShieldAlert } from 'lucide-react';
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
        ? ` (+${trust.additionalPathsCount})`
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
    <span
      data-testid="trust-badge"
      className={`inline-flex items-center font-bold rounded-full border select-none transition-colors ${sizeClasses} ${colorClasses} ${className}`}
    >
      <Icon size={iconSize} className="shrink-0" />
      <span className="truncate">{label}</span>
    </span>
  );
};
