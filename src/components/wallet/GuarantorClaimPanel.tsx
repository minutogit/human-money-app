// src/components/wallet/GuarantorClaimPanel.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { voucherService } from '../../services/voucherService';
import { saveBytesToFile, type ProtectActionFn } from '../../utils/fileIo';
import { translateError } from '../../utils/errorHelper';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import type { AppSettings, VoucherDetails, VoucherSignature } from '../../types';

interface GuarantorClaimPanelProps {
    details: VoucherDetails;
    guarantorSignatures: VoucherSignature[];
    allowPartialTransfers: boolean;
    settings: AppSettings | null;
    protectAction: ProtectActionFn;
    findMatchingStandardToml: () => Promise<string>;
    onClose: () => void;
    onFeedback: (feedback: { type: 'success' | 'error'; msg: string } | null) => void;
    onRefreshDetails: () => Promise<void>;
}

export function GuarantorClaimPanel({
    details,
    guarantorSignatures,
    allowPartialTransfers,
    settings,
    protectAction,
    findMatchingStandardToml,
    onClose,
    onFeedback,
    onRefreshDetails,
}: GuarantorClaimPanelProps) {
    const { t } = useTranslation();
    const voucher = details.voucher;
    const [selectedGuarantorId, setSelectedGuarantorId] = useState(
        () => guarantorSignatures[0]?.signerId || '',
    );
    const [claimAmount, setClaimAmount] = useState(
        () => details.currentAmount ?? voucher.nominalValue.amount,
    );
    const [isClaimingGuarantor, setIsClaimingGuarantor] = useState(false);

    useEffect(() => {
        if (!selectedGuarantorId && guarantorSignatures.length > 0) {
            setSelectedGuarantorId(guarantorSignatures[0].signerId);
        }
        setClaimAmount(details.currentAmount ?? voucher.nominalValue.amount);
    }, [details.currentAmount, guarantorSignatures, selectedGuarantorId, voucher.nominalValue.amount]);

    const handleCreateGuarantorClaim = async () => {
        const targetGuarantor = selectedGuarantorId || guarantorSignatures[0]?.signerId;
        if (!targetGuarantor) return;

        setIsClaimingGuarantor(true);
        onFeedback(null);
        try {
            const standardToml = await findMatchingStandardToml();
            const res = await protectAction(async (pwd) => {
                return await voucherService.createGuarantorClaimBundle({
                    localInstanceId: details.localInstanceId,
                    guarantorId: targetGuarantor,
                    amount: claimAmount.trim() || (details.currentAmount ?? voucher.nominalValue.amount),
                    standardToml,
                    notes: t('voucher.claimNoteDefault'),
                    password: pwd || null,
                });
            });
            if (res) {
                await saveBytesToFile(res.bundleData, {
                    defaultFilename: `claim-${voucher.voucherId.slice(0, 8)}.hmcbundle`,
                    filters: [
                        { name: t('voucher.export.allFiles'), extensions: ['hmcbundle', '*'] },
                    ],
                    settings,
                    protectAction,
                });
                onFeedback({ type: 'success', msg: t('voucher.claimBundleSaved') });
                onClose();
                await onRefreshDetails();
            }
        } catch (e) {
            onFeedback({ type: 'error', msg: translateError(e, t) });
        } finally {
            setIsClaimingGuarantor(false);
        }
    };

    return (
        <Card className="border border-amber-200 bg-amber-50/50 shadow-premium">
            <div className="space-y-4">
                <div className="flex items-start justify-between gap-2">
                    <div>
                        <h3 className="text-sm font-black text-amber-950 uppercase tracking-wider">
                            {t('voucher.claimAtGuarantorTitle')}
                        </h3>
                        <p className="text-xs text-amber-900/80 mt-0.5">
                            {t('voucher.claimAtGuarantorDesc')}
                        </p>
                    </div>
                    <button
                        type="button"
                        onClick={onClose}
                        className="p-1 text-amber-800 hover:bg-amber-100 rounded-lg"
                        aria-label={t('common.close')}
                    >
                        <X size={16} />
                    </button>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <div>
                        <label className="block text-[10px] font-bold uppercase text-amber-950 mb-1">
                            {t('voucher.claimGuarantorSelectLabel')}
                        </label>
                        <select
                            value={selectedGuarantorId || guarantorSignatures[0]?.signerId || ''}
                            onChange={(e) => setSelectedGuarantorId(e.target.value)}
                            className="w-full px-3 py-2 text-xs font-mono rounded-xl border border-amber-300 bg-white"
                        >
                            {guarantorSignatures.map((sig) => (
                                <option key={sig.signerId} value={sig.signerId}>
                                    {sig.details?.firstName || sig.details?.lastName
                                        ? `${sig.details.firstName || ''} ${sig.details.lastName || ''}`.trim()
                                        : sig.signerId.slice(0, 22)}
                                </option>
                            ))}
                        </select>
                    </div>
                    <div>
                        <label className="block text-[10px] font-bold uppercase text-amber-950 mb-1">
                            {t('voucher.claimAmountLabel')} ({details.displayCurrency})
                        </label>
                        <input
                            type="text"
                            value={claimAmount}
                            onChange={(e) => setClaimAmount(e.target.value)}
                            disabled={!allowPartialTransfers}
                            className="w-full px-3 py-2 text-xs font-bold rounded-xl border border-amber-300 bg-white disabled:opacity-70"
                        />
                    </div>
                </div>
                <div className="flex justify-end gap-2">
                    <Button type="button" size="sm" variant="secondary" onClick={onClose}>
                        {t('common.cancel')}
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        onClick={handleCreateGuarantorClaim}
                        disabled={isClaimingGuarantor}
                    >
                        {isClaimingGuarantor ? t('common.saving') : t('voucher.claimExportBtn')}
                    </Button>
                </div>
            </div>
        </Card>
    );
}
