// src/components/RestoreBackupView.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { AuthLayout } from './AuthLayout';
import { Card } from './ui/Card';
import { Button } from './ui/Button';
import { BackButton } from './ui/BackButton';
import { AlertBanner } from './ui/AlertBanner';
import { Input } from './ui/Input';
import { Textarea } from './ui/Textarea';
import { backupService } from '../services/backupService';
import { authService } from '../services/authService';
import { BackupProbeDto, BackupInspectionDto, RestoreOutcomeDto, MnemonicLanguage } from '../types';
import { cleanSeedText } from '../utils/seedUtils';
import { translateError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';
import { 
    FolderSync, 
    FileArchive, 
    KeyRound, 
    Lock, 
    CheckCircle2, 
    ShieldCheck, 
    ArrowRight, 
    RefreshCw,
    Folder,
    FileText,
    History,
    Layers,
    Hash,
    Server,
    Search,
    AlertTriangle
} from 'lucide-react';

export interface RestoreBackupViewProps {
    onRestoreSuccess: (outcome: RestoreOutcomeDto) => void;
    onSwitchToRecreate: () => void;
    onBack: () => void;
}

type WizardStep = 'select_source' | 'drop_source' | 'unlock_and_inspect' | 'completed';

export const RestoreBackupView: React.FC<RestoreBackupViewProps> = ({
    onRestoreSuccess,
    onSwitchToRecreate,
    onBack,
}) => {
    const { t } = useTranslation();

    const [step, setStep] = useState<WizardStep>('select_source');
    const [sourcePath, setSourcePath] = useState<string>('');
    const [probe, setProbe] = useState<BackupProbeDto | null>(null);

    // Drop restore state
    const [dropInput, setDropInput] = useState('');
    const [isFetchingDrop, setIsFetchingDrop] = useState(false);
    const [matchedDropKeyKind, setMatchedDropKeyKind] = useState<string | null>(null);
    const [isReconcilingL2, setIsReconcilingL2] = useState(false);

    // Credential state
    const [credentialMode, setCredentialMode] = useState<'password' | 'mnemonic'>('password');
    const [backupPassword, setBackupPassword] = useState('');
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>('english');
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [mnemonicWords, setMnemonicWords] = useState<string[]>(Array(12).fill(''));
    const [inputMode, setInputMode] = useState<'words' | 'phrase'>('words');
    const [rawPhrase, setRawPhrase] = useState('');
    const [passphrase, setPassphrase] = useState('');
    const [userPrefix, setUserPrefix] = useState('');

    // Inspection state
    const [isInspecting, setIsInspecting] = useState(false);
    const [inspection, setInspection] = useState<BackupInspectionDto | null>(null);

    // New device password state
    const [newPassword, setNewPassword] = useState('');
    const [confirmNewPassword, setConfirmNewPassword] = useState('');
    const [isRestoring, setIsRestoring] = useState(false);
    const [restoreOutcome, setRestoreOutcome] = useState<RestoreOutcomeDto | null>(null);

    // Feedback
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success'; message: string } | null>(null);

    // Step 1: Probe a file or folder
    const handleSelectFile = async () => {
        setFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                directory: false,
                filters: [{ name: 'Human Money Backup', extensions: ['hmcbackup'] }],
            });

            if (!selected) return;
            const path = typeof selected === 'string' ? selected : selected[0];
            await probeAndAdvance(path);
        } catch (err) {
            logger.error(`Failed to open backup file: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleSelectFolder = async () => {
        setFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                directory: true,
            });

            if (!selected) return;
            const path = typeof selected === 'string' ? selected : selected[0];
            await probeAndAdvance(path);
        } catch (err) {
            logger.error(`Failed to open backup folder: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const probeAndAdvance = async (path: string) => {
        setFeedback(null);
        try {
            const probeResult = await backupService.probeBackup(path);
            setSourcePath(path);
            setProbe(probeResult);
            // Default credential mode based on what the probe supports
            if (probeResult.supportsPasswordRestore) {
                setCredentialMode('password');
            } else {
                setCredentialMode('mnemonic');
            }
            setStep('unlock_and_inspect');
        } catch (err) {
            logger.error(`Failed to probe backup at ${path}: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    // Word helpers for mnemonic input
    const handleWordCountChange = (count: 12 | 24) => {
        setWordCount(count);
        setMnemonicWords((prev) => {
            const current = prev.join(' ').split(' ').filter(Boolean);
            return Array(count).fill('').map((_, i) => current[i] || '');
        });
    };

    const handleWordChange = (index: number, val: string) => {
        const cleaned = cleanSeedText(val);
        const parts = cleaned.split(' ').filter(Boolean);
        const next = [...mnemonicWords];

        if (parts.length > 1) {
            for (let i = 0; i < parts.length; i++) {
                if (index + i < wordCount) {
                    next[index + i] = parts[i];
                }
            }
            setMnemonicWords(next);
            const nextIdx = index + parts.length;
            if (nextIdx < wordCount) {
                document.getElementById(`restore-word-${nextIdx}`)?.focus();
            }
        } else {
            next[index] = cleaned;
            setMnemonicWords(next);
        }
    };

    const getFullMnemonic = (): string => {
        if (inputMode === 'phrase') {
            return cleanSeedText(rawPhrase);
        }
        return mnemonicWords.map((w) => w.trim()).filter(Boolean).join(' ');
    };

    // Step 2: Inspect Backup
    const handleInspectBackup = async () => {
        setFeedback(null);

        if (credentialMode === 'password') {
            if (!backupPassword) {
                setFeedback({ type: 'error', message: t('settings.backupPasswordMinLength') });
                return;
            }
        } else {
            const mnemonic = getFullMnemonic();
            const words = mnemonic.split(' ').filter(Boolean);
            if (words.length !== 12 && words.length !== 24) {
                setFeedback({ type: 'error', message: t('auth.enterValidSeed') });
                return;
            }
        }

        setIsInspecting(true);
        try {
            const result = await backupService.inspectBackup({
                sourcePath,
                credentialMode,
                backupPassword: credentialMode === 'password' ? backupPassword : null,
                mnemonic: credentialMode === 'mnemonic' ? getFullMnemonic() : null,
                passphrase: credentialMode === 'mnemonic' && passphrase ? passphrase : null,
                userPrefix: credentialMode === 'mnemonic' && userPrefix ? userPrefix : null,
                language: credentialMode === 'mnemonic' ? selectedLanguage : null,
            });

            setInspection(result);
            logger.info(`Backup inspected successfully: ${result.profileName}`);
        } catch (err) {
            logger.error(`Inspect backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsInspecting(false);
        }
    };

    const handleFetchAndInspectDropBackup = async () => {
        setFeedback(null);
        const mnemonic = getFullMnemonic();
        const words = mnemonic.split(' ').filter(Boolean);
        if (words.length !== 12 && words.length !== 24) {
            setFeedback({ type: 'error', message: t('auth.enterValidSeed') });
            return;
        }
        if (!dropInput.trim()) {
            setFeedback({ type: 'error', message: t('profile.backupRestoreDropAddressRequired', 'Bitte gib eine Ablage-Adresse oder einen Deskriptor ein.') });
            return;
        }

        setIsFetchingDrop(true);
        try {
            const result = await backupService.fetchAndInspectDropBackup({
                dropInput: dropInput.trim(),
                mnemonic,
                passphrase: passphrase || null,
                userPrefix: userPrefix || null,
                language: selectedLanguage,
            });

            setSourcePath(result.stagedPath);
            setInspection(result.inspection);
            setMatchedDropKeyKind(result.matchedKeyKind);
            setCredentialMode('mnemonic');
            setProbe({
                sourceKind: 'file',
                protectionMode: result.inspection.protectionMode,
                supportsPasswordRestore: false,
                supportsMnemonicRestore: true,
                blobCount: result.inspection.blobCount,
                customStandardCount: result.inspection.customStandardCount,
            });
            setStep('unlock_and_inspect');
        } catch (err) {
            logger.error(`Fetch drop backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsFetchingDrop(false);
        }
    };

    const handleReconcileL2 = async () => {
        setIsReconcilingL2(true);
        setFeedback(null);
        try {
            const res = await backupService.reconcileRestoredWalletL2(newPassword || undefined);
            if (restoreOutcome) {
                setRestoreOutcome({
                    ...restoreOutcome,
                    reconciledSpentCount: res.reconciledSpentCount,
                    l2ReconciliationStatus: res.l2ReconciliationStatus,
                });
            }
            if (res.l2ReconciliationStatus === 'reconciled') {
                setFeedback({
                    type: 'success',
                    message: t('profile.backupRestoreReconcileSuccess', 'Online-Abgleich erfolgreich! Alle Gutscheine und Siegel wurden geprüft.'),
                });
            } else {
                setFeedback({
                    type: 'error',
                    message: t('profile.backupRestoreReconcileOffline', 'Netzwerk nicht erreichbar. Bitte prüfe deine Internetverbindung.'),
                });
            }
        } catch (err) {
            logger.error(`Reconcile L2 failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsReconcilingL2(false);
        }
    };

    // Step 3: Set Local Password & Restore
    const handleRestoreSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setFeedback(null);

        if (newPassword.length < 8) {
            setFeedback({ type: 'error', message: t('profile.passwordMinLength') });
            return;
        }

        if (newPassword !== confirmNewPassword) {
            setFeedback({ type: 'error', message: t('profile.passwordMismatch') });
            return;
        }

        setIsRestoring(true);
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            const outcome = await backupService.restoreBackup({
                sourcePath,
                credentialMode,
                backupPassword: credentialMode === 'password' ? backupPassword : null,
                mnemonic: credentialMode === 'mnemonic' ? getFullMnemonic() : null,
                passphrase: credentialMode === 'mnemonic' && passphrase ? passphrase : null,
                userPrefix: credentialMode === 'mnemonic' && userPrefix ? userPrefix : null,
                language: credentialMode === 'mnemonic' ? selectedLanguage : null,
                newPassword,
                localInstanceId,
            });

            setRestoreOutcome(outcome);
            setStep('completed');
            logger.info(`Wallet restored successfully: ${JSON.stringify(outcome)}`);
        } catch (err) {
            logger.error(`Restore backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsRestoring(false);
        }
    };

    return (
        <AuthLayout maxWidth="max-w-3xl">
            {/* Header */}
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2 border-b border-theme-subtle">
                <div
                    data-testid="restore-back-btn"
                    onClick={(e) => {
                        if (e.target === e.currentTarget) {
                            if (step === 'unlock_and_inspect' || step === 'drop_source') {
                                setStep('select_source');
                            } else {
                                onBack();
                            }
                        }
                    }}
                >
                    <BackButton
                        onClick={step === 'unlock_and_inspect' || step === 'drop_source' ? () => setStep('select_source') : onBack}
                        label={t('common.back')}
                    />
                </div>
                <div className="text-right">
                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-primary">
                        Human Money Backup
                    </span>
                </div>
            </div>

            {/* Title */}
            <div className="text-center space-y-1">
                <h1 className="text-2xl font-black text-theme-secondary tracking-tight">
                    {t('profile.backupRestoreTitle')}
                </h1>
                <p className="text-xs text-theme-light font-medium">
                    {t('profile.backupRestoreSubtitle')}
                </p>
            </div>

            {/* Top Choice Link: Recreate empty identity instead */}
            {step !== 'completed' && (
                <div className="p-3.5 bg-slate-50 border border-slate-200 rounded-2xl flex items-center justify-between gap-3 text-xs">
                    <span className="text-theme-light font-medium">
                        {t('profile.backupRestoreNoFileLink')}
                    </span>
                    <Button
                        variant="secondary"
                        size="xs"
                        onClick={onSwitchToRecreate}
                        className="shrink-0"
                    >
                        <RefreshCw size={12} className="mr-1" />
                        {t('profile.backupRestoreRecreateEmptyBtn')}
                    </Button>
                </div>
            )}

            {/* Feedback Alert */}
            {feedback && (
                <AlertBanner variant={feedback.type}>
                    {feedback.message}
                </AlertBanner>
            )}

            {/* STEP 1: Select Backup Source */}
            {step === 'select_source' && (
                <div className="space-y-4">
                    <div className="text-center">
                        <h2 className="text-sm font-black uppercase tracking-widest text-theme-secondary">
                            {t('profile.backupRestoreSourceTitle')}
                        </h2>
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-2">
                        {/* File Option */}
                        <button
                            type="button"
                            onClick={handleSelectFile}
                            className="p-6 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex flex-col justify-between gap-4 cursor-pointer group shadow-sm hover:shadow-md"
                        >
                            <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                <FileArchive size={24} />
                            </div>
                            <div className="space-y-1">
                                <h3 className="text-sm font-black text-theme-secondary group-hover:text-theme-primary">
                                    {t('profile.backupRestoreSelectFile')}
                                </h3>
                                <p className="text-xs text-theme-light font-medium leading-relaxed">
                                    {t('profile.backupRestoreSelectFileDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1 text-[11px] font-bold text-theme-primary">
                                <span>Browse file (.hmcbackup)</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        {/* Folder Option */}
                        <button
                            type="button"
                            onClick={handleSelectFolder}
                            className="p-6 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex flex-col justify-between gap-4 cursor-pointer group shadow-sm hover:shadow-md"
                        >
                            <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                <FolderSync size={24} />
                            </div>
                            <div className="space-y-1">
                                <h3 className="text-sm font-black text-theme-secondary group-hover:text-theme-primary">
                                    {t('profile.backupRestoreSelectFolder')}
                                </h3>
                                <p className="text-xs text-theme-light font-medium leading-relaxed">
                                    {t('profile.backupRestoreSelectFolderDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1 text-[11px] font-bold text-theme-primary">
                                <span>Select directory</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        {/* Drop Option */}
                        <button
                            type="button"
                            data-testid="restore-select-drop-btn"
                            onClick={() => {
                                setFeedback(null);
                                setCredentialMode('mnemonic');
                                setStep('drop_source');
                            }}
                            className="p-6 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex flex-col justify-between gap-4 cursor-pointer group shadow-sm hover:shadow-md"
                        >
                            <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                <Server size={24} />
                            </div>
                            <div className="space-y-1">
                                <h3 className="text-sm font-black text-theme-secondary group-hover:text-theme-primary">
                                    {t('profile.backupRestoreSelectDrop', 'Von Ablage wiederherstellen')}
                                </h3>
                                <p className="text-xs text-theme-light font-medium leading-relaxed">
                                    {t('profile.backupRestoreSelectDropDesc', 'Freundes-, eigene oder öffentliche Ablage mit Mnemonic abrufen')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1 text-[11px] font-bold text-theme-primary">
                                <span>{t('profile.backupRestoreSelectDropAction', 'Ablage abrufen')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>
                    </div>
                </div>
            )}

            {/* STEP 1b: Drop Source (Von Ablage wiederherstellen) */}
            {step === 'drop_source' && (
                <div className="space-y-6">
                    <div className="p-4 bg-theme-primary/5 border border-theme-primary/20 rounded-2xl flex items-center justify-between gap-4">
                        <div className="flex items-center gap-3 min-w-0">
                            <Server size={20} className="text-theme-primary shrink-0" />
                            <div className="min-w-0">
                                <p className="text-[10px] font-black uppercase tracking-wider text-theme-primary">
                                    {t('profile.backupRestoreDropHeader', 'Von Ablage wiederherstellen')}
                                </p>
                                <p className="text-xs text-theme-secondary font-medium">
                                    {t('profile.backupRestoreDropSubtitle', 'Gib deine Mnemonic-Wörter und die Adresse der Ablage ein.')}
                                </p>
                            </div>
                        </div>
                        <Button
                            variant="secondary"
                            size="xs"
                            onClick={() => setStep('select_source')}
                        >
                            {t('common.change', 'Ändern')}
                        </Button>
                    </div>

                    {/* Mnemonic Inputs */}
                    <div className="space-y-4 bg-white p-6 rounded-3xl border border-theme-subtle shadow-sm">
                        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-subtle pb-3">
                            <div className="flex items-center gap-2">
                                <KeyRound size={16} className="text-theme-primary" />
                                <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                                    {t('profile.backupRestoreMnemonicTitle', 'Wiederherstellungs-Wörter (Mnemonic)')}
                                </h3>
                            </div>
                            <div className="flex items-center gap-2">
                                <select
                                    value={selectedLanguage}
                                    onChange={(e) => setSelectedLanguage(e.target.value as MnemonicLanguage)}
                                    className="text-xs bg-slate-50 border border-slate-200 rounded-lg px-2 py-1 font-medium text-theme-secondary"
                                >
                                    <option value="english">English</option>
                                    <option value="german">Deutsch</option>
                                    <option value="spanish">Español</option>
                                    <option value="french">Français</option>
                                    <option value="italian">Italiano</option>
                                </select>
                                <div className="flex bg-slate-100 p-0.5 rounded-lg text-[11px] font-bold">
                                    <button
                                        type="button"
                                        onClick={() => handleWordCountChange(12)}
                                        className={`px-2 py-0.5 rounded ${wordCount === 12 ? 'bg-white shadow-xs text-theme-primary' : 'text-theme-light'}`}
                                    >
                                        12
                                    </button>
                                    <button
                                        type="button"
                                        onClick={() => handleWordCountChange(24)}
                                        className={`px-2 py-0.5 rounded ${wordCount === 24 ? 'bg-white shadow-xs text-theme-primary' : 'text-theme-light'}`}
                                    >
                                        24
                                    </button>
                                </div>
                            </div>
                        </div>

                        {/* Words grid */}
                        <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2 pt-1">
                            {mnemonicWords.map((word, idx) => (
                                <div key={idx} className="relative flex items-center">
                                    <span className="absolute left-2.5 text-[10px] font-mono text-slate-400 select-none">
                                        {idx + 1}.
                                    </span>
                                    <input
                                        id={`restore-drop-word-${idx}`}
                                        data-testid={`restore-drop-word-${idx}`}
                                        type="text"
                                        value={word}
                                        onChange={(e) => handleWordChange(idx, e.target.value)}
                                        className="w-full pl-7 pr-2 py-1.5 text-xs font-mono bg-slate-50 border border-slate-200 focus:bg-white focus:border-theme-primary rounded-xl outline-hidden transition-all"
                                        autoCapitalize="none"
                                        autoCorrect="off"
                                    />
                                </div>
                            ))}
                        </div>

                        {/* Drop Address Input */}
                        <div className="space-y-1.5 pt-3 border-t border-theme-subtle">
                            <label className="text-xs font-bold text-theme-secondary flex items-center justify-between">
                                <span>{t('profile.backupRestoreDropAddressLabel', 'Ablage-Adresse oder Deskriptor')}</span>
                            </label>
                            <Input
                                data-testid="restore-drop-input"
                                type="text"
                                placeholder={t('profile.backupRestoreDropAddressPlaceholder', 'z. B. ablage.verein.de, https://freund.de/drop.php oder JSON-Deskriptor')}
                                value={dropInput}
                                onChange={(e) => setDropInput(e.target.value)}
                                disabled={isFetchingDrop}
                            />
                            <p className="text-[11px] text-theme-light">
                                {t('profile.backupRestoreDropAddressHint', 'Webspace-Adresse einer eigenen, Freundes- oder öffentlichen Ablage.')}
                            </p>
                        </div>

                        <Button
                            data-testid="restore-drop-fetch-btn"
                            variant="primary"
                            onClick={handleFetchAndInspectDropBackup}
                            disabled={isFetchingDrop}
                            className="w-full mt-2"
                        >
                            {isFetchingDrop ? (
                                <>
                                    <RefreshCw size={14} className="animate-spin mr-2" />
                                    {t('profile.backupRestoreFetchingDrop', 'Ablage wird durchsucht...')}
                                </>
                            ) : (
                                <>
                                    <Search size={14} className="mr-2" />
                                    {t('profile.backupRestoreFetchDropBtn', 'Sicherung suchen & laden')}
                                </>
                            )}
                        </Button>
                    </div>
                </div>
            )}

            {/* STEP 2 & 3: Unlock & Inspect, and Device Password */}
            {step === 'unlock_and_inspect' && (
                <div className="space-y-6">
                    {/* Source Selected Banner */}
                    <div className="p-4 bg-slate-50 border border-slate-200 rounded-2xl flex items-center justify-between gap-4">
                        <div className="flex items-center gap-3 min-w-0">
                            {matchedDropKeyKind ? (
                                <Server size={18} className="text-theme-primary shrink-0" />
                            ) : (
                                <Folder size={18} className="text-theme-primary shrink-0" />
                            )}
                            <div className="min-w-0">
                                <p className="text-[10px] font-black uppercase tracking-wider text-theme-light flex items-center gap-2">
                                    <span>{matchedDropKeyKind ? 'Ablage-Sicherung' : (probe?.sourceKind === 'folder' ? 'Backup Folder' : 'Backup Archive')}</span>
                                    {matchedDropKeyKind && (
                                        <span className="px-2 py-0.5 rounded-full bg-theme-primary/10 text-theme-primary font-bold">
                                            {matchedDropKeyKind === 'stealth' && 'Stealth-Schlüssel'}
                                            {matchedDropKeyKind === 'public' && 'Öffentliche Ablage (R)'}
                                            {matchedDropKeyKind === 'grace' && 'Grace-Schlüssel'}
                                            {matchedDropKeyKind === 'own' && 'Eigene Ablage'}
                                        </span>
                                    )}
                                </p>
                                <p className="text-xs font-mono text-theme-secondary truncate max-w-md">
                                    {sourcePath}
                                </p>
                            </div>
                        </div>
                        <Button
                            variant="secondary"
                            size="xs"
                            onClick={() => {
                                setInspection(null);
                                setStep('select_source');
                            }}
                        >
                            Change
                        </Button>
                    </div>

                    {/* Step 2 Section: Credential Unlock */}
                    <div className="space-y-4">
                        <div className="flex items-center justify-between">
                            <h2 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                                <KeyRound size={16} className="text-theme-primary" />
                                {t('profile.backupRestoreUnlockTitle')}
                            </h2>

                            {/* Tabs if password is supported */}
                            {probe?.supportsPasswordRestore && (
                                <div className="flex bg-slate-100 p-1 rounded-xl">
                                    <button
                                        type="button"
                                        onClick={() => {
                                            setCredentialMode('password');
                                            setInspection(null);
                                        }}
                                        className={`px-3 py-1 text-xs font-bold rounded-lg transition-all ${
                                            credentialMode === 'password'
                                                ? 'bg-white text-theme-primary shadow-sm'
                                                : 'text-theme-light hover:text-theme-secondary'
                                        }`}
                                    >
                                        {t('profile.backupRestorePasswordTab')}
                                    </button>
                                    <button
                                        type="button"
                                        onClick={() => {
                                            setCredentialMode('mnemonic');
                                            setInspection(null);
                                        }}
                                        className={`px-3 py-1 text-xs font-bold rounded-lg transition-all ${
                                            credentialMode === 'mnemonic'
                                                ? 'bg-white text-theme-primary shadow-sm'
                                                : 'text-theme-light hover:text-theme-secondary'
                                        }`}
                                    >
                                        {t('profile.backupRestoreMnemonicTab')}
                                    </button>
                                </div>
                            )}
                        </div>

                        {/* Password credential input */}
                        {credentialMode === 'password' && (
                            <div className="space-y-3 p-4 bg-white border border-theme-subtle rounded-2xl">
                                <label className="block text-xs font-bold text-theme-secondary">
                                    {t('settings.backupPasswordLabel')}
                                </label>
                                <Input
                                    data-testid="backup-password-input"
                                    type="password"
                                    value={backupPassword}
                                    onChange={(e) => setBackupPassword(e.target.value)}
                                    placeholder={t('settings.backupPasswordPlaceholder')}
                                    className="font-mono"
                                />
                            </div>
                        )}

                        {/* Mnemonic credential input */}
                        {credentialMode === 'mnemonic' && (
                            <div className="space-y-4 p-4 bg-white border border-theme-subtle rounded-2xl">
                                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-subtle pb-3">
                                    <div className="flex items-center gap-2">
                                        <select
                                            value={selectedLanguage}
                                            onChange={(e) => setSelectedLanguage(e.target.value as MnemonicLanguage)}
                                            className="px-2.5 py-1 text-xs border border-theme-subtle rounded-lg bg-bg-input text-theme-secondary focus:ring-2 focus:ring-theme-primary"
                                        >
                                            <option value="english">English</option>
                                            <option value="german">Deutsch</option>
                                            <option value="spanish">Español</option>
                                            <option value="french">Français</option>
                                            <option value="italian">Italiano</option>
                                        </select>
                                        <select
                                            value={wordCount}
                                            onChange={(e) => handleWordCountChange(Number(e.target.value) as 12 | 24)}
                                            className="px-2.5 py-1 text-xs border border-theme-subtle rounded-lg bg-bg-input text-theme-secondary focus:ring-2 focus:ring-theme-primary"
                                        >
                                            <option value={12}>{t('profile.wordCount12')}</option>
                                            <option value={24}>{t('profile.wordCount24')}</option>
                                        </select>
                                    </div>
                                    <button
                                        type="button"
                                        onClick={() => setInputMode(inputMode === 'words' ? 'phrase' : 'words')}
                                        className="text-xs font-semibold text-theme-primary hover:underline"
                                    >
                                        {inputMode === 'words' ? t('auth.enterFullPhrase') : t('auth.useSingleFields')}
                                    </button>
                                </div>

                                {inputMode === 'words' ? (
                                    <div className="grid grid-cols-3 sm:grid-cols-4 gap-2.5">
                                        {mnemonicWords.map((word, index) => (
                                            <div key={index} className="relative">
                                                <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[10px] text-theme-light select-none">
                                                    {index + 1}.
                                                </span>
                                                <Input
                                                    id={`restore-word-${index}`}
                                                    type="text"
                                                    value={word}
                                                    onChange={(e) => handleWordChange(index, e.target.value)}
                                                    className="pl-7 text-xs font-mono"
                                                    autoComplete="off"
                                                />
                                            </div>
                                        ))}
                                    </div>
                                ) : (
                                    <Textarea
                                        value={rawPhrase}
                                        onChange={(e) => setRawPhrase(e.target.value)}
                                        placeholder={t('profile.pasteSeedPhrasePlaceholder')}
                                        rows={4}
                                        className="font-mono text-xs"
                                    />
                                )}

                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            {t('profile.optionalPassphraseLabel')}
                                        </label>
                                        <Input
                                            type="password"
                                            value={passphrase}
                                            onChange={(e) => setPassphrase(e.target.value)}
                                            placeholder="Optional BIP-39 passphrase"
                                            className="text-xs"
                                        />
                                    </div>
                                    <div>
                                        <label className="block text-[11px] font-bold text-theme-secondary mb-1">
                                            User / Device Prefix (Optional)
                                        </label>
                                        <Input
                                            type="text"
                                            value={userPrefix}
                                            onChange={(e) => setUserPrefix(e.target.value)}
                                            placeholder="Leave blank to use manifest"
                                            className="text-xs font-mono"
                                        />
                                    </div>
                                </div>
                            </div>
                        )}

                        {/* Verify button */}
                        {!inspection && (
                            <Button
                                variant="primary"
                                onClick={handleInspectBackup}
                                disabled={isInspecting}
                                className="w-full"
                            >
                                {isInspecting ? (
                                    <>
                                        <RefreshCw size={14} className="animate-spin mr-2" />
                                        {t('profile.backupRestoreVerifying')}
                                    </>
                                ) : (
                                    <>
                                        <ShieldCheck size={16} className="mr-2" />
                                        {t('profile.backupRestoreVerifyButton')}
                                    </>
                                )}
                            </Button>
                        )}
                    </div>

                    {/* Verified Manifest Preview Card */}
                    {inspection && (
                        <div className="space-y-6 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <Card className="border-emerald-200 bg-emerald-50/50">
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between">
                                        <div className="flex items-center gap-2">
                                            <span className="px-2.5 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-emerald-600 text-white flex items-center gap-1.5 shadow-sm">
                                                <CheckCircle2 size={12} />
                                                {t('profile.backupRestoreVerifiedBadge')}
                                            </span>
                                        </div>
                                        <span className="text-xs text-emerald-800 font-mono">
                                            {new Date(inspection.sealTimestamp).toLocaleString()}
                                        </span>
                                    </div>

                                    <div>
                                        <h3 className="text-base font-black text-theme-secondary">
                                            {inspection.profileName}
                                        </h3>
                                        <p className="text-[11px] font-mono text-theme-light truncate">
                                            User ID: {inspection.userId}
                                        </p>
                                    </div>

                                    {/* Stats grid */}
                                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2">
                                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                                            <FileText size={16} className="text-emerald-600 shrink-0" />
                                            <div>
                                                <p className="text-xs font-black text-theme-secondary">
                                                    {inspection.blobCount}
                                                </p>
                                                <p className="text-[10px] text-theme-light">Data Blobs</p>
                                            </div>
                                        </div>
                                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                                            <History size={16} className="text-emerald-600 shrink-0" />
                                            <div>
                                                <p className="text-xs font-black text-theme-secondary">
                                                    {inspection.epoch}
                                                </p>
                                                <p className="text-[10px] text-theme-light">Epoch</p>
                                            </div>
                                        </div>
                                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                                            <Hash size={16} className="text-emerald-600 shrink-0" />
                                            <div>
                                                <p className="text-xs font-black text-theme-secondary">
                                                    {inspection.txNonce}
                                                </p>
                                                <p className="text-[10px] text-theme-light">Tx Nonce</p>
                                            </div>
                                        </div>
                                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                                            <Layers size={16} className="text-emerald-600 shrink-0" />
                                            <div>
                                                <p className="text-xs font-black text-theme-secondary">
                                                    {inspection.customStandardCount}
                                                </p>
                                                <p className="text-[10px] text-theme-light">Standards</p>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </Card>

                            {/* STEP 3: Set Local Device Password & Restore */}
                            <form onSubmit={handleRestoreSubmit} className="space-y-4 p-5 bg-white border border-theme-subtle rounded-3xl shadow-sm">
                                <div className="space-y-1">
                                    <h3 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                                        <Lock size={16} className="text-theme-primary" />
                                        {t('profile.backupRestoreDevicePasswordTitle')}
                                    </h3>
                                    <p className="text-xs text-theme-light">
                                        {t('profile.backupRestoreDevicePasswordDesc')}
                                    </p>
                                </div>

                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2">
                                    <div>
                                        <label className="block text-xs font-bold text-theme-secondary mb-1">
                                            {t('profile.backupRestoreNewPasswordLabel')}
                                        </label>
                                        <Input
                                            data-testid="new-device-password-input"
                                            type="password"
                                            value={newPassword}
                                            onChange={(e) => setNewPassword(e.target.value)}
                                            placeholder="Min. 8 characters"
                                            autoComplete="new-password"
                                        />
                                    </div>
                                    <div>
                                        <label className="block text-xs font-bold text-theme-secondary mb-1">
                                            {t('profile.backupRestoreConfirmPasswordLabel')}
                                        </label>
                                        <Input
                                            data-testid="confirm-device-password-input"
                                            type="password"
                                            value={confirmNewPassword}
                                            onChange={(e) => setConfirmNewPassword(e.target.value)}
                                            placeholder="Min. 8 characters"
                                            autoComplete="new-password"
                                        />
                                    </div>
                                </div>

                                <Button
                                    data-testid="restore-submit-btn"
                                    type="submit"
                                    variant="primary"
                                    disabled={isRestoring}
                                    className="w-full mt-2"
                                >
                                    {isRestoring ? (
                                        <>
                                            <RefreshCw size={14} className="animate-spin mr-2" />
                                            {t('profile.backupRestoreRestoring')}
                                        </>
                                    ) : (
                                        <>
                                            <ShieldCheck size={16} className="mr-2" />
                                            {t('profile.backupRestoreSubmitButton')}
                                        </>
                                    )}
                                </Button>
                            </form>
                        </div>
                    )}
                </div>
            )}

            {/* STEP 4: Completion State */}
            {step === 'completed' && restoreOutcome && (
                <div className="space-y-6 text-center animate-in zoom-in-95 duration-400">
                    <div className="w-16 h-16 rounded-full bg-emerald-100 text-emerald-600 flex items-center justify-center mx-auto shadow-inner">
                        <CheckCircle2 size={36} />
                    </div>

                    <div className="space-y-1">
                        <h2 className="text-2xl font-black text-theme-secondary">
                            {t('profile.backupRestoreSuccessTitle')}
                        </h2>
                        <p className="text-xs text-theme-light max-w-md mx-auto">
                            {t('profile.backupRestoreSuccessDesc')}
                        </p>
                    </div>

                    {/* Outcome Stats */}
                    <div className="grid grid-cols-3 gap-3 p-4 bg-slate-50 border border-slate-200 rounded-2xl max-w-md mx-auto">
                        <div>
                            <p className="text-lg font-black text-theme-primary">
                                {restoreOutcome.activeVoucherQueriesCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">Active</p>
                        </div>
                        <div>
                            <p className="text-lg font-black text-slate-700">
                                {restoreOutcome.reconciledSpentCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">Spent</p>
                        </div>
                        <div>
                            <p className="text-lg font-black text-slate-700">
                                {restoreOutcome.restoredStandardsCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">Standards</p>
                        </div>
                    </div>

                    {/* Notice */}
                    <div className="p-3.5 rounded-xl bg-slate-100 text-xs text-theme-secondary max-w-md mx-auto leading-relaxed">
                        {restoreOutcome.l2ReconciliationStatus === 'no_servers'
                            ? t('profile.backupRestoreOfflineNotice')
                            : t('profile.backupRestoreReconciledNotice', {
                                  active: restoreOutcome.activeVoucherQueriesCount,
                                  spent: restoreOutcome.reconciledSpentCount,
                              })}
                    </div>

                    {/* Mandatory SST-V4 L2 Warning Banner */}
                    {restoreOutcome.l2ReconciliationStatus !== 'reconciled' && restoreOutcome.activeVoucherQueriesCount > 0 && (
                        <div className="p-4 rounded-2xl bg-amber-50 border border-amber-200 text-amber-900 text-xs text-left space-y-2 max-w-md mx-auto">
                            <div className="flex items-center gap-2 font-bold text-amber-800">
                                <AlertTriangle size={16} className="shrink-0" />
                                <span>{t('profile.backupRestoreL2WarningTitle', 'Online-Sicherheitsabgleich erforderlich')}</span>
                            </div>
                            <p className="leading-relaxed">
                                {t('profile.backupRestoreL2WarningDesc', 'Vor dem ersten Ausgeben von Gutscheinen ist ein einmaliger Online-Sicherheitsabgleich mit dem Layer-2-Netzwerk erforderlich, um Offline-Ausgaben vor dem Geräteverlust zu prüfen.')}
                            </p>
                            <Button
                                data-testid="restore-reconcile-l2-btn"
                                variant="secondary"
                                size="sm"
                                disabled={isReconcilingL2}
                                onClick={handleReconcileL2}
                                className="w-full mt-1"
                            >
                                {isReconcilingL2 ? (
                                    <>
                                        <RefreshCw size={14} className="animate-spin mr-2" />
                                        {t('profile.backupRestoreReconcilingL2', 'Gleiche mit Layer 2 ab...')}
                                    </>
                                ) : (
                                    <>
                                        <RefreshCw size={14} className="mr-2" />
                                        {t('profile.backupRestoreReconcileL2Btn', 'Jetzt online abgleichen')}
                                    </>
                                )}
                            </Button>
                        </div>
                    )}

                    <Button
                        variant="primary"
                        onClick={() => onRestoreSuccess(restoreOutcome)}
                        className="w-full max-w-xs mx-auto py-3 text-sm shadow-md"
                    >
                        {t('profile.backupRestoreOpenWallet')}
                    </Button>
                </div>
            )}
        </AuthLayout>
    );
};
