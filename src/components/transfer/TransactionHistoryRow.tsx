// src/components/transfer/TransactionHistoryRow.tsx
import { useTranslation } from 'react-i18next';
import { ArrowUpRight, ArrowDownLeft, Calendar, ChevronDown, ChevronUp } from 'lucide-react';
import { Advanced } from '../ui/Advanced';
import { formatDateTime, formatSummary } from '../../utils/format';
import { TransactionDetailsModal } from './TransactionDetailsModal';
import type { TransactionRecord, WalletEvent } from '../../types';

interface TransactionHistoryRowProps {
    record: TransactionRecord;
    events: WalletEvent[];
    isExpanded: boolean;
    isUnacknowledged: boolean;
    isSavingBundle: boolean;
    bundleFeedback?: string;
    isRedepositing: boolean;
    redepositFeedback?: string;
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    isKnownContact: (did?: string | null) => boolean;
    onToggleExpand: (id: string) => void;
    onSaveBundle: (record: TransactionRecord) => void;
    onRedeposit: (bundleId: string) => void;
    onOpenContactDialog: (data: { did: string; name?: string }) => void;
}

export function TransactionHistoryRow({
    record,
    events,
    isExpanded,
    isUnacknowledged,
    isSavingBundle,
    bundleFeedback,
    isRedepositing,
    redepositFeedback,
    resolveIdentity,
    isKnownContact,
    onToggleExpand,
    onSaveBundle,
    onRedeposit,
    onOpenContactDialog,
}: TransactionHistoryRowProps) {
    const { t } = useTranslation();
    const isSent = record.direction === 'sent';
    const counterpartyDid = (isSent ? record.recipientId : record.senderId) || '';
    const counterpartyName = isSent
        ? resolveIdentity(record.recipientId, null)
        : resolveIdentity(record.senderId, record.senderProfileName);

    const shortReceiptNo = (record.bundleId || record.id).slice(0, 8);
    const canSaveAsContact =
        Boolean(counterpartyDid) &&
        counterpartyDid.toLowerCase() !== 'anonymous' &&
        counterpartyDid.startsWith('did:') &&
        !isKnownContact(counterpartyDid);

    return (
        <div
            className={`bg-white border transition-all duration-200 rounded-2xl shadow-xs overflow-hidden ${
                isExpanded
                    ? 'border-theme-primary/40 ring-2 ring-theme-primary/10 shadow-md'
                    : 'border-theme-subtle hover:border-theme-primary/30 hover:shadow-sm'
            }`}
        >
            <div
                className="p-4 cursor-pointer select-none flex items-center justify-between gap-4"
                onClick={() => onToggleExpand(record.id)}
            >
                <div className="flex items-center gap-3.5 flex-1 min-w-0">
                    <div
                        className={`flex items-center justify-center h-10 w-10 rounded-xl shrink-0 ${
                            isSent
                                ? 'bg-slate-100 text-slate-700'
                                : 'bg-emerald-50 text-emerald-600'
                        }`}
                    >
                        {isSent ? <ArrowUpRight size={20} /> : <ArrowDownLeft size={20} />}
                    </div>

                    <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 mb-0.5">
                            <span className="text-[10px] text-theme-light font-medium flex items-center gap-1">
                                <Calendar size={11} className="opacity-70" />
                                {formatDateTime(record.timestamp)}
                            </span>
                            <Advanced>
                                <span className="font-mono text-[10px] bg-theme-subtle/30 text-theme-light px-1.5 py-0.2 rounded font-bold">
                                    #{shortReceiptNo}
                                </span>
                            </Advanced>
                            {isSent && isUnacknowledged && (
                                <span
                                    data-testid="unacknowledged-payment-badge"
                                    className="font-semibold text-[9px] bg-amber-50 text-amber-800 border border-amber-200 px-1.5 py-0.2 rounded flex items-center gap-1"
                                >
                                    <span>📦</span>
                                    <span>{t('history.postWaitingForPickup', 'In Ablage')}</span>
                                </span>
                            )}
                        </div>

                        <div className="flex items-baseline gap-1 text-sm font-bold text-theme-secondary truncate">
                            <span className="text-theme-light font-normal text-xs shrink-0">
                                {isSent ? t('history.to') : t('history.from')}:
                            </span>
                            <span className="truncate text-theme-primary">{counterpartyName}</span>
                        </div>

                        {record.notes && (
                            <p className="text-xs text-theme-light/80 italic truncate max-w-lg mt-0.5">
                                &quot;{record.notes}&quot;
                            </p>
                        )}
                    </div>
                </div>

                <div className="text-right shrink-0 flex items-center gap-3">
                    <div>
                        <p
                            className={`text-base sm:text-lg font-bold font-mono tracking-tight ${
                                isSent ? 'text-slate-700' : 'text-emerald-600'
                            }`}
                        >
                            {isSent ? '-' : '+'}
                            {formatSummary(record.summableAmounts, record.countableItems)}
                        </p>
                        <span
                            className={`text-[10px] font-bold uppercase tracking-wider block ${
                                isSent ? 'text-theme-light' : 'text-emerald-600/80'
                            }`}
                        >
                            {isSent ? t('history.outbound') : t('history.inbound')}
                        </span>
                    </div>

                    <div
                        className="text-theme-light/40 hover:text-theme-primary transition-colors"
                        title={t('common.details')}
                        aria-label={t('common.details')}
                    >
                        {isExpanded ? <ChevronUp size={18} /> : <ChevronDown size={18} />}
                    </div>
                </div>
            </div>

            {isExpanded && (
                <TransactionDetailsModal
                    record={record}
                    events={events}
                    isSavingBundle={isSavingBundle}
                    bundleFeedback={bundleFeedback}
                    isRedepositing={isRedepositing}
                    redepositFeedback={redepositFeedback}
                    canSaveAsContact={canSaveAsContact}
                    counterpartyDid={counterpartyDid}
                    resolveIdentity={resolveIdentity}
                    onSaveBundle={onSaveBundle}
                    onRedeposit={onRedeposit}
                    onOpenContactDialog={onOpenContactDialog}
                />
            )}
        </div>
    );
}
