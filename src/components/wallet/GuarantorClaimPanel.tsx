// src/components/wallet/GuarantorClaimPanel.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X, AlertTriangle, Save } from 'lucide-react';
import { voucherService } from '../../services/voucherService';
import { postService } from '../../services/postService';
import { saveBytesToFile, type ProtectActionFn } from '../../utils/fileIo';
import { translateError } from '../../utils/errorHelper';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import type { AppSettings, ClaimBundleDto, FrontendPostDepositResult, VoucherDetails, VoucherSignature } from '../../types';

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
    const [showConfirm, setShowConfirm] = useState(false);
    const [pendingBundle, setPendingBundle] = useState<ClaimBundleDto | null>(null);
    const [saveRequiredWarning, setSaveRequiredWarning] = useState(false);

    useEffect(() => {
        if (!selectedGuarantorId && guarantorSignatures.length > 0) {
            setSelectedGuarantorId(guarantorSignatures[0].signerId);
        }
        setClaimAmount(details.currentAmount ?? voucher.nominalValue.amount);
    }, [details.currentAmount, guarantorSignatures, selectedGuarantorId, voucher.nominalValue.amount]);

    const formatSignerLabel = (sig?: VoucherSignature) => {
        if (!sig) return '';
        const fullName = `${sig.details?.firstName || ''} ${sig.details?.lastName || ''}`.trim();
        return fullName ? `${fullName} (${sig.signerId.slice(0, 12)}…)` : sig.signerId.slice(0, 22);
    };

    const selectedSigner = guarantorSignatures.find((s) => s.signerId === (selectedGuarantorId || guarantorSignatures[0]?.signerId));
    const normalizedAmount = (claimAmount.trim() || (details.currentAmount ?? voucher.nominalValue.amount)).replace(',', '.');

    const handleSavePendingBundle = async () => {
        if (!pendingBundle) return;
        const saved = await saveBytesToFile(pendingBundle.bundleData, {
            defaultFilename: `claim-${voucher.voucherId.slice(0, 8)}.hmcbundle`,
            filters: [
                { name: t('voucher.export.allFiles'), extensions: ['hmcbundle', '*'] },
            ],
            settings,
            protectAction,
        });
        if (saved) {
            setPendingBundle(null);
            setSaveRequiredWarning(false);
            onFeedback({ type: 'success', msg: t('voucher.claimBundleSaved') });
            onClose();
        }
    };

    const handleCreateGuarantorClaim = async () => {
        const targetGuarantor = selectedGuarantorId || guarantorSignatures[0]?.signerId;
        if (!targetGuarantor) return;

        setShowConfirm(false);
        setIsClaimingGuarantor(true);
        setSaveRequiredWarning(false);
        onFeedback(null);
        try {
            const standardToml = await findMatchingStandardToml();
            const outcome = await protectAction(async (pwd) => {
                const bundleRes = await voucherService.createGuarantorClaimBundle({
                    localInstanceId: details.localInstanceId,
                    guarantorId: targetGuarantor,
                    amount: normalizedAmount,
                    standardToml,
                    notes: t('voucher.claimNoteDefault'),
                    password: pwd || null,
                });
                let postRes: FrontendPostDepositResult | null = null;
                if (bundleRes?.bundleId) {
                    try {
                        postRes = await postService.depositTransferToPost(bundleRes.bundleId, pwd || undefined);
                    } catch {
                        postRes = null;
                    }
                }
                return { bundleRes, postRes };
            });
            const res = outcome?.bundleRes;
            const postRes = outcome?.postRes ?? null;
            if (res) {
                setPendingBundle(res);
                await onRefreshDetails();
                const saved = await saveBytesToFile(res.bundleData, {
                    defaultFilename: `claim-${voucher.voucherId.slice(0, 8)}.hmcbundle`,
                    filters: [
                        { name: t('voucher.export.allFiles'), extensions: ['hmcbundle', '*'] },
                    ],
                    settings,
                    protectAction,
                });
                const deliveredToPost = postRes?.recipientDepositedTo ?? 0;
                if (saved && deliveredToPost > 0) {
                    setPendingBundle(null);
                    onFeedback({
                        type: 'success',
                        msg: t('voucher.claimBundleSavedAndSent', { count: deliveredToPost }),
                    });
                    onClose();
                } else if (saved) {
                    setPendingBundle(null);
                    onFeedback({ type: 'success', msg: t('voucher.claimBundleSaved') });
                    onClose();
                } else if (deliveredToPost > 0) {
                    setPendingBundle(null);
                    onFeedback({
                        type: 'success',
                        msg: t('voucher.claimSentViaPostOnly', { count: deliveredToPost }),
                    });
                    onClose();
                } else {
                    setSaveRequiredWarning(true);
                    onFeedback({ type: 'error', msg: t('voucher.claimSaveRequiredWarning') });
                }
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
                {saveRequiredWarning && pendingBundle && (
                    <div className="p-3 rounded-xl border border-rose-300 bg-rose-50 text-xs text-rose-900 flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                        <div className="flex items-start gap-2">
                            <AlertTriangle size={15} className="shrink-0 mt-0.5 text-rose-600" />
                            <span>{t('voucher.claimSaveRequiredWarning')}</span>
                        </div>
                        <Button
                            type="button"
                            size="sm"
                            variant="primary"
                            onClick={handleSavePendingBundle}
                            className="shrink-0 gap-1.5"
                        >
                            <Save size={14} />
                            {t('voucher.claimSaveAgainBtn')}
                        </Button>
                    </div>
                )}
                <div className="flex justify-end gap-2">
                    <Button type="button" size="sm" variant="secondary" onClick={onClose}>
                        {t('common.cancel')}
                    </Button>
                    {!pendingBundle && (
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => setShowConfirm(true)}
                            disabled={isClaimingGuarantor}
                        >
                            {isClaimingGuarantor ? t('common.saving') : t('voucher.claimExportBtn')}
                        </Button>
                    )}
                </div>
            </div>
            <ConfirmationModal
                isOpen={showConfirm}
                title={t('voucher.claimConfirmTitle')}
                confirmVariant="danger"
                description={t('voucher.claimConfirmDesc', {
                    amount: `${normalizedAmount} ${details.displayCurrency}`,
                    guarantor: formatSignerLabel(selectedSigner) || selectedGuarantorId,
                })}
                confirmText={t('voucher.claimExportBtn')}
                onConfirm={handleCreateGuarantorClaim}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isClaimingGuarantor}
            />
        </Card>
    );
}
