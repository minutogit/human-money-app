// src/components/marketplace/ImportCardModal.tsx
import { useState, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Download, FileText, CheckCircle2, AlertCircle, Info, Upload } from 'lucide-react';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { Textarea } from '../ui/Textarea';
import { marketService } from '../../services/marketService';
import { useSession } from '../../context/SessionContext';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';

export interface ImportCardModalProps {
    isOpen: boolean;
    onClose: () => void;
    onImportSuccess: (did?: string | null) => void;
}

export function ImportCardModal({ isOpen, onClose, onImportSuccess }: ImportCardModalProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [payloadText, setPayloadText] = useState('');
    const [isImporting, setIsImporting] = useState(false);
    const [resultFeedback, setResultFeedback] = useState<{
        type: 'success' | 'warning' | 'error';
        title: string;
        message: string;
        did?: string | null;
    } | null>(null);

    const fileInputRef = useRef<HTMLInputElement>(null);

    const resetState = () => {
        setPayloadText('');
        setIsImporting(false);
        setResultFeedback(null);
    };

    const handleClose = () => {
        resetState();
        onClose();
    };

    const processImportPayload = async (rawPayload: string) => {
        const trimmed = rawPayload.trim();
        if (!trimmed) return;

        setIsImporting(true);
        setResultFeedback(null);

        try {
            const res = await protectAction(async (pwd) => {
                return await marketService.importContactCard(trimmed, pwd || undefined);
            });

            if (!res) {
                return;
            }

            if (res.status === 'accepted') {
                setResultFeedback({
                    type: 'success',
                    title: res.replaced
                        ? t('marketplace.importUpdatedTitle')
                        : t('marketplace.importSuccessTitle'),
                    message: res.replaced
                        ? t('marketplace.importUpdatedDesc')
                        : t('marketplace.importSuccessDesc'),
                    did: res.did,
                });
                onImportSuccess(res.did);
            } else if (res.status === 'ignored_stale') {
                setResultFeedback({
                    type: 'warning',
                    title: t('marketplace.importIgnoredTitle'),
                    message: t('marketplace.importIgnoredDesc'),
                });
            } else if (res.status === 'rejected_expired') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importExpiredTitle'),
                    message: t('marketplace.importExpiredDesc'),
                });
            } else if (res.status === 'rejected_hops') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importHopsTitle'),
                    message: t('marketplace.importHopsDesc'),
                });
            } else if (res.status === 'rejected_offender') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importOffenderTitle'),
                    message: t('marketplace.importOffenderDesc'),
                });
            } else {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importInvalidTitle'),
                    message: res.status,
                });
            }
        } catch (err) {
            logger.error(`Error importing contact card: ${stringifyError(err)}`);
            setResultFeedback({
                type: 'error',
                title: t('marketplace.importInvalidTitle'),
                message: stringifyError(err),
            });
        } finally {
            setIsImporting(false);
        }
    };

    const handleFilePicker = async () => {
        try {
            const selectedPath = await open({
                multiple: false,
                filters: [
                    { name: t('marketplace.cardFilterName'), extensions: ['hmcard', 'json'] },
                    { name: 'All Files', extensions: ['*'] },
                ],
            });

            if (typeof selectedPath === 'string') {
                const bytes = await readFile(selectedPath);
                const text = new TextDecoder().decode(bytes);
                setPayloadText(text);
                await processImportPayload(text);
            }
        } catch (err) {
            logger.info(`Tauri file dialog not available, falling back to input: ${stringifyError(err)}`);
            fileInputRef.current?.click();
        }
    };

    const handleFileInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        try {
            const text = await file.text();
            setPayloadText(text);
            await processImportPayload(text);
        } catch (err) {
            setResultFeedback({
                type: 'error',
                title: t('marketplace.importInvalidTitle'),
                message: stringifyError(err),
            });
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={handleClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <Download size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">
                            {t('marketplace.importModalTitle')}
                        </h2>
                        <p className="text-xs text-theme-light">
                            {t('marketplace.importModalSubtitle')}
                        </p>
                    </div>
                </div>
            }
            footer={
                <div className="flex items-center justify-between w-full">
                    <Button variant="secondary" size="sm" onClick={handleClose}>
                        {t('common.close')}
                    </Button>

                    <Button
                        variant="primary"
                        size="sm"
                        onClick={() => processImportPayload(payloadText)}
                        disabled={isImporting || !payloadText.trim()}
                        className="gap-1.5 shadow-sm"
                    >
                        <Download size={14} />
                        <span>{isImporting ? t('common.processing') : t('marketplace.importActionBtn')}</span>
                    </Button>
                </div>
            }
        >
            <div className="space-y-4 pt-2">
                {/* Result Feedback Banner */}
                {resultFeedback && (
                    <div
                        className={`p-3 rounded-2xl border flex items-start gap-3 text-xs ${
                            resultFeedback.type === 'success'
                                ? 'bg-emerald-50 text-emerald-900 border-emerald-200'
                                : resultFeedback.type === 'warning'
                                ? 'bg-amber-50 text-amber-900 border-amber-200'
                                : 'bg-rose-50 text-rose-900 border-rose-200'
                        }`}
                    >
                        {resultFeedback.type === 'success' ? (
                            <CheckCircle2 size={16} className="text-emerald-600 shrink-0 mt-0.5" />
                        ) : resultFeedback.type === 'warning' ? (
                            <Info size={16} className="text-amber-600 shrink-0 mt-0.5" />
                        ) : (
                            <AlertCircle size={16} className="text-rose-600 shrink-0 mt-0.5" />
                        )}
                        <div className="space-y-1">
                            <h4 className="font-bold">{resultFeedback.title}</h4>
                            <p className="leading-relaxed opacity-90">{resultFeedback.message}</p>
                        </div>
                    </div>
                )}

                {/* File Upload Option */}
                <div>
                    <input
                        ref={fileInputRef}
                        type="file"
                        accept=".hmcard,application/json"
                        onChange={handleFileInputChange}
                        className="hidden"
                    />
                    <Button
                        type="button"
                        variant="outline"
                        onClick={handleFilePicker}
                        disabled={isImporting}
                        className="w-full h-auto py-3 px-4 flex items-center justify-center gap-2 border-dashed border-2 border-theme-subtle hover:border-theme-primary/40 bg-bg-app rounded-2xl text-xs font-bold text-theme-secondary cursor-pointer"
                    >
                        <Upload size={16} className="text-theme-primary" />
                        <span>{t('marketplace.chooseCardFileBtn')}</span>
                    </Button>
                </div>

                <div className="relative flex items-center justify-center">
                    <div className="border-t border-theme-subtle w-full"></div>
                    <span className="bg-white px-3 text-[10px] font-bold text-theme-light uppercase tracking-wider absolute">
                        {t('common.or')}
                    </span>
                </div>

                {/* Paste Link or JSON */}
                <div className="space-y-1.5">
                    <label className="block text-[11px] font-bold text-theme-secondary flex items-center gap-1.5">
                        <FileText size={13} className="text-theme-primary" />
                        <span>{t('marketplace.pasteCardPayloadLabel')}</span>
                    </label>
                    <Textarea
                        value={payloadText}
                        onChange={(e) => setPayloadText(e.target.value)}
                        placeholder={t('marketplace.pasteCardPayloadPlaceholder')}
                        rows={4}
                        className="text-xs font-mono bg-bg-app"
                    />
                    <p className="text-[10px] text-theme-light">
                        {t('marketplace.pasteCardPayloadHint')}
                    </p>
                </div>
            </div>
        </ModalShell>
    );
}
