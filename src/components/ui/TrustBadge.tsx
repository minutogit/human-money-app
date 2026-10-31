import React, { useState, useRef, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
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
  const [coords, setCoords] = useState<{ top: number; left: number; isAbove: boolean }>({
    top: 0,
    left: 12,
    isAbove: false,
  });
  const badgeRef = useRef<HTMLSpanElement>(null);
  const closeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const updateCoords = useCallback(() => {
    if (badgeRef.current) {
      const rect = badgeRef.current.getBoundingClientRect();
      const isAbove = rect.top > 220;
      const innerWidth = typeof window !== 'undefined' ? window.innerWidth : 1024;
      const left = Math.max(12, Math.min(rect.left, innerWidth - 300));
      const top = isAbove ? rect.top - 8 : rect.bottom + 8;
      setCoords({ top, left, isAbove });
    }
  }, []);

  useEffect(() => {
    if (!showPopover) return;
    updateCoords();
    window.addEventListener('resize', updateCoords);
    window.addEventListener('scroll', updateCoords, true);
    return () => {
      window.removeEventListener('resize', updateCoords);
      window.removeEventListener('scroll', updateCoords, true);
    };
  }, [showPopover, updateCoords]);

  useEffect(() => {
    return () => {
      if (closeTimeoutRef.current) {
        clearTimeout(closeTimeoutRef.current);
      }
    };
  }, []);

  const handleMouseEnter = () => {
    if (closeTimeoutRef.current) {
      clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }
    updateCoords();
    setShowPopover(true);
  };

  const handleMouseLeave = () => {
    if (closeTimeoutRef.current) {
      clearTimeout(closeTimeoutRef.current);
    }
    closeTimeoutRef.current = setTimeout(() => {
      setShowPopover(false);
    }, 120);
  };

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (closeTimeoutRef.current) {
      clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }
    if (!showPopover) {
      updateCoords();
      setShowPopover(true);
    } else {
      setShowPopover(false);
    }
  };

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
        ref={badgeRef}
        data-testid="trust-badge"
        onClick={handleClick}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        className={`inline-flex items-center font-bold rounded-full border select-none transition-colors cursor-pointer ${sizeClasses} ${colorClasses} ${className}`}
        role="button"
        tabIndex={0}
      >
        <Icon size={iconSize} className="shrink-0" />
        <span className="truncate">{label}</span>
      </span>

      {showPopover && createPortal(
        <div
          data-testid="trust-badge-popover"
          onClick={(e) => e.stopPropagation()}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
          style={{
            position: 'fixed',
            top: `${coords.top}px`,
            left: `${coords.left}px`,
            transform: coords.isAbove ? 'translateY(-100%)' : undefined,
          }}
          className="z-[9999] w-72 p-3 bg-slate-900/95 backdrop-blur-md text-white rounded-2xl shadow-2xl border border-white/20 text-xs space-y-2 pointer-events-auto"
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
        </div>,
        document.body
      )}
    </div>
  );
};
