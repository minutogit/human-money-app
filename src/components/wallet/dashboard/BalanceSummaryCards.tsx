// src/components/wallet/dashboard/BalanceSummaryCards.tsx
import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Plus, ArrowDownLeft, Info } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Card } from '../../ui/Card';
import { InfoHint } from '../../ui/InfoHint';
import { Advanced } from '../../ui/Advanced';
import { formatAmount } from '../../../utils/format';
import { formatSignedNetBalance } from '../../../utils/walletEvents';
import type { useNavigation } from '../../../context/NavigationContext';
import type { AggregatedBalance } from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

interface BalanceSummaryCardsProps {
    isLoading: boolean;
    balances: AggregatedBalance[];
    voucherCountsByStandard: Record<string, number>;
    balanceViewMode: 'spendable' | 'net';
    onViewModeChange: (mode: 'spendable' | 'net') => void;
    onProtectedNavigate: (target: Parameters<NavigateFn>[0]) => void;
    onNavigate: NavigateFn;
}

function calculatePopoverPosition(
    rect: DOMRect,
    popoverEl?: HTMLElement | null,
): { top: number; left: number } {
    const popoverWidth = popoverEl?.offsetWidth || 256;
    const popoverHeight = popoverEl?.offsetHeight || 180;
    const margin = 12;

    let left = rect.right - popoverWidth;
    if (left < margin) left = margin;
    if (typeof window !== 'undefined' && left + popoverWidth > window.innerWidth - margin) {
        left = window.innerWidth - popoverWidth - margin;
    }

    const spaceBelow =
        typeof window !== 'undefined' ? window.innerHeight - rect.bottom - margin : 300;
    const top =
        spaceBelow < popoverHeight && rect.top > popoverHeight + margin
            ? rect.top - popoverHeight - 8
            : rect.bottom + 8;

    const clampedTop = Math.max(
        margin,
        typeof window !== 'undefined'
            ? Math.min(top, window.innerHeight - popoverHeight - margin)
            : top,
    );
    return { top: clampedTop, left };
}

export function BalanceSummaryCards({
    isLoading,
    balances,
    voucherCountsByStandard,
    balanceViewMode,
    onViewModeChange,
    onProtectedNavigate,
    onNavigate,
}: BalanceSummaryCardsProps) {
    const { t } = useTranslation();
    const [activeInfoPopover, setActiveInfoPopover] = useState<string | null>(null);
    const [popoverAnchorRect, setPopoverAnchorRect] = useState<DOMRect | null>(null);
    const popoverRef = useRef<HTMLDivElement | null>(null);
    const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        return () => {
            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
        };
    }, []);

    useEffect(() => {
        if (!activeInfoPopover) return;
        const closePopover = () => {
            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
            setActiveInfoPopover(null);
            setPopoverAnchorRect(null);
        };
        const handlePointerDown = (e: MouseEvent | TouchEvent) => {
            if ((e.target as Element)?.closest?.('[data-popover-trigger]')) return;
            if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
                closePopover();
            }
        };

        window.addEventListener('scroll', closePopover, true);
        window.addEventListener('resize', closePopover);
        document.addEventListener('pointerdown', handlePointerDown);
        return () => {
            window.removeEventListener('scroll', closePopover, true);
            window.removeEventListener('resize', closePopover);
            document.removeEventListener('pointerdown', handlePointerDown);
        };
    }, [activeInfoPopover]);

    const activeBalance =
        activeInfoPopover && typeof document !== 'undefined'
            ? balances.find((b) => b.standardUuid === activeInfoPopover)
            : null;
    const popoverPos =
        activeBalance && popoverAnchorRect
            ? calculatePopoverPosition(popoverAnchorRect, popoverRef.current)
            : null;

    return (
        <>
            <section>
                <div className="flex items-center justify-between mb-4 px-2">
                    <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">
                        {t('dashboard.yourBalances')}
                    </h2>
                    <Advanced>
                        <div className="flex items-center bg-white/40 p-0.5 rounded-xl border border-theme-subtle/50 text-xs shadow-inner-soft">
                            <button
                                type="button"
                                onClick={() => onViewModeChange('spendable')}
                                className={`px-3 py-1 rounded-lg font-bold transition-all text-[11px] ${
                                    balanceViewMode === 'spendable'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('dashboard.modeSpendable')}
                            </button>
                            <button
                                type="button"
                                onClick={() => onViewModeChange('net')}
                                className={`px-3 py-1 rounded-lg font-bold transition-all text-[11px] ${
                                    balanceViewMode === 'net'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('dashboard.modeNet')}
                            </button>
                        </div>
                    </Advanced>
                </div>

                {isLoading ? (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                        {[1, 2].map((i) => (
                            <div
                                key={i}
                                className="h-48 bg-white/50 animate-pulse rounded-3xl border border-theme-subtle/50"
                            />
                        ))}
                    </div>
                ) : balances.length === 0 ? (
                    <Card
                        variant="none"
                        className="border-2 border-dashed border-theme-subtle bg-white/30 dark:bg-gray-800/20 p-8 rounded-3xl flex flex-col items-center justify-center text-center space-y-4"
                        data-testid="dashboard-empty-balances"
                    >
                        <div className="max-w-md mx-auto space-y-2">
                            <h3 className="text-base font-bold text-theme-primary">
                                {t('dashboard.noVouchersTitle', 'Noch keine Gutscheine')}
                            </h3>
                            <p className="text-sm text-theme-secondary leading-relaxed">
                                {t(
                                    'dashboard.emptyBalanceExplanation',
                                    'Gutscheine sind Versprechen von Menschen – z. B. eine Stunde Hilfe. Erstelle deinen ersten oder lass dir einen schicken.',
                                )}
                            </p>
                            <div className="pt-2 flex justify-center">
                                <InfoHint topic="voucherBasics" />
                            </div>
                        </div>
                        <div className="flex flex-wrap justify-center gap-3 pt-2">
                            <Button
                                onClick={() =>
                                    onProtectedNavigate({
                                        view: 'create_voucher',
                                        previousView: { view: 'logged_in' },
                                    })
                                }
                                className="gap-2 shadow-sm"
                                size="sm"
                                data-testid="empty-balance-create-btn"
                            >
                                <Plus size={16} />
                                <span>{t('voucher.create')}</span>
                            </Button>
                            <Button
                                onClick={() => onNavigate({ view: 'receive_bundle' })}
                                className="gap-2 shadow-sm"
                                variant="secondary"
                                size="sm"
                                data-testid="empty-balance-receive-btn"
                            >
                                <ArrowDownLeft size={16} />
                                <span>{t('transfer.receive')}</span>
                            </Button>
                        </div>
                    </Card>
                ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                        {balances.map((balance, idx) => {
                            const count = voucherCountsByStandard[balance.standardUuid] || 0;
                            return (
                                <Card
                                    key={balance.standardUuid}
                                    variant="default"
                                    className={`relative overflow-hidden p-6 h-48 flex flex-col justify-between group ${
                                        idx % 2 === 0
                                            ? 'bg-gradient-to-br from-slate-500 to-slate-700'
                                            : 'bg-gradient-to-br from-theme-primary to-theme-accent'
                                    }`}
                                    hover
                                    onClick={() =>
                                        onNavigate({
                                            view: 'wallet',
                                            initialStandardFilter: balance.displayStandardName,
                                            initialStatusFilter: 'active',
                                        })
                                    }
                                >
                                    <div className="absolute -right-8 -top-8 w-32 h-32 bg-white/10 rounded-full blur-2xl group-hover:bg-white/20 transition-all duration-500" />
                                    <div className="absolute -left-4 -bottom-4 w-24 h-24 bg-black/5 rounded-full blur-xl" />

                                    <div className="relative z-10 flex justify-between items-start">
                                        <div className="flex flex-col">
                                            <span className="text-[10px] font-black text-white/60 uppercase tracking-[0.15em] mb-1">
                                                {balance.displayStandardName}
                                            </span>
                                            <h3 className="text-xl font-bold text-white leading-tight">
                                                {balance.displayCurrency}
                                            </h3>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <div className="px-2 py-1 bg-white/20 backdrop-blur-md rounded-lg border border-white/20 text-[10px] font-bold text-white">
                                                {t('dashboard.vouchersCount', { count })}
                                            </div>
                                            <button
                                                type="button"
                                                data-popover-trigger={balance.standardUuid}
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                                                    const rect = e.currentTarget.getBoundingClientRect();
                                                    if (activeInfoPopover === balance.standardUuid) {
                                                        setActiveInfoPopover(null);
                                                        setPopoverAnchorRect(null);
                                                    } else {
                                                        setActiveInfoPopover(balance.standardUuid);
                                                        setPopoverAnchorRect(rect);
                                                    }
                                                }}
                                                onMouseEnter={(e) => {
                                                    if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                                                    const rect = e.currentTarget.getBoundingClientRect();
                                                    setActiveInfoPopover(balance.standardUuid);
                                                    setPopoverAnchorRect(rect);
                                                }}
                                                onMouseLeave={() => {
                                                    closeTimerRef.current = setTimeout(() => {
                                                        setActiveInfoPopover(null);
                                                        setPopoverAnchorRect(null);
                                                    }, 200);
                                                }}
                                                className="p-1 rounded-full bg-white/20 hover:bg-white/30 text-white transition-colors"
                                                aria-label={t('dashboard.balanceDetailsTooltip')}
                                            >
                                                <Info size={14} />
                                            </button>
                                        </div>
                                    </div>

                                    <div className="relative z-10 flex flex-col items-start">
                                        <span className="text-4xl font-black text-white tracking-tighter">
                                            {balanceViewMode === 'net'
                                                ? formatSignedNetBalance(
                                                      balance.netBalance || balance.totalAmount,
                                                  )
                                                : formatAmount(balance.totalAmount)}
                                        </span>
                                        {balanceViewMode === 'net' &&
                                            balance.ownAvailableAmount &&
                                            parseFloat(balance.ownAvailableAmount) > 0 && (
                                                <span className="text-[11px] font-medium text-white/80 mt-1">
                                                    {t('dashboard.ownFrameAvailable', {
                                                        amount: formatAmount(balance.ownAvailableAmount),
                                                    })}
                                                </span>
                                            )}
                                    </div>

                                    <div className="relative z-10 flex justify-end">
                                        <div className="text-[10px] font-bold text-white/40 uppercase tracking-widest group-hover:text-white transition-colors">
                                            {t('dashboard.viewDetails')}
                                        </div>
                                    </div>
                                </Card>
                            );
                        })}

                        {balances.length === 1 && (
                            <Card
                                variant="none"
                                className="border-2 border-dashed border-theme-subtle bg-transparent p-6 h-48 flex flex-col items-center justify-center text-theme-placeholder hover:border-theme-primary hover:text-theme-primary transition-all group cursor-pointer"
                                hover
                                onClick={() =>
                                    onProtectedNavigate({
                                        view: 'create_voucher',
                                        previousView: { view: 'logged_in' },
                                    })
                                }
                            >
                                <Plus
                                    size={32}
                                    className="mb-2 opacity-50 group-hover:scale-110 transition-transform"
                                />
                                <span className="text-sm font-bold uppercase tracking-widest">
                                    {t('voucher.create')}
                                </span>
                            </Card>
                        )}
                    </div>
                )}
            </section>

            {activeBalance &&
                popoverPos &&
                createPortal(
                    <div
                        ref={popoverRef}
                        style={{ top: `${popoverPos.top}px`, left: `${popoverPos.left}px` }}
                        onClick={(e) => e.stopPropagation()}
                        onMouseEnter={() => {
                            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                        }}
                        onMouseLeave={() => {
                            closeTimerRef.current = setTimeout(() => {
                                setActiveInfoPopover(null);
                                setPopoverAnchorRect(null);
                            }, 200);
                        }}
                        className="fixed z-[9999] w-64 p-3 bg-slate-900/95 backdrop-blur-md text-white rounded-2xl shadow-2xl border border-white/20 text-xs space-y-2 pointer-events-auto animate-in fade-in zoom-in-95 duration-150"
                    >
                        <div className="font-bold border-b border-white/10 pb-1 text-white/90">
                            {t('dashboard.balanceBreakdownTitle')}
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownReceived')}:</span>
                            <span className="font-mono font-bold text-emerald-400">
                                +{formatAmount(activeBalance.receivedAmount || activeBalance.totalAmount)}
                            </span>
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownOwnAvailable')}:</span>
                            <span className="font-mono font-bold text-sky-400">
                                +{formatAmount(activeBalance.ownAvailableAmount || '0')}
                            </span>
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownOwnCirculating')}:</span>
                            <span className="font-mono font-bold text-amber-400">
                                -{formatAmount(activeBalance.ownCirculatingAmount || '0')}
                            </span>
                        </div>
                        <div className="border-t border-white/10 pt-1 flex justify-between items-center font-bold">
                            <span className="text-white/90">{t('dashboard.breakdownNet')}:</span>
                            <span
                                className={`font-mono ${
                                    parseFloat(activeBalance.netBalance || '0') < 0
                                        ? 'text-rose-400'
                                        : 'text-emerald-400'
                                }`}
                            >
                                {formatSignedNetBalance(
                                    activeBalance.netBalance || activeBalance.totalAmount,
                                )}
                            </span>
                        </div>
                    </div>,
                    document.body,
                )}
        </>
    );
}
