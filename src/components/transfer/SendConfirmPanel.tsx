// src/components/transfer/SendConfirmPanel.tsx
import { useTranslation } from 'react-i18next';
import { Shield, Send } from 'lucide-react';
import { Button } from '../ui/Button';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { UserAvatar } from '../ui/UserAvatar';
import { formatAmount, getDecimalFractionLength } from '../../utils/format';
import type { ConfirmedTransferSnapshot } from './sendTransferHelpers';
import type { Contact, VoucherSummary } from '../../types';

interface SendConfirmPanelProps {
    recipientId: string;
    contacts: Contact[];
    selectedMap: Map<string, string>;
    selectionStats: Record<string, { total: number; totalExact?: string; count: number }>;
    availableVouchers: VoucherSummary[];
    uuidToPrecisionMap: Map<string, number>;
    privacyMode: 'public' | 'stealth' | null;
    isProcessing: boolean;
    isPreviewPending: boolean;
    showConfirm: boolean;
    confirmedSnapshot: ConfirmedTransferSnapshot | null;
    onPrepareTransfer: (e: React.FormEvent) => void;
    onConfirmExecute: () => void;
    onCancelConfirm: () => void;
}

export function SendConfirmPanel({
    recipientId,
    contacts,
    selectedMap,
    selectionStats,
    availableVouchers,
    uuidToPrecisionMap,
    privacyMode,
    isProcessing,
    isPreviewPending,
    showConfirm,
    confirmedSnapshot,
    onPrepareTransfer,
    onConfirmExecute,
    onCancelConfirm,
}: SendConfirmPanelProps) {
    const { t } = useTranslation();
    const modalRecipientId = confirmedSnapshot?.recipientId ?? recipientId;
    const modalStats = confirmedSnapshot?.selectionStats ?? selectionStats;
    const modalPrivacyMode = confirmedSnapshot?.privacyMode ?? privacyMode;
    const matched = contacts.find((c) => c.did === modalRecipientId);
    const matchedName = matched
        ? [`${matched.profile.firstName || ''}`, `${matched.profile.lastName || ''}`]
              .join(' ')
              .trim() || matched.profile.organization
        : undefined;

    return (
        <>
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 bg-white border border-theme-subtle rounded-3xl p-4 shadow-sm">
                <div className="min-w-0">
                    <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                        {t('transfer.disbursementSummary')}
                    </p>
                    <div className="text-xl font-black tracking-tighter text-theme-primary flex flex-wrap gap-x-3 gap-y-0.5">
                        {selectedMap.size > 0 ? (
                            Object.entries(selectionStats).map(([unit, stat]) => {
                                const voucher = availableVouchers.find(
                                    (v) => v.displayCurrency === unit,
                                );
                                const precision = voucher
                                    ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2)
                                    : 2;
                                return (
                                    <span key={unit} className="inline-flex items-baseline gap-1">
                                        {formatAmount(stat.totalExact ?? stat.total.toString(), precision)}
                                        <span className="text-xs font-bold uppercase tracking-widest text-theme-light">
                                            {unit}
                                        </span>
                                    </span>
                                );
                            })
                        ) : (
                            <span className="text-theme-light">0.00</span>
                        )}
                    </div>
                </div>
                <Button
                    type="button"
                    variant="primary"
                    onClick={onPrepareTransfer}
                    disabled={isProcessing || isPreviewPending}
                    className="gap-2 rounded-2xl px-6 py-3 font-black text-xs uppercase tracking-widest shrink-0"
                >
                    <Send size={15} />
                    <span>{isProcessing ? t('transfer.sending') : t('transfer.sendVoucher')}</span>
                </Button>
            </div>

            <ConfirmationModal
                isOpen={showConfirm && Boolean(confirmedSnapshot)}
                title={t('transfer.confirmTitle')}
                description={
                    <div className="space-y-4 pt-2 text-left">
                        <div className="flex items-center gap-3 p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                            <UserAvatar
                                size={40}
                                pictureUrl={matched?.profile.pictureUrl}
                                name={matchedName || modalRecipientId}
                            />
                            <div className="min-w-0 flex-1 space-y-1">
                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                                    {t('transfer.confirmRecipient')}
                                </p>
                                {matchedName ? (
                                    <>
                                        <p className="text-sm font-black text-theme-secondary truncate">
                                            {matchedName}
                                        </p>
                                        <Advanced>
                                            <TechId value={modalRecipientId} kind="did" />
                                        </Advanced>
                                    </>
                                ) : (
                                    <TechId value={modalRecipientId} kind="did" />
                                )}
                            </div>
                        </div>
                        <div className="p-4 bg-slate-50 rounded-2xl border border-slate-100">
                            <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mb-2">
                                {t('transfer.confirmAssets')}
                            </p>
                            <div className="text-lg font-black tracking-tighter text-theme-primary">
                                {Object.entries(modalStats)
                                    .map(([displayCurrency, stat]) => {
                                        const valStr = stat.totalExact ?? stat.total.toString();
                                        const voucher = availableVouchers.find(
                                            (v) => v.displayCurrency === displayCurrency,
                                        );
                                        const stdPrecision = voucher
                                            ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2)
                                            : 2;
                                        const precision = Math.min(
                                            18,
                                            Math.max(stdPrecision, getDecimalFractionLength(valStr, true)),
                                        );
                                        return `${formatAmount(valStr, precision)} ${displayCurrency}`;
                                    })
                                    .join(', ')}
                            </div>
                        </div>
                        <Advanced>
                            <div className="flex items-center gap-2 px-1">
                                <Shield
                                    size={14}
                                    className={
                                        modalPrivacyMode === 'stealth'
                                            ? 'text-purple-500'
                                            : 'text-blue-500'
                                    }
                                />
                                <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                                    {t('transfer.confirmPrivacy')}{' '}
                                    <span
                                        className={
                                            modalPrivacyMode === 'stealth'
                                                ? 'text-purple-600'
                                                : 'text-blue-600'
                                        }
                                    >
                                        {modalPrivacyMode === 'stealth'
                                            ? t('transfer.stealth')
                                            : t('transfer.public')}
                                    </span>
                                </p>
                            </div>
                        </Advanced>
                    </div>
                }
                confirmText={t('transfer.confirmExecute')}
                onConfirm={onConfirmExecute}
                onCancel={onCancelConfirm}
            />
        </>
    );
}
