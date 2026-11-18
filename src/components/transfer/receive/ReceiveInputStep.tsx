// src/components/transfer/receive/ReceiveInputStep.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    UploadCloud,
    FileCheck,
    FileSignature,
    ArrowRightLeft,
    Lock,
    X,
    CheckCircle2,
    AlertCircle,
} from 'lucide-react';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { Advanced } from '../../ui/Advanced';
import { HelpIcon } from '../../ui/HelpIcon';
import type { AppSettings } from '../../../types';
import type { PendingPosFallbackState } from './useReceiveFlow';

interface ReceiveInputStepProps {
    settings: AppSettings | null;
    isPosMode: boolean;
    purchaseId: string;
    onPosModeToggle: (checked: boolean) => void;
    onPurchaseIdChange: (value: string) => void;
    pendingPosFallback: PendingPosFallbackState | null;
    onAcceptPosFallback: () => void;
    onCancelPosFallback: () => void;
    isProcessing: boolean;
    isDragOver: boolean;
    bundleName: string | null;
    onDragOver: (e: React.DragEvent<HTMLDivElement>) => void;
    onDragLeave: (e: React.DragEvent<HTMLDivElement>) => void;
    onDrop: (e: React.DragEvent<HTMLDivElement>) => void;
    onSelectFile: () => void;
    onClearSelection: () => void;
}

export function ReceiveInputStep({
    settings,
    isPosMode,
    purchaseId,
    onPosModeToggle,
    onPurchaseIdChange,
    pendingPosFallback,
    onAcceptPosFallback,
    onCancelPosFallback,
    isProcessing,
    isDragOver,
    bundleName,
    onDragOver,
    onDragLeave,
    onDrop,
    onSelectFile,
    onClearSelection,
}: ReceiveInputStepProps) {
    const { t } = useTranslation();

    return (
        <div className="space-y-8">
            <Advanced>
                {settings?.enablePosMode && (
                    <div className="flex flex-col sm:flex-row items-center justify-between gap-4 p-4 bg-white/40 border border-theme-subtle/50 rounded-2xl">
                        <div className="flex items-center gap-2">
                            <label className="flex items-center gap-3 cursor-pointer select-none">
                                <input
                                    type="checkbox"
                                    checked={isPosMode}
                                    onChange={(e) => onPosModeToggle(e.target.checked)}
                                    className="rounded accent-theme-primary h-4 w-4"
                                />
                                <span className="text-xs font-bold text-theme-secondary">
                                    {t('transfer.posModeToggle', 'PoS Cashier Mode (Point-of-Sale Checkout)')}
                                </span>
                            </label>
                            <HelpIcon topic="posMode" size={14} />
                        </div>
                        {isPosMode && (
                            <div className="w-full sm:w-64">
                                <Input
                                    placeholder={t(
                                        'transfer.purchaseIdPlaceholder',
                                        'Receipt / Purchase ID (optional)',
                                    )}
                                    value={purchaseId}
                                    onChange={(e) => onPurchaseIdChange(e.target.value)}
                                    className="text-xs font-mono py-1.5"
                                />
                            </div>
                        )}
                    </div>
                )}
            </Advanced>

            {pendingPosFallback && (
                <div
                    data-testid="pos-fallback-card"
                    className="p-6 bg-amber-500/10 border border-amber-500/30 rounded-3xl space-y-4 animate-in fade-in duration-300"
                >
                    <div className="flex items-start gap-3">
                        <AlertCircle className="text-amber-600 shrink-0 mt-0.5" size={20} />
                        <div className="space-y-1">
                            <h4 className="text-base font-bold text-amber-900">
                                {t('transfer.checkout.fallbackCardTitle', 'L2 PoS Protection Unavailable')}
                            </h4>
                            <p className="text-xs text-amber-800 leading-relaxed">
                                {pendingPosFallback.merchantWarning ||
                                    (pendingPosFallback.outcomeKind === 'offlineOnly'
                                        ? t(
                                              'transfer.checkout.fallbackNoServersDesc',
                                              'No lock servers are configured (N = 0). You can still accept this transfer on a trust basis using pure Layer-1 math (SST V4).',
                                          )
                                        : t(
                                              'transfer.checkout.fallbackQuorumDesc',
                                              'Could not reach enough trusted lock servers for green release. You can reject or accept on a trust basis (L1 verified).',
                                          ))}
                            </p>
                            <p className="text-[11px] text-amber-700/80 font-medium">
                                {t(
                                    'transfer.checkout.fallbackNotice',
                                    'Offline transfers are cryptographically protected by SST V4 self-exposure if a double-spend is attempted.',
                                )}
                            </p>
                        </div>
                    </div>
                    <div className="flex items-center justify-end gap-3 pt-2">
                        <button
                            type="button"
                            onClick={onCancelPosFallback}
                            disabled={isProcessing}
                            className="px-4 py-2 text-xs font-bold text-stone-600 hover:text-stone-900 transition-colors"
                        >
                            {t('transfer.checkout.fallbackCancel', 'Cancel / Reject')}
                        </button>
                        <Button
                            type="button"
                            onClick={onAcceptPosFallback}
                            disabled={isProcessing}
                            className="bg-amber-600 hover:bg-amber-700 text-white text-xs font-bold px-4 py-2 rounded-xl shadow-md"
                        >
                            {isProcessing
                                ? t('transfer.importFile.loading')
                                : t(
                                      'transfer.checkout.fallbackAcceptTrust',
                                      'Accept on Trust Basis (L1 Verified)',
                                  )}
                        </Button>
                    </div>
                </div>
            )}

            <div
                id="bundle-drop-zone"
                className={`relative group border-2 border-dashed rounded-[40px] p-12 transition-all flex flex-col items-center justify-center text-center overflow-hidden min-h-[280px] cursor-pointer ${
                    isDragOver
                        ? 'border-theme-primary bg-theme-primary/5 shadow-premium-lg'
                        : bundleName
                          ? 'border-emerald-500/30 bg-emerald-50/10'
                          : 'border-theme-subtle hover:border-theme-primary/40 hover:bg-white/40'
                }`}
                onDragOver={onDragOver}
                onDragLeave={onDragLeave}
                onDrop={onDrop}
                onClick={bundleName ? undefined : onSelectFile}
            >
                <div
                    className={`absolute inset-0 transition-opacity duration-700 pointer-events-none ${
                        isDragOver ? 'opacity-100' : 'opacity-0'
                    }`}
                >
                    <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[300px] h-[300px] bg-theme-primary/10 rounded-full blur-[100px]" />
                </div>

                {bundleName ? (
                    <div className="relative animate-in zoom-in duration-300 space-y-6">
                        <div className="mx-auto w-24 h-24 bg-emerald-500 rounded-[32px] flex items-center justify-center shadow-lg shadow-emerald-200 text-white">
                            {isProcessing ? (
                                <ArrowRightLeft className="animate-spin" size={48} />
                            ) : (
                                <FileCheck size={48} />
                            )}
                        </div>
                        <div>
                            <h3 className="text-xl font-black text-theme-primary tracking-tight mb-2">
                                {isProcessing
                                    ? t('transfer.importFile.loading')
                                    : t('transfer.importFile.fileDetected')}
                            </h3>
                            <div className="flex flex-col items-center gap-1.5 max-w-full">
                                <p className="text-xs font-mono font-bold text-emerald-600 bg-emerald-50 px-3 py-1 rounded-full border border-emerald-100 max-w-full truncate break-all">
                                    {bundleName}
                                </p>
                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mt-2">
                                    {isProcessing
                                        ? t('transfer.importFile.loading')
                                        : t('transfer.importFile.readyToImport')}
                                </p>
                            </div>
                        </div>
                        {!isProcessing && (
                            <div className="pt-4">
                                <button
                                    type="button"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onClearSelection();
                                    }}
                                    className="text-[10px] font-black uppercase tracking-widest text-theme-light hover:text-rose-500 transition-colors flex items-center gap-1 mx-auto"
                                >
                                    <X size={12} /> {t('transfer.importFile.discard')}
                                </button>
                            </div>
                        )}
                    </div>
                ) : (
                    <div className="relative space-y-6">
                        <div className="mx-auto w-20 h-20 bg-theme-primary/5 rounded-[32px] flex items-center justify-center border border-theme-primary/10 text-theme-primary transition-transform group-hover:scale-110 group-hover:rotate-3 duration-500 shadow-inner-soft">
                            <UploadCloud size={36} />
                        </div>
                        <div className="space-y-2">
                            <h3 className="text-xl font-black text-theme-primary tracking-tight">
                                {t('transfer.importFile.dropZone')}
                            </h3>
                            <Advanced>
                                <p className="text-sm font-medium text-theme-light/60 max-w-[280px]">
                                    {t('transfer.importFile.dropZoneHint')}
                                </p>
                            </Advanced>
                        </div>
                        <div className="pt-4">
                            <Button
                                type="button"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onSelectFile();
                                }}
                                variant="secondary"
                                className="rounded-2xl px-8 shadow-premium group-hover:bg-theme-secondary group-hover:text-white transition-all"
                            >
                                {t('transfer.importFile.selectFile')}
                            </Button>
                        </div>
                    </div>
                )}
            </div>

            <Advanced>
                <div className="text-center">
                    <p className="text-[10px] font-bold text-theme-light flex items-center justify-center gap-2">
                        <Lock size={12} />
                        {t('transfer.importFile.localProcessing')}
                    </p>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-4">
                    {[
                        {
                            icon: FileCheck,
                            titleKey: 'transfer.importFile.formatTransfer',
                            descKey: 'transfer.importFile.formatTransferDesc',
                            ext: '.transfer',
                        },
                        {
                            icon: FileSignature,
                            titleKey: 'transfer.importFile.formatSignatureRequest',
                            descKey: 'transfer.importFile.formatSigReqDesc',
                            ext: '.ask',
                        },
                        {
                            icon: CheckCircle2,
                            titleKey: 'transfer.importFile.formatSignatureResponse',
                            descKey: 'transfer.importFile.formatSigRespDesc',
                            ext: '.sig',
                        },
                    ].map((item, i) => (
                        <div
                            key={i}
                            className="p-4 bg-white/40 border border-theme-subtle/50 rounded-2xl flex flex-col items-center text-center"
                        >
                            <item.icon size={18} className="text-theme-light/60 mb-2" />
                            <h4 className="text-[10px] font-black uppercase tracking-widest text-theme-secondary mb-1">
                                {t(item.titleKey)}
                            </h4>
                            <p className="text-[10px] text-theme-light font-medium leading-tight">
                                {t(item.descKey)}
                            </p>
                            <span className="mt-2 text-[9px] font-mono font-bold bg-theme-subtle/20 px-2 py-0.5 rounded text-theme-light">
                                {item.ext}
                            </span>
                        </div>
                    ))}
                </div>
            </Advanced>
        </div>
    );
}
