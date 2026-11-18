// src/components/transfer/receive/ReceivePaymentRequestCard.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { profileService } from '../../../services/profileService';
import { buildPaymentRequestText, buildPaymentRequestUri } from '../../../utils/humocoUri';
import { parseStandardInfo } from '../../../hooks/useParsedStandards';
import { Input } from '../../ui/Input';
import { Advanced } from '../../ui/Advanced';
import { CopyButton } from '../../ui/CopyButton';
import { QrCodeDisplay } from '../../ui/QrCodeDisplay';
import type { PublicProfile, VoucherStandardInfo } from '../../../types';

interface ReceivePaymentRequestCardProps {
    ownUserId: string;
    ownProfile: PublicProfile | null;
    voucherStandards: VoucherStandardInfo[];
    onCopyError: (message: string) => void;
}

export function ReceivePaymentRequestCard({
    ownUserId,
    ownProfile,
    voucherStandards,
    onCopyError,
}: ReceivePaymentRequestCardProps) {
    const { t } = useTranslation();
    const [reqStandardId, setReqStandardId] = useState('');
    const [reqAmount, setReqAmount] = useState('');
    const [reqNote, setReqNote] = useState('');
    const [reqIsTest, setReqIsTest] = useState(false);
    const [qrSvg, setQrSvg] = useState<string | null>(null);

    useEffect(() => {
        if (!reqStandardId && voucherStandards.length > 0) {
            setReqStandardId(voucherStandards[0].id);
        }
    }, [reqStandardId, voucherStandards]);

    const selectedReqStandard =
        voucherStandards.find((s) => s.id === reqStandardId) || voucherStandards[0];
    const parsedStd = selectedReqStandard ? parseStandardInfo(selectedReqStandard) : null;
    const reqStdUuid = parsedStd?.uuid || selectedReqStandard?.id;
    const reqUnit = parsedStd?.unit || selectedReqStandard?.displayName;
    const reqNameParts = [ownProfile?.firstName, ownProfile?.lastName].filter(Boolean);
    const reqName =
        reqNameParts.length > 0 ? reqNameParts.join(' ') : ownProfile?.organization || '';

    const requestParams = {
        to: ownUserId,
        name: reqName || undefined,
        standardUuid: reqStdUuid,
        standardName: selectedReqStandard?.displayName,
        unit: reqUnit,
        amount: reqAmount.trim() || undefined,
        note: reqNote.trim() || undefined,
        isTest: reqIsTest,
    };

    const generatedPaymentUri = ownUserId ? buildPaymentRequestUri(requestParams) : '';
    const generatedPaymentRequest = buildPaymentRequestText(requestParams);

    useEffect(() => {
        if (!generatedPaymentUri) {
            setQrSvg(null);
            return;
        }
        let active = true;
        profileService
            .encodeTextQrSvg(generatedPaymentUri)
            .then((svg) => {
                if (active) {
                    setQrSvg(typeof svg === 'string' && svg.includes('<svg') ? svg : null);
                }
            })
            .catch(() => {
                if (active) setQrSvg(null);
            });
        return () => {
            active = false;
        };
    }, [generatedPaymentUri]);

    return (
        <div className="bg-white border border-theme-subtle rounded-3xl p-6 shadow-sm space-y-5">
            <div>
                <h3 className="text-base font-black text-theme-secondary">
                    {t('transfer.myCodeTitle')}
                </h3>
                <p className="text-xs text-theme-light mt-1">{t('transfer.myCodeDesc')}</p>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-12 gap-6 items-center">
                <div className="md:col-span-5 flex flex-col items-center gap-3">
                    <QrCodeDisplay
                        svg={qrSvg}
                        qrTestId="receive-qr-svg"
                        size={192}
                        ariaLabel={t('transfer.myCodeTitle')}
                    />
                    <CopyButton
                        value={generatedPaymentRequest}
                        label={t('transfer.requestPayment.copyButton')}
                        copiedLabel={t('transfer.requestPayment.copied')}
                        variant="primary"
                        size="sm"
                        resetDelayMs={2500}
                        onCopyFailed={() => onCopyError(t('transfer.requestPayment.copyFailed'))}
                        className="w-full py-2.5 rounded-xl gap-2 font-bold text-xs shadow-sm"
                    />
                </div>

                <div className="md:col-span-7 space-y-4">
                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">
                            {t('transfer.requestPayment.selectStandard')}
                        </label>
                        <select
                            value={reqStandardId}
                            onChange={(e) => setReqStandardId(e.target.value)}
                            className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary text-sm font-medium focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-sm"
                        >
                            {voucherStandards.map((std) => (
                                <option key={std.id} value={std.id}>
                                    {std.displayName}
                                </option>
                            ))}
                        </select>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">
                                {t('transfer.requestPayment.amount')} ({t('common.optional')})
                            </label>
                            <Input
                                type="number"
                                min="0"
                                step="any"
                                value={reqAmount}
                                onChange={(e) => setReqAmount(e.target.value)}
                                placeholder="z. B. 15"
                                className="text-sm font-medium"
                            />
                        </div>
                        <div>
                            <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">
                                {t('transfer.requestPayment.unit')}
                            </label>
                            <div className="px-4 py-2.5 bg-slate-50 border border-theme-subtle rounded-xl text-sm font-bold text-theme-secondary">
                                {reqUnit || '–'}
                            </div>
                        </div>
                    </div>

                    <div>
                        <label className="block text-xs font-bold text-theme-light uppercase tracking-widest mb-1.5">
                            {t('transfer.requestPayment.purpose')} ({t('common.optional')})
                        </label>
                        <Input
                            type="text"
                            value={reqNote}
                            onChange={(e) => setReqNote(e.target.value)}
                            placeholder={t('transfer.requestPayment.purposePlaceholder')}
                            className="text-sm font-medium"
                        />
                    </div>

                    <Advanced>
                        <label className="flex items-center gap-3 cursor-pointer select-none pt-1">
                            <input
                                type="checkbox"
                                checked={reqIsTest}
                                onChange={(e) => setReqIsTest(e.target.checked)}
                                className="rounded accent-theme-primary h-4 w-4"
                            />
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('transfer.requestPayment.isTest')}
                            </span>
                        </label>
                    </Advanced>
                </div>
            </div>

            <Advanced>
                <div className="bg-slate-50 border border-theme-subtle rounded-2xl p-4 space-y-2.5">
                    <div className="flex items-center justify-between">
                        <h4 className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                            {t('transfer.requestPayment.previewTitle')}
                        </h4>
                        <span className="text-[10px] font-bold text-theme-light bg-white border border-theme-subtle/50 px-2 py-0.5 rounded-full">
                            {t('transfer.requestPayment.readyToShare')}
                        </span>
                    </div>
                    <pre className="p-3 bg-white border border-theme-subtle rounded-xl text-xs font-mono text-theme-secondary whitespace-pre-wrap break-all shadow-inner">
                        {generatedPaymentRequest}
                    </pre>
                </div>
            </Advanced>
        </div>
    );
}
