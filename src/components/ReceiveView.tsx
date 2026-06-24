// src/components/ReceiveView.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../services/transferService';
import { settingsService } from '../services/settingsService';
import { voucherService } from '../services/voucherService';
import { standardsService } from '../services/standardsService';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { translateError, isBackendError, stringifyError } from '../utils/errorHelper';
import { 
    VoucherStandardInfo,
    AppSettings,
    ReceiveSuccessPayload,
    VoucherDetails
} from '../types';
import { PageLayout } from './ui/PageLayout';
import { 
    UploadCloud, 
    FileCheck, 
    ShieldAlert, 
    FileSignature, 
    ArrowRightLeft, 
    Lock, 
    X,
    CheckCircle2,
    AlertCircle
} from 'lucide-react';

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
}

export function ReceiveView({ onBack, onReceiveSuccess }: ReceiveViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [bundlePath, setBundlePath] = useState<string | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [showConfirm, setShowConfirm] = useState(false);
    const [isProcessing, setIsProcessing] = useState(false);
    const [bundlePassword, setBundlePassword] = useState("");
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [resultModal, setResultModal] = useState<{
        isOpen: boolean;
        title: string;
        content: React.ReactNode;
        confirmText: string;
        voucherId?: string;
    } | null>(null);
    const [toleranceModal, setToleranceModal] = useState<{
        type: 'Soft' | 'Critical';
        message: string;
    } | null>(null);
    const [confirmText, setConfirmText] = useState("");
    const [pendingPasswordImport, setPendingPasswordImport] = useState<{
        fileData: number[];
        type: 'transfer' | 'ask' | 'sig';
        name: string;
    } | null>(null);

    useEffect(() => {
        async function fetchData() {
            try {
                logger.info("ReceiveView opened, fetching initial data.");
                const [standards, currentSettings] = await Promise.all([
                     standardsService.getStandards(),
                     settingsService.getSettings().catch(e => {
                         logger.warn(`Failed to fetch app settings: ${translateError(e, t)}`);
                         return null;
                     })
                ]);
                setVoucherStandards(standards);
                setSettings(currentSettings);
            } catch (e) {
                const msg = `Failed to fetch initial data: ${translateError(e, t)}`;
                logger.error(msg);
                setFeedbackMsg(msg);
            }
        }
        fetchData();
    }, [t]);

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                defaultPath: settings?.lastUsedDirectory,
                filters: [
                    { name: 'All Human Money Files', extensions: ['transfer', 'ask', 'sig', 'humocoreq', 'humocosig'] },
                    { name: 'Transfer File (.transfer)', extensions: ['transfer'] },
                    { name: 'Signature Request (.ask)', extensions: ['ask', 'humocoreq'] },
                    { name: 'Signature Response (.sig)', extensions: ['sig', 'humocosig'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (typeof selectedPath === 'string') {
                logger.info(`File selected via dialog: ${selectedPath}`);
                setBundlePath(selectedPath);
                
                if (settings) {
                    updateLastUsedDirectory(selectedPath, settings, protectAction).then(() => {
                        settingsService.getSettings().then(setSettings).catch(() => {});
                    });
                }

                const fileName = selectedPath.split(/[/\\]/).pop() || '';
                setBundleName(fileName);
                
                let detectedType: 'transfer' | 'ask' | 'sig' | null = null;
                if (fileName.endsWith('.transfer')) {
                    detectedType = 'transfer';
                } else if (fileName.endsWith('.ask') || fileName.endsWith('.humocoreq')) {
                    detectedType = 'ask';
                } else if (fileName.endsWith('.sig') || fileName.endsWith('.humocosig')) {
                    detectedType = 'sig';
                }


                if (detectedType) {
                    const fileUint8Array = await readFile(selectedPath);
                    const fileData = Array.from(fileUint8Array);
                    executeReceiveDirectly(fileData, detectedType, fileName);
                }
            }
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        }
    };

    const clearSelection = () => {
        setBundlePath(null);
        setBundleName(null);

        setDroppedFileContent(null);
        setPendingPasswordImport(null);
    }

    const [isDragOver, setIsDragOver] = useState(false);
    const [droppedFileContent, setDroppedFileContent] = useState<number[] | null>(null);

    useEffect(() => {
        const preventGlobalDefaults = (e: Event) => e.preventDefault();
        const handleGlobalDrop = (e: Event) => e.preventDefault();

        document.addEventListener("dragenter", preventGlobalDefaults, true);
        document.addEventListener("dragover", preventGlobalDefaults, true);
        document.addEventListener("dragleave", preventGlobalDefaults, true);
        document.addEventListener("drop", handleGlobalDrop, true);

        return () => {
            document.removeEventListener("dragenter", preventGlobalDefaults, true);
            document.removeEventListener("dragover", preventGlobalDefaults, true);
            document.removeEventListener("dragleave", preventGlobalDefaults, true);
            document.removeEventListener("drop", handleGlobalDrop, true);
        };
    }, []);

    const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(true);
    };

    const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(false);
    };

    const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(false);

        const files = e.dataTransfer.files;
        if (files.length > 0) {
            const file = files[0];
            const validExtensions = ['.transfer', '.ask', '.sig', '.humocoreq', '.humocosig'];
            if (validExtensions.some(ext => file.name.endsWith(ext))) {
                try {
                    const fileBuffer = await file.arrayBuffer();
                    const uint8Array = new Uint8Array(fileBuffer);
                    const bundleData = Array.from(uint8Array);
                    
                    setBundlePath(file.name);
                    setBundleName(file.name);
                    setDroppedFileContent(bundleData);
                    
                    let detectedType: 'transfer' | 'ask' | 'sig' | null = null;
                    if (file.name.endsWith('.transfer')) detectedType = 'transfer';
                    else if (file.name.endsWith('.ask') || file.name.endsWith('.humocoreq')) detectedType = 'ask';
                    else if (file.name.endsWith('.sig') || file.name.endsWith('.humocosig')) detectedType = 'sig';


                    if (detectedType) {
                        executeReceiveDirectly(bundleData, detectedType, file.name);
                    }
                } catch (error) {
                    setFeedbackMsg(translateError(error, t));
                }
            } else {
                setFeedbackMsg(t('transfer.importFile.invalidFileType'));
            }
        }
    };

    async function executeReceiveDirectly(
        fileData: number[],
        type: 'transfer' | 'ask' | 'sig',
        name: string,
        password?: string
    ) {
        setIsProcessing(true);
        setFeedbackMsg('');
        try {
            if (type === 'transfer') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) standardDefinitionsToml[uuidMatch[1]] = standard.content;
                });

                const payload = await protectAction(async (walletPassword) => {
                    return await transferService.receiveBundle({
                        bundleData: fileData,
                        standardDefinitionsToml,
                        password: password || walletPassword || undefined,
                        forceAcceptToleranceBundle: false
                    });
                });

                if (payload) onReceiveSuccess(payload);
            } else if (type === 'ask') {
                const openedVoucher = await voucherService.openSigningRequest(fileData, password || undefined);
                onReceiveSuccess(openedVoucher);
            } else if (type === 'sig') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) standardDefinitionsToml[uuidMatch[1]] = standard.content;
                });
                
                const standardTomlContent = Object.values(standardDefinitionsToml)[0];
                const updatedInstanceId = await protectAction(async (walletPassword) => {
                    return await voucherService.processAndAttachSignature(fileData, standardTomlContent, password || undefined, walletPassword || undefined);
                });
                
                if (updatedInstanceId) {
                    setResultModal({
                        isOpen: true,
                        title: t('transfer.importFile.transferComplete'),
                        content: <p className="text-sm font-medium text-theme-secondary">{t('transfer.importFile.transferCompleteDesc')}</p>,
                        confirmText: t('transfer.importFile.viewDetails'),
                        voucherId: updatedInstanceId
                    });
                }
            }
        } catch (e) {
            const errorStr = stringifyError(e);
            
            // Check if it's a password required error
            const isPasswordRequired = errorStr.includes("Password required") || errorStr.toLowerCase().includes("decrypt") || errorStr.includes("SymmetricEncryption") || errorStr.includes("MacError");
            
            if (isPasswordRequired) {
                // Show the password prompt modal
                setPendingPasswordImport({
                    fileData,
                    type,
                    name
                });
                setShowConfirm(true);
                return;
            }

            const isTolerance = (isBackendError(e) && (e.code === 'error.transfer.bundleRecoveryZone' || e.code === 'error.transfer.bundleToleranceZone')) || errorStr.includes("ToleranceZone");
            if (isTolerance) {
                const isCritical = (isBackendError(e) && e.code === 'error.transfer.bundleRecoveryZone') || errorStr.includes("Extended");
                setToleranceModal({
                    type: isCritical ? 'Critical' : 'Soft',
                    message: translateError(e, t)
                });
                return;
            }
            if (errorStr.includes("already attached")) {
                const match = errorStr.match(/\[LOCAL_ID:([\w-]+)]/);
                if (match) {
                    setResultModal({
                        isOpen: true,
                        title: t('transfer.importFile.duplicateSignature'),
                        content: <p className="text-sm font-medium text-theme-secondary">{t('transfer.importFile.duplicateSignatureDesc')}</p>,
                        confirmText: t('transfer.importFile.goToAsset'),
                        voucherId: match[1]
                    });
                    return;
                }
            }
            setFeedbackMsg(translateError(e, t));
            clearSelection();
        } finally {
            if (!toleranceModal && !pendingPasswordImport) {
                setIsProcessing(false);
            }
        }
    }

    const handleConfirmPassword = async () => {
        if (!pendingPasswordImport) return;
        
        const { fileData, type, name } = pendingPasswordImport;
        // Keep processing flag active
        setIsProcessing(true);
        // Clear pending password import so next steps clean up
        setPendingPasswordImport(null);
        setShowConfirm(false);
        
        await executeReceiveDirectly(fileData, type, name, bundlePassword);
        setBundlePassword("");
    };

    const handleCancelPassword = () => {
        setPendingPasswordImport(null);
        setShowConfirm(false);
        setBundlePassword("");
        setIsProcessing(false);
        clearSelection();
    };

    async function confirmToleranceImport() {
        if (!toleranceModal) return;
        setIsProcessing(true);
        try {
            let fileData: number[];
            if (droppedFileContent) fileData = droppedFileContent;
            else if (bundlePath) {
                const fileUint8Array = await readFile(bundlePath);
                fileData = Array.from(fileUint8Array);
            } else return;

            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(standard => {
                const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                if (uuidMatch && uuidMatch[1]) standardDefinitionsToml[uuidMatch[1]] = standard.content;
            });

            const payload = await protectAction(async (password) => {
                return await transferService.receiveBundle({
                    bundleData: fileData,
                    standardDefinitionsToml,
                    password: password || undefined,
                    forceAcceptToleranceBundle: true
                });
            });

            if (payload) onReceiveSuccess(payload);
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        } finally {
            setIsProcessing(false);
            setToleranceModal(null);
            setConfirmText("");
            setShowConfirm(false);
        }
    }

    return (
        <PageLayout 

            title={t('transfer.importFile.title')} 
            description={t('transfer.importFile.description')} 
            onBack={onBack}
        >
            <div className="max-w-3xl mx-auto space-y-8">
                {feedbackMsg && (
                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-start gap-3 animate-in shake duration-500">
                        <AlertCircle className="text-rose-500 shrink-0" size={18} />
                        <p className="text-sm font-bold text-rose-800 leading-tight">{feedbackMsg}</p>
                    </div>
                )}

                <div className="space-y-8">
                    <div
                        id="bundle-drop-zone"
                        className={`relative group border-2 border-dashed rounded-[40px] p-12 transition-all flex flex-col items-center justify-center text-center overflow-hidden min-h-[340px] cursor-pointer ${
                            isDragOver 
                                ? 'border-theme-primary bg-theme-primary/5 shadow-premium-lg' 
                                : bundleName 
                                    ? 'border-emerald-500/30 bg-emerald-50/10' 
                                    : 'border-theme-subtle hover:border-theme-primary/40 hover:bg-white/40'
                        }`}
                        onDragOver={handleDragOver}
                        onDragLeave={handleDragLeave}
                        onDrop={handleDrop}
                        onClick={bundleName ? undefined : handleFileSelect}
                    >
                        {/* Background subtle pattern or glow */}
                        <div className={`absolute inset-0 transition-opacity duration-700 pointer-events-none ${isDragOver ? 'opacity-100' : 'opacity-0'}`}>
                            <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[300px] h-[300px] bg-theme-primary/10 rounded-full blur-[100px]"></div>
                        </div>

                        {bundleName ? (
                            <div className="relative animate-in zoom-in duration-300 space-y-6">
                                <div className="mx-auto w-24 h-24 bg-emerald-500 rounded-[32px] flex items-center justify-center shadow-lg shadow-emerald-200 text-white">
                                    {isProcessing ? <ArrowRightLeft className="animate-spin" size={48} /> : <FileCheck size={48} />}
                                </div>
                                <div>
                                    <h3 className="text-xl font-black text-theme-primary tracking-tight mb-2">
                                        {isProcessing ? t('transfer.importFile.loading') : t('transfer.importFile.fileDetected')}
                                    </h3>
                                    <div className="flex flex-col items-center gap-1.5">
                                        <p className="text-xs font-mono font-bold text-emerald-600 bg-emerald-50 px-3 py-1 rounded-full border border-emerald-100">
                                            {bundleName}
                                        </p>
                                        <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mt-2">
                                            {isProcessing ? t('transfer.importFile.loading') : t('transfer.importFile.readyToImport')}
                                        </p>
                                    </div>
                                </div>
                                {!isProcessing && (
                                    <div className="pt-4">
                                        <button 
                                            type="button" 
                                            onClick={(e) => { e.stopPropagation(); clearSelection(); }}
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
                                    <h3 className="text-xl font-black text-theme-primary tracking-tight">{t('transfer.importFile.dropZone')}</h3>
                                    <p className="text-sm font-medium text-theme-light/60 max-w-[280px]">
                                        {t('transfer.importFile.dropZoneHint')}
                                    </p>
                                </div>
                                <div className="pt-4">
                                    <Button type="button" onClick={(e) => { e.stopPropagation(); handleFileSelect(); }} variant="secondary" className="rounded-2xl px-8 shadow-premium group-hover:bg-theme-secondary group-hover:text-white transition-all">
                                        {t('transfer.importFile.selectFile')}
                                    </Button>
                                </div>
                            </div>
                        )}
                    </div>

                    <div className="text-center">
                        <p className="text-[10px] font-bold text-theme-light flex items-center justify-center gap-2">
                            <Lock size={12} />
                            {t('transfer.importFile.localProcessing')}
                        </p>
                    </div>
                </div>

                {/* File Format Guide */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-10">
                    {[
                        { icon: FileCheck, titleKey: 'transfer.importFile.formatTransfer', descKey: 'transfer.importFile.formatTransferDesc', ext: '.transfer' },
                        { icon: FileSignature, titleKey: 'transfer.importFile.formatSignatureRequest', descKey: 'transfer.importFile.formatSigReqDesc', ext: '.ask' },
                        { icon: CheckCircle2, titleKey: 'transfer.importFile.formatSignatureResponse', descKey: 'transfer.importFile.formatSigRespDesc', ext: '.sig' }
                    ].map((item, i) => (
                        <div key={i} className="p-4 bg-white/40 border border-theme-subtle/50 rounded-2xl flex flex-col items-center text-center">
                            <item.icon size={18} className="text-theme-light/60 mb-2" />
                            <h4 className="text-[10px] font-black uppercase tracking-widest text-theme-secondary mb-1">{t(item.titleKey)}</h4>
                            <p className="text-[10px] text-theme-light font-medium leading-tight">{t(item.descKey)}</p>
                            <span className="mt-2 text-[9px] font-mono font-bold bg-theme-subtle/20 px-2 py-0.5 rounded text-theme-light">{item.ext}</span>
                        </div>
                    ))}
                </div>
            </div>

            {/* Modals */}
            <ConfirmationModal
                isOpen={showConfirm && !!pendingPasswordImport}
                title={
                    pendingPasswordImport?.type === 'transfer' 
                        ? t('transfer.importFile.importModalTitleTransfer') 
                        : pendingPasswordImport?.type === 'ask' 
                            ? t('transfer.importFile.importModalTitleAsk') 
                            : t('transfer.importFile.importModalTitleSig')
                }
                description={
                    <div className="space-y-6 pt-2">
                        <div className="p-4 bg-rose-500/5 rounded-2xl border border-rose-500/10 flex items-center gap-4">
                            <div className="p-2 bg-white rounded-xl shadow-sm text-rose-500">
                                <Lock size={24} />
                            </div>
                            <div>
                                <p className="text-sm font-bold text-theme-secondary truncate max-w-[200px]">{pendingPasswordImport?.name}</p>
                                <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">{t('transfer.importFile.typeLabel', { fileType: pendingPasswordImport?.type })}</p>
                            </div>
                        </div>

                        <div className="space-y-3">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                                <Lock size={12} /> {t('transfer.importFile.passwordLabel')}
                            </label>
                            <Input
                                type="password"
                                value={bundlePassword}
                                onChange={(e: React.ChangeEvent<HTMLInputElement>) => setBundlePassword(e.target.value)}
                                placeholder={t('transfer.importFile.passwordPlaceholder')}
                                autoFocus
                            />
                            <p className="text-[10px] text-theme-light font-medium italic">
                                {t('transfer.importFile.passwordHint')}
                            </p>
                        </div>
                    </div>
                }
                confirmText={t('transfer.importFile.confirmImport')}
                onConfirm={handleConfirmPassword}
                onCancel={handleCancelPassword}
                isProcessing={isProcessing}
            />

            {toleranceModal && (
                <ConfirmationModal
                    isOpen={true}
                    title={toleranceModal.type === 'Soft' ? t('transfer.importFile.syncFromBackup') : t('transfer.importFile.criticalConflict')}
                    confirmVariant="danger"
                    description={
                        <div className="space-y-6 pt-2">
                            <div className="p-4 bg-rose-50 border border-rose-200 rounded-2xl flex items-start gap-3">
                                <ShieldAlert size={20} className="text-rose-600 shrink-0 mt-0.5" />
                                <p className="text-sm font-medium text-rose-900 leading-relaxed">{toleranceModal.message}</p>
                            </div>
                            
                            {toleranceModal.type === 'Soft' ? (
                                <label className="flex items-center gap-4 p-4 bg-white border border-theme-subtle rounded-2xl cursor-pointer group hover:border-theme-accent/40 transition-all">
                                    <input 
                                        type="checkbox" 
                                        className="w-6 h-6 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent"
                                        checked={confirmText === "checked"}
                                        onChange={(e) => setConfirmText(e.target.checked ? "checked" : "")}
                                    />
                                    <span className="text-sm font-bold text-theme-secondary select-none">
                                        {t('transfer.importFile.acceptResponsibility')}
                                    </span>
                                </label>
                            ) : (
                                <div className="space-y-3">
                                    <p className="text-[10px] font-black text-rose-500 uppercase tracking-[0.2em]">{t('transfer.importFile.mandatoryAffirmation')}</p>
                                    <Input 
                                        className="border-2 border-rose-500/30 focus:border-rose-500 uppercase font-mono tracking-widest text-center py-4"
                                        placeholder={t('transfer.importFile.typeToConfirm')}
                                        value={confirmText}
                                        onChange={(e) => setConfirmText(e.target.value)}
                                    />
                                </div>
                            )}
                        </div>
                    }
                    confirmText={t('transfer.importFile.importAnyway')}
                    onConfirm={confirmToleranceImport}
                    onCancel={() => { setToleranceModal(null); setConfirmText(""); setIsProcessing(false); setShowConfirm(false); }}
                    isProcessing={isProcessing}
                    confirmDisabled={toleranceModal.type === 'Soft' ? confirmText !== "checked" : confirmText.toUpperCase() !== "IMPORT"}
                />
            )}

            {resultModal && (
                <ConfirmationModal
                    isOpen={resultModal.isOpen}
                    title={resultModal.title}
                    description={resultModal.content}
                    confirmText={resultModal.confirmText}
                    cancelText={t('transfer.importFile.dismiss')}
                    onConfirm={() => {
                        if (resultModal.voucherId) {
                            onReceiveSuccess({
                                senderId: "",
                                transferSummary: { summableAmounts: {}, countableItems: {} },
                                involvedVouchers: [resultModal.voucherId],
                                isSignatureAttached: true,
                                voucherId: resultModal.voucherId
                            });
                        }
                        setResultModal(null);
                    }}
                    onCancel={() => { setResultModal(null); onBack(); }}
                />
            )}
        </PageLayout>
    );
}
