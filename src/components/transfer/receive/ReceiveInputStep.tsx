// src/components/transfer/receive/ReceiveInputStep.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    UploadCloud,
    FileCheck,
    FileSignature,
    Lock,
    X,
    CheckCircle2,
    RefreshCw,
    ClipboardPaste,
    ArrowRightLeft,
} from 'lucide-react';
import { Button } from '../../ui/Button';
import { Textarea } from '../../ui/Textarea';
import { AlertBanner } from '../../ui/AlertBanner';
import { Advanced } from '../../ui/Advanced';
import { readFromClipboard } from '../../../utils/clipboard';

interface ReceiveInputStepProps {
    isProcessing: boolean;
    isCheckingPost?: boolean;
    postCheckStatus?: { variant: 'info' | 'success' | 'error'; message: string } | null;
    onCheckPostNow?: () => void;
    onPasteRawInput?: (rawText: string) => Promise<boolean>;
    isDragOver: boolean;
    bundleName: string | null;
    onDragOver: (e: React.DragEvent<HTMLDivElement>) => void;
    onDragLeave: (e: React.DragEvent<HTMLDivElement>) => void;
    onDrop: (e: React.DragEvent<HTMLDivElement>) => void;
    onSelectFile: () => void;
    onClearSelection: () => void;
}

export function ReceiveInputStep({
    isProcessing,
    isCheckingPost = false,
    postCheckStatus = null,
    onCheckPostNow,
    onPasteRawInput,
    isDragOver,
    bundleName,
    onDragOver,
    onDragLeave,
    onDrop,
    onSelectFile,
    onClearSelection,
}: ReceiveInputStepProps) {
    const { t } = useTranslation();
    const [showPasteBox, setShowPasteBox] = useState(false);
    const [pastedText, setPastedText] = useState('');

    return (
        <div className="space-y-8">
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
                        <div className="pt-4 flex flex-wrap items-center justify-center gap-2.5">
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
                            {onCheckPostNow && (
                                <Button
                                    type="button"
                                    variant="outline"
                                    data-testid="receive-check-post-btn"
                                    disabled={isCheckingPost || isProcessing}
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onCheckPostNow();
                                    }}
                                    className="rounded-2xl gap-1.5"
                                >
                                    <RefreshCw size={14} className={isCheckingPost ? 'animate-spin' : ''} />
                                    <span>{t('transfer.checkPostNowBtn', 'Postfach jetzt prüfen')}</span>
                                </Button>
                            )}
                            {onPasteRawInput && (
                                <Button
                                    type="button"
                                    variant="outline"
                                    data-testid="receive-toggle-paste-btn"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        setShowPasteBox((prev) => !prev);
                                    }}
                                    className="rounded-2xl gap-1.5"
                                >
                                    <ClipboardPaste size={14} />
                                    <span>{t('common.pasteLinkBtn', 'Link / Code einfügen')}</span>
                                </Button>
                            )}
                        </div>
                    </div>
                )}
            </div>

            {postCheckStatus && (
                <AlertBanner variant={postCheckStatus.variant}>
                    {postCheckStatus.message}
                </AlertBanner>
            )}

            {showPasteBox && onPasteRawInput && (
                <div className="p-4 bg-white border border-theme-subtle rounded-2xl space-y-3 shadow-sm">
                    <div className="flex items-center justify-between gap-2">
                        <span className="text-xs font-bold text-theme-secondary">
                            {t('common.pasteModalTitle', 'Link oder Code einfügen')}
                        </span>
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={async () => {
                                const clip = await readFromClipboard();
                                if (!clip.error && clip.text.trim()) {
                                    setPastedText(clip.text);
                                }
                            }}
                        >
                            <ClipboardPaste size={12} className="mr-1" />
                            {t('common.pasteFromClipboard', 'Aus Zwischenablage einfügen')}
                        </Button>
                    </div>
                    <Textarea
                        rows={3}
                        value={pastedText}
                        onChange={(e) => setPastedText(e.target.value)}
                        placeholder={t('common.pastePlaceholder', 'z. B. humoco://pay?… oder ganze Chat-/E-Mail-Nachricht')}
                    />
                    <div className="flex justify-end gap-2">
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() => setShowPasteBox(false)}
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            type="button"
                            size="xs"
                            variant="primary"
                            disabled={!pastedText.trim() || isProcessing}
                            onClick={async () => {
                                const ok = await onPasteRawInput(pastedText);
                                if (ok) {
                                    setPastedText('');
                                    setShowPasteBox(false);
                                }
                            }}
                        >
                            {t('common.pasteOpenBtn', 'Öffnen')}
                        </Button>
                    </div>
                </div>
            )}

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
