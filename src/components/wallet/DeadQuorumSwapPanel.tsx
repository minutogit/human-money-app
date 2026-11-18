// src/components/wallet/DeadQuorumSwapPanel.tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowLeftRight } from 'lucide-react';
import { voucherService } from '../../services/voucherService';
import { translateError } from '../../utils/errorHelper';
import type { ProtectActionFn } from '../../utils/fileIo';
import { Button } from '../ui/Button';
import { CopyButton } from '../ui/CopyButton';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import type { DeadQuorumAdjudicationDto } from '../../types';

interface DeadQuorumSwapPanelProps {
    localInstanceId: string;
    isActiveVoucher: boolean;
    protectAction: ProtectActionFn;
    findMatchingStandardToml: () => Promise<string>;
    onRefreshDetails: () => Promise<void>;
}

export function DeadQuorumSwapPanel({
    localInstanceId,
    isActiveVoucher,
    protectAction,
    findMatchingStandardToml,
    onRefreshDetails,
}: DeadQuorumSwapPanelProps) {
    const { t } = useTranslation();
    const [swapOfferJson, setSwapOfferJson] = useState('');
    const [swapClaimJson, setSwapClaimJson] = useState('');
    const [deadAnchorHexInput, setDeadAnchorHexInput] = useState('');
    const [replAnchorHexInput, setReplAnchorHexInput] = useState('');
    const [swapSubmissionsText, setSwapSubmissionsText] = useState('');
    const [swapAdjudication, setSwapAdjudication] = useState<DeadQuorumAdjudicationDto | null>(null);
    const [swapError, setSwapError] = useState<string | null>(null);
    const [isRunningSwap, setIsRunningSwap] = useState(false);

    const handleCreateSwapOffer = async () => {
        if (!deadAnchorHexInput.trim() || !replAnchorHexInput.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            const offer = await protectAction(async (pwd) => {
                return await voucherService.createDeadQuorumSwapOffer({
                    deadAnchorIdHex: deadAnchorHexInput.trim(),
                    replacementAnchorIdHex: replAnchorHexInput.trim(),
                    password: pwd || null,
                });
            });
            if (offer) setSwapOfferJson(offer);
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
    };

    const handleCreateSwapClaim = async () => {
        if (!swapOfferJson.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            const claimJson = await protectAction(async (pwd) => {
                return await voucherService.createDeadQuorumSwapClaim({
                    offerJson: swapOfferJson.trim(),
                    localInstanceId,
                    password: pwd || null,
                });
            });
            if (claimJson) setSwapClaimJson(claimJson);
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
    };

    const handleAdjudicateSwap = async () => {
        if (!swapOfferJson.trim() || !swapSubmissionsText.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            let submissions: string[];
            const raw = swapSubmissionsText.trim();
            if (raw.startsWith('[')) {
                const arr = JSON.parse(raw) as unknown[];
                submissions = arr.map((item) =>
                    typeof item === 'string' ? item : JSON.stringify(item),
                );
            } else {
                submissions = raw
                    .split(/\n---\n|\n(?=\{)/)
                    .map((s) => s.trim())
                    .filter(Boolean);
            }
            const standardToml = await findMatchingStandardToml();
            const adj = await protectAction(async (pwd) => {
                return await voucherService.adjudicateDeadQuorumSwap({
                    offerJson: swapOfferJson.trim(),
                    submissionsJson: submissions,
                    standardToml,
                    password: pwd || null,
                });
            });
            if (adj) {
                setSwapAdjudication(adj);
                await onRefreshDetails();
            }
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
    };

    return (
        <AdvancedDisclosure
            title={t('voucher.deadQuorumSwapTitle')}
            subtitle={t('voucher.deadQuorumSwapSubtitle')}
            icon={<ArrowLeftRight size={15} className="text-theme-primary" />}
        >
            <div className="space-y-3 pt-1 text-xs">
                {swapError && (
                    <p className="text-[11px] font-medium text-rose-600">{swapError}</p>
                )}

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between gap-2">
                        <p className="font-bold text-[11px] text-theme-secondary">
                            {t('voucher.swapClaimSectionTitle')}
                        </p>
                        {swapOfferJson.trim() && (
                            <CopyButton value={swapOfferJson} size="xs" iconOnly />
                        )}
                    </div>
                    <textarea
                        rows={2}
                        value={swapOfferJson}
                        onChange={(e) => setSwapOfferJson(e.target.value)}
                        placeholder={t('voucher.swapOfferJsonPlaceholder')}
                        className="w-full p-2 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                    />
                    <Button
                        type="button"
                        size="sm"
                        onClick={handleCreateSwapClaim}
                        disabled={isRunningSwap || !swapOfferJson.trim() || !isActiveVoucher}
                    >
                        {t('voucher.createSwapClaimBtn')}
                    </Button>
                    {swapClaimJson && (
                        <div className="space-y-1">
                            <div className="flex justify-end">
                                <CopyButton value={swapClaimJson} size="xs" />
                            </div>
                            <textarea
                                readOnly
                                rows={3}
                                value={swapClaimJson}
                                className="w-full p-2 text-[10px] font-mono rounded-lg border border-emerald-300 bg-emerald-50/40"
                            />
                        </div>
                    )}
                </div>

                <div className="pt-2 border-t border-theme-subtle/40 space-y-2">
                    <p className="font-bold text-[11px] text-theme-secondary">
                        {t('voucher.swapAdjudicateSectionTitle')}
                    </p>
                    <input
                        type="text"
                        value={deadAnchorHexInput}
                        onChange={(e) => setDeadAnchorHexInput(e.target.value)}
                        placeholder={t('voucher.deadAnchorHexPlaceholder')}
                        className="w-full px-2.5 py-1.5 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                    />
                    <input
                        type="text"
                        value={replAnchorHexInput}
                        onChange={(e) => setReplAnchorHexInput(e.target.value)}
                        placeholder={t('voucher.replAnchorHexPlaceholder')}
                        className="w-full px-2.5 py-1.5 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                    />
                    <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={handleCreateSwapOffer}
                        disabled={
                            isRunningSwap ||
                            !deadAnchorHexInput.trim() ||
                            !replAnchorHexInput.trim()
                        }
                    >
                        {t('voucher.createSwapOfferBtn')}
                    </Button>
                    <textarea
                        rows={2}
                        value={swapSubmissionsText}
                        onChange={(e) => setSwapSubmissionsText(e.target.value)}
                        placeholder={t('voucher.swapSubmissionsPlaceholder')}
                        className="w-full p-2 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                    />
                    <Button
                        type="button"
                        size="sm"
                        onClick={handleAdjudicateSwap}
                        disabled={
                            isRunningSwap ||
                            !swapOfferJson.trim() ||
                            !swapSubmissionsText.trim()
                        }
                    >
                        {t('voucher.adjudicateSwapBtn')}
                    </Button>
                    {swapAdjudication && (
                        <div className="p-2.5 rounded-lg border border-emerald-200 bg-emerald-50/50 text-[11px] space-y-1">
                            <div>
                                <span className="font-bold">{t('voucher.swapWinnerLabel')}: </span>
                                <span className="font-mono">
                                    {swapAdjudication.winnerRecipientId || t('common.na')}
                                </span>
                            </div>
                            <div>
                                {t('voucher.swapOffendersExposed', {
                                    count: swapAdjudication.exposedOffenderDids.length,
                                })}
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </AdvancedDisclosure>
    );
}
