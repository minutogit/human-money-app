// src/components/wallet/dashboard/RecentActivitySection.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    Plus,
    ArrowUpRight,
    ArrowDownLeft,
    CheckCircle2,
    Clock,
    AlertCircle,
} from 'lucide-react';
import { Button } from '../../ui/Button';
import { Card } from '../../ui/Card';
import {
    getWalletEventPresentation,
    type WalletEventKind,
} from '../../../utils/walletEvents';
import type { useNavigation } from '../../../context/NavigationContext';
import type { WalletEvent } from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

const ICON_BY_KIND: Record<WalletEventKind, React.ElementType> = {
    created: Plus,
    sent: ArrowUpRight,
    received: ArrowDownLeft,
    quarantined: AlertCircle,
    activated: CheckCircle2,
    voided: AlertCircle,
    expired: Clock,
    unknown: AlertCircle,
};

interface RecentActivitySectionProps {
    incompleteCount: number;
    recentEvents: WalletEvent[];
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    onNavigate: NavigateFn;
}

function formatEventMonthDay(isoString: string): string {
    return new Date(isoString).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
    });
}

export function RecentActivitySection({
    incompleteCount,
    recentEvents,
    resolveIdentity,
    onNavigate,
}: RecentActivitySectionProps) {
    const { t } = useTranslation();

    return (
        <>
            {incompleteCount > 0 && (
                <section>
                    <Card
                        variant="accent"
                        className="p-5 flex items-center justify-between border-l-4 border-l-theme-accent"
                        hover
                        onClick={() =>
                            onNavigate({ view: 'wallet', initialStatusFilter: 'incomplete' })
                        }
                    >
                        <div className="flex items-center gap-4">
                            <div className="p-3 rounded-2xl bg-theme-accent text-white shadow-md">
                                <Clock size={24} />
                            </div>
                            <div>
                                <p className="font-black text-theme-secondary uppercase tracking-wider text-xs">
                                    {t('auth.actionRequired')}
                                </p>
                                <p className="text-lg font-bold text-theme-primary">
                                    {t('dashboard.needSignatures', { count: incompleteCount })}
                                </p>
                            </div>
                        </div>
                        <Button variant="primary" size="sm" className="rounded-full px-6">
                            {t('common.view')}
                        </Button>
                    </Card>
                </section>
            )}

            <section>
                <div className="flex items-center justify-between mb-6 px-2">
                    <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">
                        {t('dashboard.recentActivity')}
                    </h2>
                    <button
                        onClick={() => onNavigate({ view: 'transaction_history' })}
                        className="text-[10px] font-black text-theme-accent uppercase tracking-widest hover:underline"
                    >
                        {t('dashboard.historyLink')}
                    </button>
                </div>

                <div className="space-y-3">
                    {recentEvents.length > 0 ? (
                        recentEvents.map((event) => {
                            const presentation = getWalletEventPresentation(
                                event,
                                'dashboard',
                                t,
                                resolveIdentity,
                            );
                            const Icon = ICON_BY_KIND[presentation.kind];
                            return (
                                <Card
                                    key={event.eventId}
                                    variant="default"
                                    className="p-4 flex items-center justify-between group"
                                    hover
                                    onClick={() => {
                                        if (presentation.isTransfer) {
                                            onNavigate({ view: 'transaction_history' });
                                        } else {
                                            onNavigate({
                                                view: 'voucher_details',
                                                voucherId: event.localInstanceId,
                                                previousView: { view: 'logged_in' },
                                            });
                                        }
                                    }}
                                >
                                    <div className="flex items-center gap-4 flex-1 min-w-0">
                                        <div
                                            className={`p-3 rounded-2xl ${presentation.bgColor} ${presentation.color} transition-transform group-hover:scale-110 shrink-0`}
                                        >
                                            <Icon size={20} />
                                        </div>
                                        <div className="min-w-0">
                                            <p className="font-bold text-sm text-theme-secondary truncate">
                                                {presentation.label}
                                            </p>
                                            <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest mt-0.5">
                                                {presentation.amountDisplay}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="text-right shrink-0">
                                        <span className="text-[10px] font-bold text-theme-placeholder uppercase tracking-widest">
                                            {formatEventMonthDay(event.timestamp)}
                                        </span>
                                    </div>
                                </Card>
                            );
                        })
                    ) : (
                        <div className="text-center py-8 px-6">
                            <Card
                                variant="none"
                                className="border-2 border-dashed border-theme-subtle p-8 space-y-4"
                            >
                                <p className="text-sm font-bold uppercase tracking-widest text-theme-placeholder">
                                    {t('dashboard.noRecentActivity')}
                                </p>
                                <div className="flex flex-wrap items-center justify-center gap-2">
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={() =>
                                            onNavigate({
                                                view: 'create_voucher',
                                                previousView: { view: 'logged_in' },
                                            })
                                        }
                                    >
                                        {t('voucher.create')}
                                    </Button>
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => onNavigate({ view: 'receive_bundle' })}
                                    >
                                        {t('transfer.receive')}
                                    </Button>
                                </div>
                            </Card>
                        </div>
                    )}
                </div>
            </section>
        </>
    );
}
