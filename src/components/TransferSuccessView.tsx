// src/components/TransferSuccessView.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { ModalShell } from './ui/ModalShell';
import { AppSettings, IntentDispatchObservation } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { settingsService } from '../services/settingsService';
import { postService } from '../services/postService';
import { copyToClipboard } from '../utils/clipboard';
import { 
    Download, 
    Share2, 
    ShieldCheck, 
    User,
    ArrowUpRight,
    FolderOpen,
    QrCode,
    CheckCircle2,
    AlertTriangle,
    RefreshCw,
    UploadCloud,
    Copy,
    Check
} from 'lucide-react';
import { suggestFilename } from '../utils/userIdHelper';
import { stringifyError } from '../utils/errorHelper';

interface TransferSuccessViewProps {
    bundleData: number[];
    recipientId: string;
    summary: string;
    l2Dispatch?: IntentDispatchObservation[];
    bundleId?: string;
    initialDepositedTo?: number;
    depositError?: string;
    onDone: () => void;
}

export function TransferSuccessView({
    bundleData,
    recipientId,
    summary,
    l2Dispatch,
    bundleId,
    initialDepositedTo = 0,
    depositError,
    onDone
}: TransferSuccessViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [feedback, setFeedback] = useState('');
    const [isSaving, setIsSaving] = useState(false);
    const [savedFilePath, setSavedFilePath] = useState<string | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);

    // Dual-Delivery post state (§6.3)
    const [depositedTo, setDepositedTo] = useState<number>(initialDepositedTo);
    const [isRedepositing, setIsRedepositing] = useState(false);
    const [redepositError, setRedepositError] = useState<string | null>(depositError || null);
    const [showQrModal, setShowQrModal] = useState(false);
    const [copiedData, setCopiedData] = useState(false);

    useEffect(() => {
        async function fetchSettings() {
            try {
                const currentSettings = await settingsService.getSettings().catch(() => null);
                setSettings(currentSettings);
            } catch (e) {
                console.error("Failed to load settings in TransferSuccessView:", e);
            }
        }
        fetchSettings();
    }, []);

    // SST-V4 Invariant: Retry ONLY calls depositTransferToPost(bundleId), NEVER create_transfer_bundle!
    const handleRetryDeposit = async () => {
        if (!bundleId) return;
        setIsRedepositing(true);
        setRedepositError(null);
        try {
            const res = await protectAction(async (password) => {
                return await postService.depositTransferToPost(bundleId, password || undefined);
            });
            if (res) {
                setDepositedTo(res.depositedTo);
                if (res.depositedTo > 0) {
                    setFeedback(t('transfer.depositedInPostSuccess', '📦 Zahlung in der Ablage hinterlegt (wartet auf Abholung)'));
                } else if (res.errors && res.errors.length > 0) {
                    setRedepositError(res.errors.join('; '));
                }
            }
        } catch (e) {
            setRedepositError(stringifyError(e));
        } finally {
            setIsRedepositing(false);
        }
    };

    async function handleSaveFile() {
        setIsSaving(true);
        setFeedback('');

        try {
            const recipientName = suggestFilename(recipientId);
            const now = new Date();
            const dateTimePart = now.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, '');
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: t('transfer.success.saveTitle'),
                defaultPath: settings?.lastUsedDirectory 
                    ? `${settings.lastUsedDirectory}/${suggestedFilename}`
                    : suggestedFilename,
                filters: [{
                    name: t('transfer.success.fileFilter'),
                    extensions: ['transfer']
                }]
            });

            if (filePath) {
                const content = new Uint8Array(bundleData);
                await writeFile(filePath, content);
                setSavedFilePath(filePath);
                logger.info(`Successfully saved transfer bundle to ${filePath}`);
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        settingsService.getSettings().then(setSettings).catch(() => {});
                    });
                }
                setFeedback(t('transfer.success.synced'));
            }
        } catch (e) {
            setFeedback(`Error: ${stringifyError(e)}`);
        } finally {
            setIsSaving(false);
        }
    }

    const handleCopyBase64 = async () => {
        try {
            const binary = String.fromCharCode(...bundleData);
            const b64 = window.btoa(binary);
            await copyToClipboard(b64);
            setCopiedData(true);
            setTimeout(() => setCopiedData(false), 2000);
        } catch (e) {
            console.error("Failed to copy bundle data", e);
        }
    };

    return (
        <div className="flex flex-col items-center justify-center min-h-[80vh] py-12 px-4 animate-in fade-in zoom-in-95 duration-500">
            <div className="w-full max-w-xl space-y-8">
                {/* Status Hero */}
                <div className="text-center space-y-4">
                    <div className="relative inline-block">
                        <div className="p-6 rounded-[32px] bg-emerald-500 text-white shadow-premium-lg animate-bounce-subtle">
                            <ArrowUpRight size={64} />
                        </div>
                        <div className="absolute -bottom-2 -right-2 bg-white p-1.5 rounded-full shadow-md">
                            <div className="bg-emerald-500 w-4 h-4 rounded-full border-2 border-white"></div>
                        </div>
                    </div>
                    <div className="space-y-1">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tight">{t('transfer.success.title')}</h1>
                        <p className="text-theme-light font-medium">{t('transfer.success.description')}</p>
                    </div>
                </div>

                {/* Transfer Info Card */}
                <div className="bg-white border border-theme-subtle rounded-[40px] p-8 shadow-premium space-y-8">
                    {/* Amount Hero */}
                    <div className="text-center space-y-2">
                        <span className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">{t('transfer.success.totalAmount')}</span>
                        <div className="text-4xl font-black text-theme-primary tracking-tighter">
                            {summary}
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="space-y-2">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5 px-1">
                                <User size={10} /> {t('transfer.success.recipientId')}
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                <p className="text-sm font-mono font-bold text-theme-secondary break-all">
                                    {recipientId}
                                </p>
                            </div>
                        </div>

                        {/* Dual-Delivery Post Drop Status (§6.3) */}
                        {depositedTo > 0 ? (
                            <div data-testid="post-deposit-success-card" className="p-5 bg-emerald-50/80 border border-emerald-200 rounded-3xl space-y-2">
                                <div className="flex items-center gap-2 text-emerald-800 font-bold text-sm">
                                    <CheckCircle2 size={18} className="text-emerald-600 shrink-0" />
                                    <span>{t('transfer.depositedInPostSuccess', '📦 Zahlung in der Ablage hinterlegt (wartet auf Abholung)')}</span>
                                </div>
                                <p className="text-xs text-emerald-700 font-medium leading-relaxed">
                                    {t('transfer.depositedInPostDesc', 'Die Zahlung wurde auf {{count}} Ablage(n) hinterlegt und wird vom Empfänger automatisch beim nächsten App-Start abgerufen.', { count: depositedTo })}
                                </p>
                            </div>
                        ) : bundleId ? (
                            <div data-testid="post-deposit-retry-card" className="p-5 bg-amber-50/80 border border-amber-200 rounded-3xl space-y-3">
                                <div className="flex items-center gap-2 text-amber-900 font-bold text-sm">
                                    <AlertTriangle size={18} className="text-amber-600 shrink-0" />
                                    <span>{t('transfer.depositPendingTitle', 'Nicht in Ablage hinterlegt (offline oder keine gemeinsame Ablage)')}</span>
                                </div>
                                <p className="text-xs text-amber-800/80 font-medium leading-relaxed">
                                    {redepositError || t('transfer.depositPendingDesc', 'Die Zahlung konnte nicht automatisch auf einer Ablage hinterlegt werden. Du kannst es erneut versuchen oder die Zahlung direkt als Datei oder QR-Code übergeben.')}
                                </p>
                                <Button
                                    variant="outline"
                                    size="sm"
                                    disabled={isRedepositing}
                                    onClick={handleRetryDeposit}
                                    data-testid="retry-deposit-button"
                                    className="w-full text-xs font-bold gap-2 border-amber-300 text-amber-900 hover:bg-amber-100/50"
                                >
                                    {isRedepositing ? <RefreshCw size={14} className="animate-spin" /> : <UploadCloud size={14} />}
                                    {isRedepositing ? t('transfer.depositing', 'Hinterlege in Ablage...') : t('transfer.retryDepositButton', 'Erneut in Ablage hinterlegen')}
                                </Button>
                            </div>
                        ) : null}

                        <div className="p-5 bg-blue-50/50 border border-blue-100 rounded-3xl flex items-start gap-4">
                            <div className="p-2 bg-white rounded-xl shadow-sm text-blue-500">
                                <Share2 size={20} />
                            </div>
                            <div className="space-y-1">
                                <h4 className="text-xs font-black text-blue-900 uppercase tracking-widest">{t('transfer.success.nextStep')}</h4>
                                <p className="text-xs text-blue-800/80 font-medium leading-relaxed">
                                    {t('transfer.success.nextStepDesc')}
                                </p>
                            </div>
                        </div>

                        {l2Dispatch && l2Dispatch.length > 0 && (() => {
                            const isAnyDispatched = l2Dispatch.some(obs => obs.httpStatus === 200 || obs.httpStatus === 201);
                            
                            if (isAnyDispatched) {
                                return (
                                    <div className="p-5 bg-emerald-50/60 border border-emerald-200 rounded-3xl space-y-3">
                                        <div className="flex items-center justify-between">
                                            <span className="text-[10px] font-black uppercase tracking-widest text-emerald-900 flex items-center gap-1.5">
                                                <ShieldCheck size={14} className="text-emerald-600" />
                                                {t('transfer.l2DispatchedTitle')}
                                            </span>
                                            <span className="text-[9px] font-bold px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800">
                                                {l2Dispatch.length} {l2Dispatch.length === 1 ? 'Lock-Intent' : 'Lock-Intents'}
                                            </span>
                                        </div>
                                        <p className="text-xs text-emerald-900/80 font-medium leading-relaxed">
                                            {t('transfer.l2DispatchedDesc')}
                                        </p>
                                        <div className="space-y-1.5 pt-1">
                                            {l2Dispatch.map((obs, idx) => (
                                                <div key={`${obs.localInstanceId}-${idx}`} className="flex items-center justify-between text-xs p-2.5 bg-white/80 rounded-xl border border-emerald-100">
                                                    <span className="font-mono text-[11px] text-theme-light truncate max-w-[220px]" title={obs.localInstanceId}>
                                                        {obs.localInstanceId.slice(0, 16)}...
                                                    </span>
                                                    <span className={`text-[10px] font-bold px-2 py-0.5 rounded ${obs.httpStatus && obs.httpStatus >= 200 && obs.httpStatus < 300 ? 'bg-emerald-100 text-emerald-800' : 'bg-amber-100 text-amber-800'}`}>
                                                        {obs.httpStatus ? `HTTP ${obs.httpStatus}` : obs.status}
                                                    </span>
                                                </div>
                                            ))}
                                        </div>
                                        <p className="text-[10px] text-theme-light/70 italic border-t border-emerald-200/60 pt-2">
                                            {t('transfer.l2TransportDisclaimer')}
                                        </p>
                                    </div>
                                );
                            }

                            return (
                                <div className="p-5 bg-slate-50 border border-slate-200 rounded-3xl space-y-3">
                                    <div className="flex items-center justify-between">
                                        <span className="text-[10px] font-black uppercase tracking-widest text-theme-secondary flex items-center gap-1.5">
                                            <ShieldCheck size={14} className="text-theme-primary" />
                                            {t('transfer.l2PreparedOfflineTitle')}
                                        </span>
                                        <span className="text-[9px] font-bold px-2 py-0.5 rounded-full bg-slate-200 text-slate-700">
                                            {l2Dispatch.length} {l2Dispatch.length === 1 ? 'Lock-Intent' : 'Lock-Intents'}
                                        </span>
                                    </div>
                                    <p className="text-xs text-theme-light font-medium leading-relaxed">
                                        {t('transfer.l2PreparedOfflineDesc')}
                                    </p>
                                    <div className="space-y-1.5 pt-1">
                                        {l2Dispatch.map((obs, idx) => (
                                            <div key={`${obs.localInstanceId}-${idx}`} className="flex items-center justify-between text-xs p-2.5 bg-white rounded-xl border border-slate-100">
                                                <span className="font-mono text-[11px] text-theme-light truncate max-w-[220px]" title={obs.localInstanceId}>
                                                    {obs.localInstanceId.slice(0, 16)}...
                                                </span>
                                                <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-slate-100 text-slate-700">
                                                    {obs.status}
                                                </span>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            );
                        })()}
                    </div>
                </div>

                {/* 100% Offline & Direct Sharing Action Controls */}
                <div className="flex flex-col items-center gap-3">
                    <Button 
                        size="lg" 
                        onClick={handleSaveFile} 
                        disabled={isSaving} 
                        data-testid="save-transfer-file-button"
                        className="w-full max-w-sm py-4 rounded-3xl shadow-premium-lg text-base font-bold gap-2"
                    >
                        {isSaving ? <Download className="animate-bounce" size={20} /> : <Download size={20} />}
                        {isSaving ? t('transfer.success.savingFile') : t('transfer.success.saveFile')}
                    </Button>

                    <Button
                        variant="outline"
                        size="md"
                        onClick={() => setShowQrModal(true)}
                        data-testid="show-qr-button"
                        className="w-full max-w-sm py-3 rounded-2xl text-xs font-bold gap-2 border-theme-subtle"
                    >
                        <QrCode size={16} />
                        {t('transfer.showQrCodeButton', 'QR-Code & Direktübergabe anzeigen')}
                    </Button>

                    {savedFilePath && (
                        <Button
                            variant="secondary"
                            onClick={() => {
                                revealItemInDir(savedFilePath).catch((err) => {
                                    logger.error(`Failed to reveal item in dir: ${stringifyError(err)}`);
                                });
                            }}
                            className="w-full max-w-sm py-3 rounded-2xl text-xs font-bold gap-2 animate-in fade-in duration-200"
                        >
                            <FolderOpen size={16} />
                            {t('transfer.success.revealInFolder')}
                        </Button>
                    )}

                    <Button 
                        variant="secondary" 
                        onClick={onDone} 
                        className="w-full max-w-sm py-3 rounded-2xl text-xs font-black uppercase tracking-widest mt-2"
                    >
                        {t('transfer.success.finish')}
                    </Button>
                    
                    {feedback && (
                        <p className="text-xs font-bold text-emerald-600 flex items-center gap-2 animate-in slide-in-from-bottom-2">
                            <ShieldCheck size={14} />
                            {feedback}
                        </p>
                    )}
                </div>
            </div>

            {/* Offline QR Code Modal */}
            {showQrModal && (
                <ModalShell
                    isOpen={showQrModal}
                    onClose={() => setShowQrModal(false)}
                    title={t('transfer.qrModalTitle', 'Direktübergabe & Offline-Zahlung')}
                >
                    <div className="space-y-4 p-2 text-center">
                        <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle text-left space-y-2">
                            <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                                {t('transfer.qrModalDirectHeader', 'Direkte Offline-Zahlung')}
                            </h4>
                            <p className="text-xs text-theme-secondary leading-relaxed">
                                {t('transfer.qrModalDirectDesc', 'Transfer-Dateien (.transfer) können direkt per USB, Bluetooth, Datei-Freigabe oder Chat an den Empfänger übergeben werden. Der Empfänger importiert die Datei in seiner Human Money App unter "Gutscheine empfangen".')}
                            </p>
                        </div>

                        <div className="p-3 bg-white border border-theme-subtle rounded-xl flex items-center justify-between gap-2">
                            <span className="text-xs font-mono text-theme-light truncate">
                                {suggestFilename(recipientId)}.transfer ({bundleData.length} Bytes)
                            </span>
                            <Button
                                size="sm"
                                variant="outline"
                                onClick={handleCopyBase64}
                                className="gap-1 text-xs shrink-0"
                            >
                                {copiedData ? <Check size={14} className="text-emerald-600" /> : <Copy size={14} />}
                                {copiedData ? t('common.copied', 'Kopiert!') : t('transfer.copyBundleData', 'Daten kopieren')}
                            </Button>
                        </div>

                        <Button
                            variant="primary"
                            onClick={() => setShowQrModal(false)}
                            className="w-full text-xs font-bold py-2.5 rounded-xl"
                        >
                            {t('common.close', 'Schließen')}
                        </Button>
                    </div>
                </ModalShell>
            )}
        </div>
    );
}
