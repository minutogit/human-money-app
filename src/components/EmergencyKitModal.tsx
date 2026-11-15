// src/components/EmergencyKitModal.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { ModalShell } from './ui/ModalShell';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AlertBanner } from './ui/AlertBanner';
import { recoveryService } from '../services/recoveryService';
import { ExportedMnemonicKitDto } from '../types';
import { translateError } from '../utils/errorHelper';
import {
    FileText,
    Printer,
    Download,
    CheckCircle2,
    KeyRound,
    ArrowRight,
    Info,
} from 'lucide-react';

interface EmergencyKitModalProps {
    isOpen: boolean;
    onClose: () => void;
    onSuccess?: () => void;
}

type ModalStep = 'unlock' | 'view_sheet' | 'confirm_word' | 'success';

export const EmergencyKitModal: React.FC<EmergencyKitModalProps> = ({
    isOpen,
    onClose,
    onSuccess,
}) => {
    const { t } = useTranslation();
    const [step, setStep] = useState<ModalStep>('unlock');
    const [password, setPassword] = useState<string>('');
    const [kitData, setKitData] = useState<ExportedMnemonicKitDto | null>(null);
    const [isNotRetained, setIsNotRetained] = useState<boolean>(false);

    // Confirmation step state
    const [controlWordIndex, setControlWordIndex] = useState<number>(4); // Word #4 (1-based index)
    const [controlWordInput, setControlWordInput] = useState<string>('');

    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [errorMsg, setErrorMsg] = useState<string>('');

    useEffect(() => {
        if (!isOpen) {
            setStep('unlock');
            setPassword('');
            setKitData(null);
            setIsNotRetained(false);
            setControlWordInput('');
            setErrorMsg('');
            setIsLoading(false);
            return;
        }

        setStep('unlock');
        setPassword('');
        setKitData(null);
        setIsNotRetained(false);
        setControlWordInput('');
        setErrorMsg('');
        setIsLoading(false);
    }, [isOpen]);

    const handleUnlock = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!password || isLoading) return;

        setIsLoading(true);
        setErrorMsg('');

        try {
            const dto = await recoveryService.exportProfileMnemonic(password);
            if (!dto) {
                setIsNotRetained(true);
            } else {
                setKitData(dto);
                // Choose random word index between 1 and 12
                const rndIndex = Math.floor(Math.random() * 12) + 1;
                setControlWordIndex(rndIndex);
                setStep('view_sheet');
            }
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    };

    const handlePrint = () => {
        window.print();
    };

    const handleDownloadHtml = () => {
        if (!kitData) return;
        const words = kitData.phrase.split(' ');
        const wordsHtml = words
            .map(
                (w, i) =>
                    `<div style="display:inline-block;width:30%;margin:5px;padding:8px;border:1px solid #ccc;border-radius:6px;font-family:monospace;"><b>${i + 1}.</b> ${w}</div>`
            )
            .join('');

        const syncsHtml =
            kitData.syncUrls && kitData.syncUrls.length > 0
                ? kitData.syncUrls.map((s) => `<li>${s}</li>`).join('')
                : '<li>(Keine externen Sync-Dienste)</li>';

        const guardiansHtml =
            kitData.guardianNames && kitData.guardianNames.length > 0
                ? kitData.guardianNames.map((g) => `<li>${g}</li>`).join('')
                : '<li>(Keine Kontakte eingerichtet)</li>';

        const fullHtml = `<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>HuMoCo Notfall-Blatt</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 40px; color: #111; line-height: 1.5; }
.alert { background: #fee2e2; border: 2px solid #ef4444; color: #991b1b; padding: 16px; border-radius: 8px; font-weight: bold; text-align: center; margin-bottom: 24px; }
.section { margin-bottom: 24px; }
.words-grid { margin: 16px 0; }
.footer { font-size: 12px; color: #666; margin-top: 40px; border-top: 1px solid #eee; padding-top: 10px; }
</style>
</head>
<body>
<div class="alert">
  ${t('backup.emergencyKitRedAlert')}
</div>
<h2>HuMoCo Profil-Wiederherstellung (Notfall-Blatt)</h2>
<div class="section">
  <h3>12 Wiederherstellungswörter</h3>
  <div class="words-grid">${wordsHtml}</div>
</div>
<div class="section">
  <h3>Zugehörige Sync-Dienste</h3>
  <ul>${syncsHtml}</ul>
</div>
<div class="section">
  <h3>Eingerichtete Vertrauenskontakte</h3>
  <ul>${guardiansHtml}</ul>
</div>
<div class="section">
  <h3>Anleitung für den Ernstfall</h3>
  <ol>
    <li>${t('backup.emergencyKitInstructions1')}</li>
    <li>${t('backup.emergencyKitInstructions2')}</li>
    <li>${t('backup.emergencyKitInstructions3')}</li>
  </ol>
</div>
<div class="footer">Erstellt mit HuMoCo Human Money &bull; URI: ${kitData.kitUri}</div>
</body>
</html>`;

        const blob = new Blob([fullHtml], { type: 'text/html' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `humoco-notfallblatt-${Date.now()}.html`;
        a.click();
        URL.revokeObjectURL(url);
    };

    const handleConfirmWord = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!controlWordInput.trim() || isLoading) return;

        setIsLoading(true);
        setErrorMsg('');

        try {
            await recoveryService.confirmEmergencySheetSaved(
                password,
                controlWordIndex,
                controlWordInput.trim()
            );
            setStep('success');
            onSuccess?.();
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    };

    if (!isOpen) return null;

    const words = kitData?.phrase ? kitData.phrase.split(' ') : [];

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            title={t('backup.emergencyKitTitle')}
        >
            <div className="space-y-6">
                {errorMsg && (
                    <AlertBanner
                        variant="error"
                        onDismiss={() => setErrorMsg('')}
                    >
                        {errorMsg}
                    </AlertBanner>
                )}

                {/* Case 0: Mnemonic was not retained */}
                {isNotRetained && (
                    <div className="space-y-4 animate-in fade-in duration-300">
                        <div className="p-4 bg-amber-500/10 border border-amber-500/20 rounded-2xl flex items-start gap-3">
                            <Info className="text-amber-500 shrink-0 mt-0.5" size={20} />
                            <p className="text-xs text-theme-secondary font-medium leading-relaxed">
                                {t('backup.emergencyKitNotRetainedHint')}
                            </p>
                        </div>
                        <div className="flex justify-end">
                            <Button type="button" variant="primary" onClick={onClose}>
                                {t('common.close')}
                            </Button>
                        </div>
                    </div>
                )}

                {/* Step 1: Unlock with Password */}
                {!isNotRetained && step === 'unlock' && (
                    <form onSubmit={handleUnlock} className="space-y-4 animate-in fade-in duration-300">
                        <p className="text-xs text-theme-light">
                            {t('backup.emergencyKitDesc')}
                        </p>

                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                <KeyRound size={15} className="text-theme-primary" />
                                {t('auth.password')}
                            </label>
                            <Input
                                type="password"
                                value={password}
                                onChange={(e) => setPassword(e.target.value)}
                                placeholder={t('auth.passwordPlaceholder')}
                                className="text-xs"
                            />
                        </div>

                        <div className="flex justify-end gap-3 pt-2">
                            <Button type="button" variant="outline" onClick={onClose} disabled={isLoading}>
                                {t('common.cancel')}
                            </Button>
                            <Button
                                type="submit"
                                variant="primary"
                                disabled={!password || isLoading}
                                data-testid="unlock-emergency-kit-btn"
                                className="flex items-center gap-2"
                            >
                                <FileText size={16} />
                                {isLoading ? t('common.loading') : t('common.confirm')}
                            </Button>
                        </div>
                    </form>
                )}

                {/* Step 2: View Printable Sheet */}
                {!isNotRetained && step === 'view_sheet' && kitData && (
                    <div className="space-y-6 animate-in fade-in duration-300">
                        {/* Red warning alert */}
                        <div className="p-4 bg-rose-600 text-white rounded-2xl font-black text-xs text-center tracking-wide uppercase shadow-sm">
                            {t('backup.emergencyKitRedAlert')}
                        </div>

                        {/* Top action row */}
                        <div className="flex flex-wrap gap-2 justify-end">
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={handlePrint}
                                className="flex items-center gap-2 text-xs"
                            >
                                <Printer size={15} />
                                {t('backup.emergencyKitPrintButton')}
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={handleDownloadHtml}
                                className="flex items-center gap-2 text-xs"
                            >
                                <Download size={15} />
                                {t('backup.emergencyKitDownloadHtmlButton')}
                            </Button>
                        </div>

                        {/* Main sheet preview */}
                        <div className="p-5 bg-white text-slate-900 rounded-2xl border border-slate-200 shadow-sm space-y-5">
                            {/* QR and Title */}
                            <div className="flex flex-col sm:flex-row items-center gap-4 justify-between border-b border-slate-100 pb-4">
                                <div>
                                    <h3 className="font-black text-sm uppercase tracking-wider text-slate-900">
                                        HuMoCo Notfall-Blatt
                                    </h3>
                                    <p className="text-xs text-slate-500 mt-0.5">
                                        {t('backup.emergencyKitScanQr')}
                                    </p>
                                </div>
                                {kitData.qrSvg && (
                                    <div
                                        className="w-24 h-24 shrink-0"
                                        dangerouslySetInnerHTML={{ __html: kitData.qrSvg }}
                                    />
                                )}
                            </div>

                            {/* 12 Cleartext Words */}
                            <div className="space-y-2">
                                <span className="text-[11px] font-bold uppercase tracking-wider text-slate-600 block">
                                    {t('backup.restoreSheetManualWordsLabel')}
                                </span>
                                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                                    {words.map((word, idx) => (
                                        <div
                                            key={idx}
                                            className="p-2 bg-slate-50 border border-slate-200 rounded-lg text-xs font-mono font-bold flex items-center justify-between"
                                        >
                                            <span className="text-slate-400 text-[10px]">#{idx + 1}</span>
                                            <span className="text-slate-900">{word}</span>
                                        </div>
                                    ))}
                                </div>
                            </div>

                            {/* Sync URLs & Guardians */}
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-xs pt-2 border-t border-slate-100">
                                <div>
                                    <span className="font-bold text-slate-700 block mb-1">
                                        {t('backup.emergencyKitSyncUrls')}
                                    </span>
                                    <ul className="space-y-0.5 text-slate-600 font-mono text-[11px]">
                                        {kitData.syncUrls && kitData.syncUrls.length > 0 ? (
                                            kitData.syncUrls.map((s, i) => <li key={i} className="truncate">{s}</li>)
                                        ) : (
                                            <li className="italic text-slate-400">-</li>
                                        )}
                                    </ul>
                                </div>
                                <div>
                                    <span className="font-bold text-slate-700 block mb-1">
                                        {t('backup.emergencyKitGuardianNames')}
                                    </span>
                                    <ul className="space-y-0.5 text-slate-600 text-[11px]">
                                        {kitData.guardianNames && kitData.guardianNames.length > 0 ? (
                                            kitData.guardianNames.map((g, i) => <li key={i}>{g}</li>)
                                        ) : (
                                            <li className="italic text-slate-400">-</li>
                                        )}
                                    </ul>
                                </div>
                            </div>
                        </div>

                        {/* Continue to confirmation step */}
                        <div className="flex justify-between items-center pt-2">
                            <Button type="button" variant="outline" onClick={onClose}>
                                {t('common.cancel')}
                            </Button>
                            <Button
                                type="button"
                                variant="primary"
                                onClick={() => setStep('confirm_word')}
                                data-testid="proceed-confirm-sheet-btn"
                                className="flex items-center gap-2"
                            >
                                <span>{t('backup.emergencyKitConfirmationTitle')}</span>
                                <ArrowRight size={16} />
                            </Button>
                        </div>
                    </div>
                )}

                {/* Step 3: Confirm 1 Word */}
                {!isNotRetained && step === 'confirm_word' && (
                    <form onSubmit={handleConfirmWord} className="space-y-4 animate-in fade-in duration-300">
                        <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-3">
                            <label className="text-xs font-bold text-theme-secondary block">
                                {t('backup.emergencyKitConfirmationPrompt', { index: controlWordIndex })}
                            </label>
                            <Input
                                type="text"
                                value={controlWordInput}
                                onChange={(e) => setControlWordInput(e.target.value)}
                                placeholder="z. B. apple"
                                data-testid="control-word-input"
                                className="text-xs font-mono"
                                autoFocus
                            />
                        </div>

                        <div className="flex justify-between items-center pt-2">
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => setStep('view_sheet')}
                                disabled={isLoading}
                            >
                                {t('common.back')}
                            </Button>
                            <Button
                                type="submit"
                                variant="primary"
                                disabled={!controlWordInput.trim() || isLoading}
                                data-testid="submit-confirm-word-btn"
                                className="flex items-center gap-2"
                            >
                                <CheckCircle2 size={16} />
                                {isLoading ? t('common.loading') : t('common.confirm')}
                            </Button>
                        </div>
                    </form>
                )}

                {/* Step 4: Success confirmation */}
                {!isNotRetained && step === 'success' && (
                    <div className="space-y-4 text-center py-4 animate-in fade-in duration-300">
                        <div className="w-12 h-12 rounded-full bg-emerald-500/10 text-emerald-500 mx-auto flex items-center justify-center">
                            <CheckCircle2 size={28} />
                        </div>
                        <h4 className="text-sm font-bold text-theme-secondary">
                            {t('backup.emergencyKitConfirmedSuccess')}
                        </h4>
                        <div className="flex justify-center pt-2">
                            <Button type="button" variant="primary" onClick={onClose}>
                                {t('common.done', 'Fertig')}
                            </Button>
                        </div>
                    </div>
                )}
            </div>
        </ModalShell>
    );
};
