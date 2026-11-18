// src/components/transfer/TransactionDetailsModal.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    User,
    Download,
    Hash,
    Layers,
    Info,
    CheckCircle2,
    UserPlus,
    RefreshCw,
    UploadCloud,
    PlusCircle,
    ArrowUpRight,
    ArrowDownLeft,
    ShieldAlert,
    XCircle,
    Clock,
    History,
} from 'lucide-react';
import { Button } from '../ui/Button';
import { Advanced } from '../ui/Advanced';
import { Disclosure } from '../ui/Disclosure';
import { TechId } from '../ui/TechId';
import { formatDateTime } from '../../utils/format';
import {
    getWalletEventPresentation,
    matchEventsForTransactionRecord,
    type WalletEventKind,
} from '../../utils/walletEvents';
import type { TransactionRecord, WalletEvent } from '../../types';

export const HISTORY_EVENT_ICONS: Record<WalletEventKind, React.ElementType> = {
    created: PlusCircle,
    sent: ArrowUpRight,
    received: ArrowDownLeft,
    quarantined: ShieldAlert,
    activated: CheckCircle2,
    voided: XCircle,
    expired: Clock,
    unknown: Info,
};

interface TransactionDetailsModalProps {
    record: TransactionRecord;
    events: WalletEvent[];
    isSavingBundle: boolean;
    bundleFeedback?: string;
    isRedepositing: boolean;
    redepositFeedback?: string;
    canSaveAsContact: boolean;
    counterpartyDid: string;
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    onSaveBundle: (record: TransactionRecord) => void;
    onRedeposit: (bundleId: string) => void;
    onOpenContactDialog: (data: { did: string; name?: string }) => void;
}

export function TransactionDetailsModal({
    record,
    events,
    isSavingBundle,
    bundleFeedback,
    isRedepositing,
    redepositFeedback,
    canSaveAsContact,
    counterpartyDid,
    resolveIdentity,
    onSaveBundle,
    onRedeposit,
    onOpenContactDialog,
}: TransactionDetailsModalProps) {
    const { t } = useTranslation();
    const isSent = record.direction === 'sent';
    const fullTxId = record.bundleId || record.id;
    const matchingEvents = matchEventsForTransactionRecord(record, events);

    return (
        <div className="px-5 pb-5 pt-3 border-t border-theme-subtle/60 bg-bg-app/30 animate-in slide-in-from-top-1 duration-150 space-y-4">
            {record.notes && (
                <div className="p-3 bg-white border border-theme-subtle rounded-xl text-xs">
                    <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider mb-1">
                        {t('history.notes')}
                    </p>
                    <p className="text-theme-secondary whitespace-pre-wrap break-words leading-relaxed">
                        {record.notes}
                    </p>
                </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-3" onClick={(e) => e.stopPropagation()}>
                    <div className="p-3 bg-white border border-theme-subtle rounded-xl space-y-1.5">
                        <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1">
                            <Hash size={12} /> {t('history.txId')}
                        </p>
                        <TechId value={fullTxId} kind="bundle" />
                    </div>

                    {counterpartyDid && (
                        <div className="p-3 bg-white border border-theme-subtle rounded-xl space-y-1.5">
                            <div className="flex items-center justify-between gap-2">
                                <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1">
                                    <User size={12} />{' '}
                                    {isSent ? t('history.recipientDid') : t('history.senderDid')}
                                </p>
                                {canSaveAsContact && (
                                    <button
                                        type="button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            onOpenContactDialog({
                                                did: counterpartyDid,
                                                name: isSent ? undefined : record.senderProfileName,
                                            });
                                        }}
                                        className="text-xs font-bold text-theme-primary hover:text-theme-accent transition-colors flex items-center gap-1"
                                    >
                                        <UserPlus size={12} />
                                        <span className="text-[10px]">
                                            {t('history.saveAsContact')}
                                        </span>
                                    </button>
                                )}
                            </div>
                            <TechId value={counterpartyDid} kind="did" />
                        </div>
                    )}
                </div>

                <div className="flex flex-col justify-between space-y-3">
                    <div className="p-3.5 bg-emerald-50/70 border border-emerald-200/80 rounded-xl">
                        <p className="text-[10px] font-bold text-emerald-800 uppercase tracking-wider mb-1">
                            {t('history.txStatus')}
                        </p>
                        <div className="flex items-center gap-2 text-emerald-900">
                            <CheckCircle2 size={16} className="text-emerald-600 shrink-0" />
                            <span className="text-xs font-bold leading-tight">
                                {t('history.verified')}
                            </span>
                        </div>
                    </div>

                    {isSent && record.bundleData && record.bundleData.length > 0 && (
                        <div className="pt-2 space-y-2">
                            <Button
                                size="sm"
                                variant="secondary"
                                className="w-full gap-2 rounded-xl text-xs"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onSaveBundle(record);
                                }}
                                disabled={isSavingBundle}
                            >
                                <Download size={14} />
                                {isSavingBundle ? t('history.saving') : t('history.export')}
                            </Button>
                            {bundleFeedback && (
                                <span className="block text-center text-[10px] font-bold text-emerald-600 mt-1 animate-pulse">
                                    {bundleFeedback}
                                </span>
                            )}

                            {record.bundleId && (
                                <div>
                                    <Button
                                        size="sm"
                                        variant="outline"
                                        className="w-full gap-1.5 rounded-xl text-xs border-amber-200 text-amber-900 hover:bg-amber-50"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            if (record.bundleId) onRedeposit(record.bundleId);
                                        }}
                                        disabled={isRedepositing}
                                        data-testid="history-redeposit-button"
                                    >
                                        {isRedepositing ? (
                                            <RefreshCw size={12} className="animate-spin" />
                                        ) : (
                                            <UploadCloud size={12} />
                                        )}
                                        {isRedepositing
                                            ? t('transfer.depositing', 'Hinterlege...')
                                            : t('history.redepositButton', 'Erneut in Ablage hinterlegen')}
                                    </Button>
                                    {redepositFeedback && (
                                        <span className="block text-center text-[10px] font-bold text-amber-700 mt-1">
                                            {redepositFeedback}
                                        </span>
                                    )}
                                </div>
                            )}
                        </div>
                    )}
                </div>
            </div>

            {record.involvedSourcesDetails && record.involvedSourcesDetails.length > 0 && (
                <div className="pt-2 space-y-2">
                    <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1.5">
                        <Layers size={12} />{' '}
                        {t('history.involvedAssets', {
                            count: record.involvedSourcesDetails.length,
                        })}
                    </p>
                    <div className="overflow-hidden border border-theme-subtle rounded-xl bg-white shadow-xs">
                        <table className="w-full text-xs text-left">
                            <thead className="bg-bg-app border-b border-theme-subtle">
                                <tr>
                                    <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider">
                                        {t('history.assetStandard')}
                                    </th>
                                    <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider text-right">
                                        {t('history.value')}
                                    </th>
                                    <Advanced>
                                        <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider">
                                            {t('history.localId')}
                                        </th>
                                    </Advanced>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-theme-subtle/50">
                                {record.involvedSourcesDetails.map((detail, idx) => (
                                    <tr key={idx} className="hover:bg-bg-app/40 transition-colors">
                                        <td className="px-3.5 py-2 font-bold text-theme-secondary">
                                            {detail.standardName}
                                        </td>
                                        <td className="px-3.5 py-2 font-mono font-bold text-theme-primary text-right">
                                            {detail.amount} {detail.displayCurrency}
                                        </td>
                                        <Advanced>
                                            <td
                                                className="px-3.5 py-2 font-mono text-theme-light opacity-60 text-[11px]"
                                                title={detail.localInstanceId}
                                            >
                                                {detail.localInstanceId.slice(0, 10)}...
                                            </td>
                                        </Advanced>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </div>
            )}

            {matchingEvents.length > 0 && (
                <Advanced>
                    <div className="pt-2" onClick={(e) => e.stopPropagation()}>
                        <Disclosure
                            toggleTestId="tx-event-log-toggle"
                            contentTestId="tx-event-log-content"
                            icon={<History size={13} className="text-theme-light" />}
                            title={
                                <span className="text-[11px] font-bold text-theme-secondary">
                                    {t('history.txEventsToggle', { count: matchingEvents.length })}
                                </span>
                            }
                            className="border border-theme-subtle rounded-xl bg-white overflow-hidden"
                            buttonClassName="w-full flex items-center justify-between gap-2 px-3.5 py-2.5 text-left hover:bg-bg-app/40 transition-colors cursor-pointer"
                            contentClassName="px-3.5 pb-3 pt-1 border-t border-theme-subtle/50 space-y-2"
                        >
                            {matchingEvents.map((ev) => {
                                const pres = getWalletEventPresentation(
                                    ev,
                                    'history',
                                    t,
                                    resolveIdentity,
                                );
                                const Icon = HISTORY_EVENT_ICONS[pres.kind];
                                return (
                                    <div
                                        key={ev.eventId}
                                        className="flex items-center justify-between gap-3 py-1.5 text-xs"
                                    >
                                        <div className="flex items-center gap-2.5 min-w-0">
                                            <span
                                                className={`inline-flex items-center justify-center h-6 w-6 rounded-lg shrink-0 ${pres.bgColor} ${pres.color}`}
                                            >
                                                <Icon size={13} />
                                            </span>
                                            <div className="min-w-0">
                                                <p className="font-semibold text-theme-secondary truncate">
                                                    {pres.label}
                                                </p>
                                                <div className="flex items-center gap-2 text-[10px] font-mono text-theme-light">
                                                    <span>#{ev.localInstanceId.slice(0, 8)}</span>
                                                    {pres.amountDisplay && (
                                                        <span className="font-bold text-theme-accent">
                                                            {pres.amountDisplay}
                                                        </span>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                        <span className="text-[10px] text-theme-light shrink-0">
                                            {formatDateTime(ev.timestamp)}
                                        </span>
                                    </div>
                                );
                            })}
                        </Disclosure>
                    </div>
                </Advanced>
            )}
        </div>
    );
}
