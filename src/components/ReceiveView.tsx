// src/components/ReceiveView.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../services/transferService';
import { settingsService } from '../services/settingsService';
import { voucherService } from '../services/voucherService';
import { standardsService } from '../services/standardsService';
import { wotService } from '../services/wotService';
import { integrityService } from '../services/integrityService';
import { contactService } from '../services/contactService';
import ContactDialog from './ContactDialog';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { copyToClipboard } from '../utils/clipboard';
import { profileService } from '../services/profileService';
import { buildPaymentRequestText } from '../utils/humocoUri';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { useOptionalNavigation } from '../context/NavigationContext';
import { marketService } from '../services/marketService';
import { HelpIcon } from './ui/HelpIcon';

import { translateError, isBackendError, stringifyError } from '../utils/errorHelper';
import { 
    VoucherStandardInfo,
    VoucherStandardDefinition,
    AppSettings,
    ReceiveSuccessPayload,
    VoucherDetails,
    VoucherSummary,
    Contact,
    WotImportResultDto,
    PublicProfile
} from '../types';
import { PageLayout } from './ui/PageLayout';
import { SegmentedTabs } from './ui/SegmentedTabs';
import { AlertBanner } from './ui/AlertBanner';
import { ModalShell } from './ui/ModalShell';
import { ImportStandardModal } from './ImportStandardModal';
import { 
    UploadCloud, 
    FileCheck, 
    ShieldAlert, 
    FileSignature, 
    ArrowRightLeft, 
    Lock, 
    X,
    CheckCircle2,
    AlertCircle,
    Copy,
    Check
} from 'lucide-react';

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
    droppedFile?: { name: string; bytes: number[] };
}

export function ReceiveView({ onBack, onReceiveSuccess, droppedFile }: ReceiveViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const nav = useOptionalNavigation();
    const navigate = nav?.navigate;
    const [activeTab, setActiveTab] = useState<'import' | 'request'>('import');
    const [ownUserId, setOwnUserId] = useState('');
    const [ownProfile, setOwnProfile] = useState<PublicProfile | null>(null);
    const [reqStandardId, setReqStandardId] = useState<string>('');
    const [reqAmount, setReqAmount] = useState<string>('');
    const [reqNote, setReqNote] = useState<string>('');
    const [reqIsTest, setReqIsTest] = useState<boolean>(false);
    const [copiedRequest, setCopiedRequest] = useState<boolean>(false);
    const [bundlePath, setBundlePath] = useState<string | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [showConfirm, setShowConfirm] = useState(false);
    const [isProcessing, setIsProcessing] = useState(false);
    const [bundlePassword, setBundlePassword] = useState("");
    const [isPosMode, setIsPosMode] = useState(false);
    const [purchaseId, setPurchaseId] = useState("");
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [resultModal, setResultModal] = useState<{
        isOpen: boolean;
        title: string;
        content: React.ReactNode;
        confirmText: string;
        voucherId?: string;
        proofId?: string;
    } | null>(null);
    const [toleranceModal, setToleranceModal] = useState<{
        type: 'Soft' | 'Critical';
        message: string;
    } | null>(null);
    const [confirmText, setConfirmText] = useState("");
    const [pendingPasswordImport, setPendingPasswordImport] = useState<{
        fileData: number[];
        type: 'transfer' | 'ask' | 'sig' | 'wot' | 'proof' | 'card';
        name: string;
    } | null>(null);

    const [wotImportModalData, setWotImportModalData] = useState<WotImportResultDto | null>(null);
    const [selectedProviderLevel, setSelectedProviderLevel] = useState<number>(0);
    const [isSavingProviderTrust, setIsSavingProviderTrust] = useState(false);
    const [wotContactDialogOpen, setWotContactDialogOpen] = useState(false);
    const [wotExistingContact, setWotExistingContact] = useState<Contact | null>(null);
    const [importStandardModalData, setImportStandardModalData] = useState<{
        containerBytes: number[];
        definition?: VoucherStandardDefinition;
    } | null>(null);
    const [pendingPosFallback, setPendingPosFallback] = useState<{
        fileData: number[];
        standardDefinitionsToml: Record<string, string>;
        errorCode: string;
        purchaseId: string;
    } | null>(null);

    useEffect(() => {
        async function fetchData() {
            try {
                logger.info("ReceiveView opened, fetching initial data.");
                const [standards, currentSettings, userId, profile] = await Promise.all([
                     standardsService.getStandards(),
                     settingsService.getSettings().catch(e => {
                         logger.warn(`Failed to fetch app settings: ${translateError(e, t)}`);
                         return null;
                     }),
                     voucherService.getUserId().catch(() => ''),
                     profileService.getProfile().catch(() => null)
                ]);
                setVoucherStandards(standards);
                if (standards.length > 0) {
                    setReqStandardId(standards[0].id);
                }
                setSettings(currentSettings);
                setOwnUserId(userId);
                setOwnProfile(profile);
            } catch (e) {
                const msg = `Failed to fetch initial data: ${translateError(e, t)}`;
                logger.error(msg);
                setFeedbackMsg(msg);
            }
        }
        fetchData();
    }, [t]);

    useEffect(() => {
        if (droppedFile) {
            setBundlePath(droppedFile.name);
            setBundleName(droppedFile.name);
            setDroppedFileContent(droppedFile.bytes);

            if (droppedFile.name.endsWith('.standard') || droppedFile.name.endsWith('.toml')) {
                processStandardFile(droppedFile.bytes);
            } else {
                let detectedType: 'transfer' | 'ask' | 'sig' | 'wot' | 'proof' | 'card' | null = null;
                if (droppedFile.name.endsWith('.transfer')) detectedType = 'transfer';
                else if (droppedFile.name.endsWith('.ask') || droppedFile.name.endsWith('.humocoreq')) detectedType = 'ask';
                else if (droppedFile.name.endsWith('.sig') || droppedFile.name.endsWith('.humocosig')) detectedType = 'sig';
                else if (droppedFile.name.endsWith('.wot')) detectedType = 'wot';
                else if (droppedFile.name.endsWith('.hmcproof')) detectedType = 'proof';
                else if (droppedFile.name.endsWith('.hmcard')) detectedType = 'card';

                if (detectedType) {
                    executeReceiveDirectly(droppedFile.bytes, detectedType, droppedFile.name);
                }
            }
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [droppedFile]);

    const processStandardFile = async (fileData: number[]) => {
        setIsProcessing(true);
        try {
            const def = await standardsService.inspectStandardFile(fileData, null);
            setImportStandardModalData({ containerBytes: fileData, definition: def });
        } catch {
            setImportStandardModalData({ containerBytes: fileData });
        } finally {
            setIsProcessing(false);
        }
    };

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                defaultPath: settings?.lastUsedDirectory,
                filters: [
                    { name: 'All Human Money Files', extensions: ['transfer', 'ask', 'sig', 'standard', 'toml', 'humocoreq', 'humocosig', 'wot', 'hmcproof', 'hmcard'] },
                    { name: 'Transfer File (.transfer)', extensions: ['transfer'] },
                    { name: 'Signature Request (.ask)', extensions: ['ask', 'humocoreq'] },
                    { name: 'Signature Response (.sig)', extensions: ['sig', 'humocosig'] },
                    { name: 'Double Spend Proof (.hmcproof)', extensions: ['hmcproof'] },
                    { name: 'Voucher Standard (.standard, .toml)', extensions: ['standard', 'toml'] },
                    { name: t('transfer.importFile.wotFilter'), extensions: ['wot'] },
                    { name: t('marketplace.cardFilterName'), extensions: ['hmcard'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (typeof selectedPath === 'string') {
                logger.info(`File selected via dialog: ${selectedPath}`);
                setBundlePath(selectedPath);
                
                if (settings) {
                    Promise.resolve(updateLastUsedDirectory(selectedPath, settings, protectAction))
                        .then(() => settingsService.getSettings().then(setSettings).catch(() => {}))
                        .catch(() => {});
                }

                const fileName = selectedPath.split(/[/\\]/).pop() || '';
                setBundleName(fileName);
                
                const fileUint8Array = await readFile(selectedPath);
                const fileData = Array.from(fileUint8Array);

                if (fileName.endsWith('.standard') || fileName.endsWith('.toml')) {
                    await processStandardFile(fileData);
                } else {
                    let detectedType: 'transfer' | 'ask' | 'sig' | 'wot' | 'proof' | 'card' | null = null;
                    if (fileName.endsWith('.transfer')) {
                        detectedType = 'transfer';
                    } else if (fileName.endsWith('.ask') || fileName.endsWith('.humocoreq')) {
                        detectedType = 'ask';
                    } else if (fileName.endsWith('.sig') || fileName.endsWith('.humocosig')) {
                        detectedType = 'sig';
                    } else if (fileName.endsWith('.wot')) {
                        detectedType = 'wot';
                    } else if (fileName.endsWith('.hmcproof')) {
                        detectedType = 'proof';
                    } else if (fileName.endsWith('.hmcard')) {
                        detectedType = 'card';
                    }

                    if (detectedType) {
                        executeReceiveDirectly(fileData, detectedType, fileName);
                    }
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
        setImportStandardModalData(null);
        setPendingPosFallback(null);
        setWotImportModalData(null);
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
            const validExtensions = ['.transfer', '.ask', '.sig', '.standard', '.toml', '.humocoreq', '.humocosig', '.wot', '.hmcproof'];
            if (validExtensions.some(ext => file.name.endsWith(ext))) {
                try {
                    const fileBuffer = await file.arrayBuffer();
                    const uint8Array = new Uint8Array(fileBuffer);
                    const bundleData = Array.from(uint8Array);
                    
                    setBundlePath(file.name);
                    setBundleName(file.name);
                    setDroppedFileContent(bundleData);
                    
                    if (file.name.endsWith('.standard') || file.name.endsWith('.toml')) {
                        await processStandardFile(bundleData);
                    } else {
                        let detectedType: 'transfer' | 'ask' | 'sig' | 'wot' | 'proof' | null = null;
                        if (file.name.endsWith('.transfer')) detectedType = 'transfer';
                        else if (file.name.endsWith('.ask') || file.name.endsWith('.humocoreq')) detectedType = 'ask';
                        else if (file.name.endsWith('.sig') || file.name.endsWith('.humocosig')) detectedType = 'sig';
                        else if (file.name.endsWith('.wot')) detectedType = 'wot';
                        else if (file.name.endsWith('.hmcproof')) detectedType = 'proof';

                        if (detectedType) {
                            executeReceiveDirectly(bundleData, detectedType, file.name);
                        }
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
        type: 'transfer' | 'ask' | 'sig' | 'wot' | 'proof' | 'card',
        name: string,
        password?: string
    ) {
        setIsProcessing(true);
        setFeedbackMsg('');
        try {
            if (type === 'transfer') {
                let currentStandards = voucherStandards;
                if (currentStandards.length === 0) {
                    try {
                        currentStandards = await standardsService.getStandards();
                        setVoucherStandards(currentStandards);
                    } catch {
                        // ignore error
                    }
                }
                const standardDefinitionsToml: Record<string, string> = {};
                currentStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) standardDefinitionsToml[uuidMatch[1]] = standard.content;
                });

                if (isPosMode || purchaseId.trim().length > 0) {
                    const activePurchaseId = purchaseId.trim() || `pos-${Date.now()}`;
                    if (!purchaseId.trim()) {
                        setPurchaseId(activePurchaseId);
                    }
                    try {
                        const completion = await protectAction(async (walletPassword) => {
                            return await transferService.checkoutReceivedBundle({
                                purchaseId: activePurchaseId,
                                bundleData: fileData,
                                standardDefinitionsToml,
                                password: password || walletPassword || undefined,
                            });
                        });

                        if (completion) {
                            const basePayload: ReceiveSuccessPayload = completion.receivePayload ?? {
                                senderId: "pos-terminal",
                                transferSummary: { summableAmounts: {}, countableItems: {} },
                                involvedVouchers: [],
                            };
                            const payload: ReceiveSuccessPayload = {
                                ...basePayload,
                                purchaseId: completion.purchaseId,
                                newlyCommitted: completion.newlyCommitted,
                                isReplay: !completion.newlyCommitted,
                                checkoutVerdict: completion.verdict,
                            };
                            onReceiveSuccess(payload);
                            return;
                        }
                    } catch (posErr) {
                        if (isBackendError(posErr) && (posErr.code === 'pos.noServersConfigured' || posErr.code === 'pos.quorumUnavailable')) {
                            setPendingPosFallback({
                                fileData,
                                standardDefinitionsToml,
                                errorCode: posErr.code,
                                purchaseId: activePurchaseId,
                            });
                            setIsProcessing(false);
                            return;
                        }
                        throw posErr;
                    }
                }

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
                const standardsList = voucherStandards.length > 0 ? voucherStandards : await standardsService.getStandards().catch(() => []);
                standardsList.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    if (uuidMatch && uuidMatch[1]) standardDefinitionsToml[uuidMatch[1]] = standard.content;
                });

                let targetVoucherId: string | null = null;
                try {
                    const jsonStr = new TextDecoder().decode(new Uint8Array(fileData));
                    const container = JSON.parse(jsonStr);
                    if (container.ciphertext && (!container.recipients || container.recipients.length === 0)) {
                        try {
                            const base64 = container.ciphertext.replace(/-/g, '+').replace(/_/g, '/');
                            const payloadStr = window.atob(base64);
                            const sigObj = JSON.parse(payloadStr);
                            const inner = sigObj.Signature || sigObj;
                            if (inner && inner.voucher_id) {
                                targetVoucherId = inner.voucher_id;
                            }
                        } catch {
                            // Ciphertext could be encrypted or non-base64 JSON
                        }
                    }
                } catch {
                    // Non-JSON container
                }

                let matchedStandardUuid: string | null = null;
                const summaries = await voucherService.getSummaries().catch(() => [] as VoucherSummary[]);
                const incompleteVouchers = summaries.filter(v => {
                    const statusStr = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
                    return statusStr?.toLowerCase() === 'incomplete';
                });

                if (targetVoucherId) {
                    for (const v of incompleteVouchers) {
                        try {
                            const details = await voucherService.getDetails(v.localInstanceId);
                            if (details?.voucher?.voucherId === targetVoucherId) {
                                matchedStandardUuid = v.voucherStandardUuid || details.voucher.voucherStandard?.uuid || null;
                                break;
                            }
                        } catch {
                            // Ignore details fetch error
                        }
                    }
                    if (!matchedStandardUuid) {
                        for (const v of summaries) {
                            try {
                                const details = await voucherService.getDetails(v.localInstanceId);
                                if (details?.voucher?.voucherId === targetVoucherId) {
                                    matchedStandardUuid = v.voucherStandardUuid || details.voucher.voucherStandard?.uuid || null;
                                    break;
                                }
                            } catch {
                                // Ignore details fetch error
                            }
                        }
                    }
                }

                if (!matchedStandardUuid && incompleteVouchers.length > 0) {
                    const incompleteUuids = Array.from(new Set(incompleteVouchers.map(v => v.voucherStandardUuid).filter(Boolean)));
                    if (incompleteUuids.length === 1) {
                        matchedStandardUuid = incompleteUuids[0];
                    }
                }
                
                const standardTomlContent = (matchedStandardUuid && standardDefinitionsToml[matchedStandardUuid])
                    || Object.values(standardDefinitionsToml)[0]
                    || "";
                const updatedInstanceId = await protectAction(async (walletPassword) => {
                    return await voucherService.processAndAttachSignature(fileData, standardTomlContent, password || undefined, walletPassword || undefined);
                });
                
                if (updatedInstanceId) {
                    setResultModal({
                        isOpen: true,
                        title: t('voucher.signatureAttachedTitle'),
                        content: <p className="text-sm font-medium text-theme-secondary">{t('voucher.signatureAttachedDesc')}</p>,
                        confirmText: t('transfer.importFile.viewDetails'),
                        voucherId: updatedInstanceId
                    });
                }
            } else if (type === 'wot') {
                const wotResult = await protectAction(async (walletPassword) => {
                    return await wotService.importPackage({
                        containerBytes: fileData,
                        containerPassword: password || null,
                        password: walletPassword || null,
                    });
                });

                if (wotResult) {
                    setWotImportModalData(wotResult);
                    setSelectedProviderLevel(wotResult.existingTrustLevel ?? 0);
                }
            } else if (type === 'proof') {
                const proofId = await protectAction(async (walletPassword) => {
                    return await integrityService.importDoubleSpendProof(fileData, walletPassword || undefined);
                });

                if (proofId) {
                    setResultModal({
                        isOpen: true,
                        title: t('conflict.proofImportedTitle'),
                        content: <p className="text-sm font-medium text-theme-secondary">{t('conflict.proofImportedDesc')}</p>,
                        confirmText: t('conflict.viewProofBtn'),
                        proofId
                    });
                }
            } else if (type === 'card') {
                const text = new TextDecoder().decode(new Uint8Array(fileData));
                const cardResult = await protectAction(async (walletPassword) => {
                    return await marketService.importContactCard(text, walletPassword || undefined);
                });
                if (cardResult && navigate) {
                    navigate({
                        view: 'marketplace',
                        selectedDid: cardResult.did ?? undefined,
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
                setIsProcessing(false);  // Reset so modal buttons are active
                return;
            }

            const isTolerance = (isBackendError(e) && (e.code === 'error.transfer.bundleRecoveryZone' || e.code === 'error.transfer.bundleToleranceZone')) || errorStr.includes("ToleranceZone");
            if (isTolerance) {
                const isCritical = (isBackendError(e) && e.code === 'error.transfer.bundleRecoveryZone') || errorStr.includes("Extended");
                setToleranceModal({
                    type: isCritical ? 'Critical' : 'Soft',
                    message: translateError(e, t)
                });
                setIsProcessing(false);  // Reset so modal buttons are active
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
                    setIsProcessing(false);  // Reset so modal buttons are active
                    return;
                }
            }
            setFeedbackMsg(translateError(e, t));
            clearSelection();
        } finally {
            setIsProcessing(false);  // Always reset, no stale-closure guards
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

    const handleAcceptPosTrustFallback = async () => {
        if (!pendingPosFallback) return;
        setIsProcessing(true);
        try {
            const payload = await protectAction(async (walletPassword) => {
                return await transferService.receiveBundle({
                    bundleData: pendingPosFallback.fileData,
                    standardDefinitionsToml: pendingPosFallback.standardDefinitionsToml,
                    password: walletPassword || undefined,
                    forceAcceptToleranceBundle: false
                });
            });

            if (payload) {
                const enrichedPayload: ReceiveSuccessPayload = {
                    ...payload,
                    purchaseId: pendingPosFallback.purchaseId,
                    newlyCommitted: true,
                    isReplay: false,
                    checkoutVerdict: {
                        outcome: pendingPosFallback.errorCode === 'pos.noServersConfigured' ? 'UnverifiedOffline' : 'QuorumUnavailable',
                        canReleaseGoodsAutomatically: false,
                        requiresMerchantApproval: true,
                        isGoodsReleaseAuthorized: false,
                        merchantWarning: pendingPosFallback.errorCode === 'pos.noServersConfigured'
                            ? t('transfer.checkout.fallbackNoServersDesc', 'No L2 servers are configured to lock vouchers against double-spends in real time. You can still accept this transfer on a trust basis using pure Layer-1 math (SST V4).')
                            : t('transfer.checkout.fallbackQuorumDesc', 'Could not reach an L2 quorum to verify lock status. You can either retry or accept on a trust basis (L1 verified).'),
                    },
                };
                onReceiveSuccess(enrichedPayload);
            }
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        } finally {
            setIsProcessing(false);
            setPendingPosFallback(null);
        }
    };

    const handleCancelPosFallback = () => {
        setPendingPosFallback(null);
        clearSelection();
    };

    const isProvider = wotImportModalData?.isProviderFilter;
    const senderProfile = wotImportModalData?.senderProfile;
    const senderDisplayName = isProvider
        ? (wotImportModalData?.providerName || wotImportModalData?.senderDid || '')
        : (senderProfile
            ? (senderProfile.firstName || senderProfile.lastName
                ? `${senderProfile.firstName || ''} ${senderProfile.lastName || ''}`.trim()
                : senderProfile.organization || wotImportModalData?.senderDid || '')
            : (wotImportModalData?.senderDid || ''));

    const handleSaveProviderTrust = async () => {
        if (!wotImportModalData?.senderDid) return;
        setIsSavingProviderTrust(true);
        try {
            await protectAction(async (pwd) => {
                await wotService.setProviderTrust(
                    wotImportModalData.senderDid,
                    selectedProviderLevel,
                    pwd || null
                );
            });
            handleCloseWotModal();
        } catch (err) {
            logger.error("Failed to save provider trust: " + stringifyError(err));
            setResultModal({
                isOpen: true,
                title: t('common.error'),
                content: <p className="text-sm font-medium text-rose-600">{translateError(err, t)}</p>,
                confirmText: t('common.ok'),
            });
        } finally {
            setIsSavingProviderTrust(false);
        }
    };

    const handleOpenContactFromWot = async () => {
        if (!wotImportModalData) return;
        try {
            const allContacts = await contactService.getContacts();
            const found = allContacts.find(c => c.did === wotImportModalData.senderDid) || null;
            setWotExistingContact(found);
        } catch {
            setWotExistingContact(null);
        }
        setWotContactDialogOpen(true);
    };

    const handleSaveContactFromWot = async (contact: Contact) => {
        await protectAction(async (pwd) => {
            await contactService.saveContact(contact, pwd || undefined);
        });
        setWotContactDialogOpen(false);
        setWotImportModalData(null);
        clearSelection();
    };

    const handleCloseWotModal = () => {
        setWotImportModalData(null);
        clearSelection();
    };

    const selectedReqStandard = voucherStandards.find(s => s.id === reqStandardId) || voucherStandards[0];
    const reqUuidMatch = selectedReqStandard?.content.match(/uuid\s*=\s*["']([^"']+)["']/);
    const reqUnitMatch = selectedReqStandard?.content.match(/unit\s*=\s*["']([^"']+)["']/);
    const reqStdUuid = reqUuidMatch ? reqUuidMatch[1] : selectedReqStandard?.id;
    const reqUnit = reqUnitMatch ? reqUnitMatch[1] : selectedReqStandard?.displayName;
    const reqNameParts = [ownProfile?.firstName, ownProfile?.lastName].filter(Boolean);
    const reqName = reqNameParts.length > 0 ? reqNameParts.join(' ') : (ownProfile?.organization || '');

    const generatedPaymentRequest = buildPaymentRequestText({
        to: ownUserId,
        name: reqName || undefined,
        standardUuid: reqStdUuid,
        standardName: selectedReqStandard?.displayName,
        unit: reqUnit,
        amount: reqAmount.trim() || undefined,
        note: reqNote.trim() || undefined,
        isTest: reqIsTest,
    });

    const handleCopyPaymentRequest = async () => {
        setFeedbackMsg('');
        try {
            const success = await copyToClipboard(generatedPaymentRequest);
            if (!success) {
                setFeedbackMsg(t('transfer.requestPayment.copyFailed'));
                return;
            }
            setCopiedRequest(true);
            setTimeout(() => setCopiedRequest(false), 2500);
        } catch (err) {
            logger.error(`Failed to copy payment request: ${stringifyError(err)}`);
            setFeedbackMsg(t('transfer.requestPayment.copyFailed'));
        }
    };

    return (
        <PageLayout 
            title={activeTab === 'import' ? t('transfer.importFile.title') : t('transfer.requestPayment.title')} 
            description={activeTab === 'import' ? t('transfer.importFile.description') : t('transfer.requestPayment.description')} 
            onBack={onBack}
        >
            <div className="max-w-3xl mx-auto space-y-8">
                {/* Mode Tabs */}
                <div className="flex justify-center">
                    <SegmentedTabs<'import' | 'request'>
                        activeTab={activeTab}
                        onChange={setActiveTab}
                        tabs={[
                            { id: 'import', label: t('transfer.receiveTabs.importFile') },
                            { id: 'request', label: t('transfer.receiveTabs.requestPayment') }
                        ]}
                    />
                </div>

                {feedbackMsg && (
                    <AlertBanner variant="error">
                        {feedbackMsg}
                    </AlertBanner>
                )}

                {activeTab === 'request' ? (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        <div className="bg-white border border-theme-subtle rounded-3xl p-6 shadow-sm space-y-5">
                            <div>
                                <h3 className="text-base font-black text-theme-secondary">
                                    {t('transfer.requestPayment.title')}
                                </h3>
                                <p className="text-xs text-theme-light mt-1">
                                    {t('transfer.requestPayment.description')}
                                </p>
                            </div>

                            <div className="space-y-4 pt-2">
                                {/* Standard Selection */}
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

                                {/* Amount and Unit */}
                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
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

                                {/* Note / Purpose */}
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

                                {/* Is Test Voucher Checkbox */}
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
                            </div>
                        </div>

                        {/* Live Preview Box */}
                        <div className="bg-slate-50 border border-theme-subtle rounded-3xl p-6 space-y-4">
                            <div className="flex items-center justify-between">
                                <h4 className="text-xs font-black uppercase tracking-widest text-theme-light">
                                    {t('transfer.requestPayment.previewTitle')}
                                </h4>
                                <span className="text-[10px] font-bold text-theme-light bg-white border border-theme-subtle/50 px-2 py-0.5 rounded-full">
                                    {t('transfer.requestPayment.readyToShare')}
                                </span>
                            </div>
                            <pre className="p-4 bg-white border border-theme-subtle rounded-2xl text-xs font-mono text-theme-secondary whitespace-pre-wrap break-all shadow-inner">
                                {generatedPaymentRequest}
                            </pre>
                            <Button
                                type="button"
                                onClick={handleCopyPaymentRequest}
                                className="w-full py-3.5 rounded-2xl gap-2 font-bold text-sm shadow-md"
                            >
                                {copiedRequest ? <Check size={18} className="text-emerald-300" /> : <Copy size={18} />}
                                {copiedRequest ? t('transfer.requestPayment.copied') : t('transfer.requestPayment.copyButton')}
                            </Button>
                        </div>
                    </div>
                ) : (
                <div className="space-y-8">
                {/* PoS Mode Toggle */}
                {settings?.enablePosMode && (
                    <div className="flex flex-col sm:flex-row items-center justify-between gap-4 p-4 bg-white/40 border border-theme-subtle/50 rounded-2xl">
                        <div className="flex items-center gap-2">
                            <label className="flex items-center gap-3 cursor-pointer select-none">
                                <input
                                    type="checkbox"
                                    checked={isPosMode}
                                    onChange={(e) => setIsPosMode(e.target.checked)}
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
                                    placeholder={t('transfer.purchaseIdPlaceholder', 'Receipt / Purchase ID (optional)')}
                                    value={purchaseId}
                                    onChange={(e) => setPurchaseId(e.target.value)}
                                    className="text-xs font-mono py-1.5"
                                />
                            </div>
                        )}
                    </div>
                )}

                {/* PoS Trust Fallback Card */}
                {pendingPosFallback && (
                    <div data-testid="pos-fallback-card" className="p-6 bg-amber-500/10 border border-amber-500/30 rounded-3xl space-y-4 animate-in fade-in duration-300">
                        <div className="flex items-start gap-3">
                            <AlertCircle className="text-amber-600 shrink-0 mt-0.5" size={20} />
                            <div className="space-y-1">
                                <h4 className="text-base font-bold text-amber-900">
                                    {t('transfer.checkout.fallbackCardTitle', 'L2 PoS Protection Unavailable')}
                                </h4>
                                <p className="text-xs text-amber-800 leading-relaxed">
                                    {pendingPosFallback.errorCode === 'pos.noServersConfigured'
                                        ? t('transfer.checkout.fallbackNoServersDesc', 'No L2 servers are configured to lock vouchers against double-spends in real time. You can still accept this transfer on a trust basis using pure Layer-1 math (SST V4).')
                                        : t('transfer.checkout.fallbackQuorumDesc', 'Could not reach an L2 quorum to verify lock status. You can either retry or accept on a trust basis (L1 verified).')}
                                </p>
                                <p className="text-[11px] text-amber-700/80 font-medium">
                                    {t('transfer.checkout.fallbackNotice', 'Offline transfers are cryptographically protected by SST V4 self-exposure if a double-spend is attempted.')}
                                </p>
                            </div>
                        </div>
                        <div className="flex items-center justify-end gap-3 pt-2">
                            <button
                                type="button"
                                onClick={handleCancelPosFallback}
                                disabled={isProcessing}
                                className="px-4 py-2 text-xs font-bold text-stone-600 hover:text-stone-900 transition-colors"
                            >
                                {t('transfer.checkout.fallbackCancel', 'Cancel / Reject')}
                            </button>
                            <Button
                                type="button"
                                onClick={handleAcceptPosTrustFallback}
                                disabled={isProcessing}
                                className="bg-amber-600 hover:bg-amber-700 text-white text-xs font-bold px-4 py-2 rounded-xl shadow-md"
                            >
                                {isProcessing ? t('transfer.importFile.loading') : t('transfer.checkout.fallbackAcceptTrust', 'Accept on Trust Basis (L1 Verified)')}
                            </Button>
                        </div>
                    </div>
                )}

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
                                    <div className="flex flex-col items-center gap-1.5 max-w-full">
                                        <p className="text-xs font-mono font-bold text-emerald-600 bg-emerald-50 px-3 py-1 rounded-full border border-emerald-100 max-w-full truncate break-all">
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
            )}
        </div>

            {/* Modals */}
            <ConfirmationModal
                isOpen={showConfirm && !!pendingPasswordImport}
                title={
                    pendingPasswordImport?.type === 'transfer' 
                        ? t('transfer.importFile.importModalTitleTransfer') 
                        : pendingPasswordImport?.type === 'ask' 
                            ? t('transfer.importFile.importModalTitleAsk') 
                            : pendingPasswordImport?.type === 'sig'
                                ? t('transfer.importFile.importModalTitleSig')
                                : t('transfer.wotImport.title')
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
                        if (resultModal.proofId && navigate) {
                            navigate({ view: 'conflict_details', proofId: resultModal.proofId, previousView: { view: 'receive_bundle' } });
                        } else if (resultModal.voucherId) {
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

            {importStandardModalData && (
                <ImportStandardModal
                    isOpen={true}
                    containerBytes={importStandardModalData.containerBytes}
                    initialDefinition={importStandardModalData.definition}
                    onClose={() => setImportStandardModalData(null)}
                    onSuccess={(importedId) => {
                        setResultModal({
                            isOpen: true,
                            title: t('standards.importSuccess'),
                            content: (
                                <div className="space-y-3">
                                    <p className="text-xs font-bold text-theme-primary">{t('standards.importSuccess')}</p>
                                    <p className="text-[10px] font-mono text-theme-light">Standard ID: {importedId}</p>
                                </div>
                            ),
                            confirmText: t('common.ok')
                        });
                        standardsService.getStandards().then(setVoucherStandards).catch(() => {});
                    }}
                />
            )}

            <ModalShell
                isOpen={!!wotImportModalData}
                onClose={handleCloseWotModal}
                maxWidth="md"
                title={
                    <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-2xl bg-emerald-50 text-emerald-600 flex items-center justify-center shrink-0 border border-emerald-100">
                            <CheckCircle2 size={20} />
                        </div>
                        <h3 className="text-lg font-black text-theme-secondary tracking-tight">
                            {wotImportModalData?.isProviderFilter
                                ? t('transfer.wotImport.providerTitle')
                                : t('transfer.wotImport.title')}
                        </h3>
                    </div>
                }
                description={
                    wotImportModalData?.isProviderFilter
                        ? t('transfer.wotImport.providerDescription', { name: senderDisplayName })
                        : t('transfer.wotImport.fromSender', { name: senderDisplayName })
                }
                footer={
                    <div className="flex gap-3">
                        <button
                            type="button"
                            onClick={handleCloseWotModal}
                            className="flex-1 py-2.5 px-4 rounded-xl border border-theme-subtle text-theme-light font-bold text-xs hover:bg-bg-app transition-colors cursor-pointer"
                        >
                            {t('common.close')}
                        </button>
                        {wotImportModalData?.isProviderFilter ? (
                            <Button
                                type="button"
                                onClick={handleSaveProviderTrust}
                                disabled={isSavingProviderTrust}
                                className="flex-[2] py-2.5 px-4 rounded-xl text-xs font-bold"
                            >
                                {isSavingProviderTrust ? t('common.loading') : t('contacts.saveContact')}
                            </Button>
                        ) : (
                            <Button
                                type="button"
                                onClick={handleOpenContactFromWot}
                                className="flex-[2] py-2.5 px-4 rounded-xl text-xs font-bold"
                            >
                                {wotImportModalData?.isKnownContact
                                    ? t('transfer.wotImport.updateContactAction')
                                    : t('transfer.wotImport.rateContactAction')}
                            </Button>
                        )}
                    </div>
                }
            >
                {wotImportModalData && (
                    <div className="space-y-4">
                        <div className="p-4 bg-white rounded-2xl border border-theme-subtle/60 shadow-sm space-y-2">
                            <p className="text-sm font-semibold text-theme-secondary">
                                {wotImportModalData.isProviderFilter
                                    ? t('transfer.wotImport.providerStats', {
                                          count: wotImportModalData.providerMemberCount ?? 0,
                                      })
                                    : t('transfer.wotImport.stats', {
                                          imported: wotImportModalData.imported,
                                          updated: wotImportModalData.updated,
                                      })}
                            </p>
                            <p className="text-[10px] font-mono text-theme-light/60 truncate" title={wotImportModalData.senderDid}>
                                {wotImportModalData.senderDid}
                            </p>
                        </div>

                        {wotImportModalData.isProviderFilter && (
                            <div className="space-y-2 pt-1">
                                <label className="block text-xs font-bold text-theme-light uppercase tracking-widest">
                                    {t('contacts.trustSectionLabel')}
                                </label>
                                <div className="grid grid-cols-4 gap-2">
                                    {[
                                        { level: 0, labelKey: 'contacts.providerTrustInactive' },
                                        { level: 1, labelKey: 'contacts.trustLevel1' },
                                        { level: 2, labelKey: 'contacts.trustLevel2' },
                                        { level: 3, labelKey: 'contacts.trustLevel3' },
                                    ].map((opt) => (
                                        <button
                                            key={opt.level}
                                            type="button"
                                            onClick={() => setSelectedProviderLevel(opt.level)}
                                            className={`py-2 px-1.5 rounded-xl text-xs font-bold border text-center transition-all cursor-pointer ${
                                                selectedProviderLevel === opt.level
                                                    ? 'bg-theme-secondary border-theme-secondary text-white shadow-sm'
                                                    : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                            }`}
                                        >
                                            {t(opt.labelKey)}
                                        </button>
                                    ))}
                                </div>
                            </div>
                        )}
                    </div>
                )}
            </ModalShell>


            {wotContactDialogOpen && (
                <ContactDialog
                    isOpen={wotContactDialogOpen}
                    onClose={() => setWotContactDialogOpen(false)}
                    onSave={handleSaveContactFromWot}
                    existingContact={wotExistingContact}
                    initialDid={wotImportModalData?.senderDid}
                    initialProfile={wotImportModalData?.senderProfile}
                />
            )}
        </PageLayout>
    );
}
