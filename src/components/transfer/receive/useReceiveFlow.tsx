// src/components/transfer/receive/useReceiveFlow.ts
import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { settingsService } from '../../../services/settingsService';
import { voucherService } from '../../../services/voucherService';
import { standardsService } from '../../../services/standardsService';
import { profileService } from '../../../services/profileService';
import { postService, POST_INBOX_UPDATED_EVENT } from '../../../services/postService';
import { backupService } from '../../../services/backupService';
import { useSession } from '../../../context/SessionContext';
import { useOptionalNavigation } from '../../../context/NavigationContext';
import { openFileAsBytes } from '../../../utils/fileIo';
import {
    classifyHumocoFile,
    extractHumocoDeepLink,
    getReceiveFileDialogFilters,
    isReceiveImportKind,
    ReceiveImportKind,
} from '../../../utils/humocoFileClassifier';
import { parseContactCard } from '../../../utils/humocoUri';
import { logger } from '../../../utils/log';
import { translateError, isBackendError, stringifyError } from '../../../utils/errorHelper';
import { buildStandardDefinitionsToml, executeReceiveImport } from './receiveImportService';
import { useReceiveWotImport } from './useReceiveWotImport';
import { useReceiveModals } from './useReceiveModals';
import {
    VoucherStandardInfo,
    AppSettings,
    ReceiveSuccessPayload,
    VoucherDetails,
    PublicProfile,
} from '../../../types';

export type {
    PendingPasswordImportState,
    PendingPosFallbackState,
    ResultModalState,
} from './useReceiveModals';

const POS_ACTIVE_STORAGE_KEY = 'humoco_pos_cashier_active';

export interface UseReceiveFlowOptions {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
    droppedFile?: { name: string; bytes: number[] };
}

export function useReceiveFlow({ onBack, onReceiveSuccess, droppedFile }: UseReceiveFlowOptions) {
    const { t } = useTranslation();
    const { protectAction, accessMode } = useSession();
    const nav = useOptionalNavigation();
    const navigate = nav?.navigate;

    const [ownUserId, setOwnUserId] = useState('');
    const [ownProfile, setOwnProfile] = useState<PublicProfile | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [droppedFileContent, setDroppedFileContent] = useState<number[] | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [isProcessing, setIsProcessing] = useState(false);
    const [isDragOver, setIsDragOver] = useState(false);
    const [isPosMode, setIsPosMode] = useState(false);
    const [isSettingsLoaded, setIsSettingsLoaded] = useState(false);
    const [purchaseId, setPurchaseId] = useState('');
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [isCheckingPost, setIsCheckingPost] = useState(false);
    const [postCheckStatus, setPostCheckStatus] = useState<{ variant: 'info' | 'success' | 'error'; message: string } | null>(null);
    const processedDroppedFileRef = useRef<{ name: string; bytes: number[] } | null>(null);
    const autoStagedPosMsgIdRef = useRef<string | null>(null);

    const modalsRef = useRef<ReturnType<typeof useReceiveModals> | null>(null);
    const wotRef = useRef<ReturnType<typeof useReceiveWotImport> | null>(null);

    const clearSelection = useCallback(() => {
        setBundleName(null);
        setDroppedFileContent(null);
        modalsRef.current?.clearModalSelections();
    }, []);

    const resetAllSelections = useCallback(() => {
        clearSelection();
        wotRef.current?.handleCloseWotModal();
    }, [clearSelection]);

    const ensureStandardsToml = useCallback(async (): Promise<Record<string, string>> => {
        let currentStandards = voucherStandards;
        if (currentStandards.length === 0) {
            try {
                currentStandards = await standardsService.getStandards();
                setVoucherStandards(currentStandards);
            } catch {
                // Ignore standards fetch fallback error
            }
        }
        return buildStandardDefinitionsToml(currentStandards);
    }, [voucherStandards]);

    const executeReceiveDirectly = useCallback(
        async (
            fileData: number[],
            type: ReceiveImportKind,
            name: string,
            password?: string,
            forcePos?: boolean,
        ) => {
            setIsProcessing(true);
            setFeedbackMsg('');
            try {
                const outcome = await executeReceiveImport({
                    fileData,
                    type,
                    password,
                    usePosCheckout: Boolean(forcePos || isPosMode || purchaseId.trim().length > 0),
                    purchaseId,
                    onEnsurePurchaseId: setPurchaseId,
                    ensureStandardsToml,
                    protectAction,
                    accessMode,
                });

                if (outcome.kind === 'success') {
                    onReceiveSuccess(outcome.payload);
                } else if (outcome.kind === 'posFallback') {
                    modalsRef.current?.setPendingPosFallback({
                        fileData: outcome.fileData,
                        standardDefinitionsToml: outcome.standardDefinitionsToml,
                        outcomeKind: outcome.outcomeKind,
                        merchantWarning: outcome.merchantWarning,
                        verdict: outcome.verdict,
                        purchaseId: outcome.purchaseId,
                    });
                } else if (outcome.kind === 'signatureAttached') {
                    modalsRef.current?.setResultModal({
                        isOpen: true,
                        title: t('voucher.signatureAttachedTitle'),
                        content: (
                            <p className="text-sm font-medium text-theme-secondary">
                                {t('voucher.signatureAttachedDesc')}
                            </p>
                        ),
                        confirmText: t('transfer.importFile.viewDetails'),
                        voucherId: outcome.voucherId,
                    });
                } else if (outcome.kind === 'wotImported') {
                    wotRef.current?.openWotImportResult(outcome.result);
                } else if (outcome.kind === 'proofImported') {
                    modalsRef.current?.setResultModal({
                        isOpen: true,
                        title: t('conflict.proofImportedTitle'),
                        content: (
                            <p className="text-sm font-medium text-theme-secondary">
                                {t('conflict.proofImportedDesc')}
                            </p>
                        ),
                        confirmText: t('conflict.viewProofBtn'),
                        proofId: outcome.proofId,
                    });
                } else if (outcome.kind === 'cardImported') {
                    if (outcome.status === 'accepted' && outcome.did && navigate) {
                        navigate({ view: 'marketplace', selectedDid: outcome.did });
                    } else {
                        const statusMsgMap: Record<string, string> = {
                            ignored_stale: t('marketplace.importIgnoredDesc'),
                            rejected_expired: t('marketplace.importExpiredDesc'),
                            rejected_hops: t('marketplace.importHopsDesc'),
                            rejected_offender: t('marketplace.importOffenderDesc'),
                        };
                        setFeedbackMsg(statusMsgMap[outcome.status] ?? outcome.status);
                    }
                }
            } catch (e) {
                const errorStr = stringifyError(e);
                const isPasswordRequired =
                    errorStr.includes('Password required') ||
                    errorStr.toLowerCase().includes('decrypt') ||
                    errorStr.includes('SymmetricEncryption') ||
                    errorStr.includes('MacError');
                if (isPasswordRequired) {
                    modalsRef.current?.setPendingPasswordImport({ fileData, type, name });
                    modalsRef.current?.setShowConfirm(true);
                    return;
                }

                const isTolerance =
                    (isBackendError(e) &&
                        (e.code === 'error.transfer.bundleRecoveryZone' ||
                            e.code === 'error.transfer.bundleToleranceZone')) ||
                    errorStr.includes('ToleranceZone');
                if (isTolerance) {
                    const isCritical =
                        (isBackendError(e) && e.code === 'error.transfer.bundleRecoveryZone') ||
                        errorStr.includes('Extended');
                    modalsRef.current?.setToleranceModal({
                        type: isCritical ? 'Critical' : 'Soft',
                        message: translateError(e, t),
                    });
                    return;
                }

                if (errorStr.includes('already attached')) {
                    const match = errorStr.match(/\[LOCAL_ID:([\w-]+)]/);
                    if (match) {
                        modalsRef.current?.setResultModal({
                            isOpen: true,
                            title: t('transfer.importFile.duplicateSignature'),
                            content: (
                                <p className="text-sm font-medium text-theme-secondary">
                                    {t('transfer.importFile.duplicateSignatureDesc')}
                                </p>
                            ),
                            confirmText: t('transfer.importFile.goToAsset'),
                            voucherId: match[1],
                        });
                        return;
                    }
                }

                setFeedbackMsg(translateError(e, t));
                clearSelection();
            } finally {
                setIsProcessing(false);
            }
        },
        [
            accessMode,
            clearSelection,
            ensureStandardsToml,
            isPosMode,
            navigate,
            onReceiveSuccess,
            protectAction,
            purchaseId,
            t,
        ],
    );

    const modals = useReceiveModals({
        onBack,
        onReceiveSuccess,
        navigate,
        protectAction,
        accessMode,
        ensureStandardsToml,
        droppedFileContent,
        setFeedbackMsg,
        setIsProcessing,
        resetAllSelections,
        clearSelection,
        executeReceiveDirectly,
    });
    modalsRef.current = modals;

    const wot = useReceiveWotImport({
        protectAction,
        clearSelection,
        setResultModal: modals.setResultModal,
    });
    wotRef.current = wot;

    useEffect(() => {
        void settingsService.prewarmL2Checkout?.().catch(() => undefined);
    }, []);

    useEffect(() => {
        let active = true;
        async function fetchData() {
            try {
                logger.info('ReceiveView opened, fetching initial data.');
                const [standards, currentSettings, userId, profile] = await Promise.all([
                    standardsService.getStandards(),
                    settingsService.getSettings().catch((e) => {
                        logger.warn(`Failed to fetch app settings: ${translateError(e, t)}`);
                        return null;
                    }),
                    voucherService.getUserId().catch(() => ''),
                    profileService.getProfile().catch(() => null),
                ]);
                if (!active) return;
                setVoucherStandards(standards);
                setSettings(currentSettings);
                const storedLocalPos =
                    typeof localStorage !== 'undefined' &&
                    localStorage.getItem(POS_ACTIVE_STORAGE_KEY) === 'true';
                const initialPosActive = Boolean(
                    currentSettings?.enablePosMode &&
                        (currentSettings?.posCashierActive ?? storedLocalPos)
                );
                if (initialPosActive) {
                    setIsPosMode(true);
                }
                if (typeof localStorage !== 'undefined' && typeof currentSettings?.posCashierActive === 'boolean') {
                    localStorage.setItem(
                        POS_ACTIVE_STORAGE_KEY,
                        currentSettings.posCashierActive ? 'true' : 'false'
                    );
                }
                setOwnUserId(userId);
                setOwnProfile(profile);
            } catch (e) {
                if (!active) return;
                const msg = `Failed to fetch initial data: ${translateError(e, t)}`;
                logger.error(msg);
                setFeedbackMsg(msg);
            } finally {
                if (active) setIsSettingsLoaded(true);
            }
        }
        fetchData();
        return () => {
            active = false;
        };
    }, [t]);

    useEffect(() => {
        if (!isSettingsLoaded || !isPosMode || droppedFile || isProcessing) {
            return;
        }
        const checkStagedPosPayments = () => {
            if (typeof postService?.getPostInboxSummary !== 'function') return;
            postService
                .getPostInboxSummary()
                .then((summary) => {
                    const next = summary?.pendingPosPayments?.[0];
                    if (
                        next &&
                        next.id !== autoStagedPosMsgIdRef.current &&
                        Array.isArray(next.bundleBytes) &&
                        next.bundleBytes.length > 0
                    ) {
                        autoStagedPosMsgIdRef.current = next.id;
                        void executeReceiveDirectly(
                            next.bundleBytes,
                            'transfer',
                            `post-${next.id}.transfer`,
                            undefined,
                            true,
                        );
                    }
                })
                .catch(() => {});
        };
        checkStagedPosPayments();
        window.addEventListener(POST_INBOX_UPDATED_EVENT, checkStagedPosPayments);
        return () => window.removeEventListener(POST_INBOX_UPDATED_EVENT, checkStagedPosPayments);
    }, [isSettingsLoaded, isPosMode, droppedFile, isProcessing, executeReceiveDirectly]);

    const processStandardFile = useCallback(async (fileData: number[]) => {
        setIsProcessing(true);
        try {
            const def = await standardsService.inspectStandardFile(fileData, null);
            modalsRef.current?.setImportStandardModalData({ containerBytes: fileData, definition: def });
        } catch {
            modalsRef.current?.setImportStandardModalData({ containerBytes: fileData });
        } finally {
            setIsProcessing(false);
        }
    }, []);

    const processIncomingFile = useCallback(
        async (fileName: string, bytes: number[], forcePos?: boolean) => {
            setBundleName(fileName);
            setDroppedFileContent(bytes);
            const classification = classifyHumocoFile({ fileName, bytes });
            if (classification.kind === 'deeplink' && classification.deepLink) {
                clearSelection();
                window.dispatchEvent(new CustomEvent('humoco:deeplink', { detail: classification.deepLink }));
                return true;
            }
            if (classification.kind === 'backup') {
                clearSelection();
                const stagedPath = await backupService.stageDroppedBackupBytes(bytes);
                navigate?.({
                    view: 'restore_backup',
                    initialMethod: 'file_or_folder',
                    initialBackupPath: stagedPath,
                });
                return true;
            }
            if (classification.kind === 'standard') {
                await processStandardFile(bytes);
                return true;
            }
            if (isReceiveImportKind(classification.kind)) {
                await executeReceiveDirectly(bytes, classification.kind, fileName, undefined, forcePos);
                return true;
            }
            return false;
        },
        [clearSelection, executeReceiveDirectly, navigate, processStandardFile],
    );

    useEffect(() => {
        if (droppedFile && isSettingsLoaded && processedDroppedFileRef.current !== droppedFile) {
            processedDroppedFileRef.current = droppedFile;
            const forcePos = Boolean(
                settings?.enablePosMode &&
                    (isPosMode ||
                        (settings.posCashierActive ??
                            (typeof localStorage !== 'undefined' &&
                                localStorage.getItem(POS_ACTIVE_STORAGE_KEY) === 'true'))),
            );
            void processIncomingFile(droppedFile.name, droppedFile.bytes, forcePos);
        }
    }, [droppedFile, isSettingsLoaded, isPosMode, processIncomingFile, settings?.enablePosMode, settings?.posCashierActive]);

    useEffect(() => {
        const preventGlobalDefaults = (e: Event) => e.preventDefault();
        document.addEventListener('dragenter', preventGlobalDefaults, true);
        document.addEventListener('dragover', preventGlobalDefaults, true);
        document.addEventListener('dragleave', preventGlobalDefaults, true);
        document.addEventListener('drop', preventGlobalDefaults, true);
        return () => {
            document.removeEventListener('dragenter', preventGlobalDefaults, true);
            document.removeEventListener('dragover', preventGlobalDefaults, true);
            document.removeEventListener('dragleave', preventGlobalDefaults, true);
            document.removeEventListener('drop', preventGlobalDefaults, true);
        };
    }, []);

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const opened = await openFileAsBytes({
                settings,
                protectAction,
                filters: getReceiveFileDialogFilters(t),
            });
            if (!opened) return;
            logger.info(`File selected via dialog: ${opened.path}`);
            if (settings) settingsService.getSettings().then(setSettings).catch(() => {});
            const fileName = opened.path.split(/[/\\]/).pop() || '';
            await processIncomingFile(fileName, opened.bytes);
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        }
    };

    const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(false);
        const files = e.dataTransfer.files;
        if (files.length === 0) return;
        const file = files[0];
        try {
            const buf = await file.arrayBuffer();
            const handled = await processIncomingFile(file.name, Array.from(new Uint8Array(buf)));
            if (!handled) {
                clearSelection();
                setFeedbackMsg(t('transfer.importFile.invalidFileType'));
            }
        } catch (error) {
            setFeedbackMsg(translateError(error, t));
        }
    };

    const handlePosModeToggle = (checked: boolean) => {
        setIsPosMode(checked);
        if (typeof localStorage !== 'undefined') {
            localStorage.setItem(POS_ACTIVE_STORAGE_KEY, checked ? 'true' : 'false');
        }
        if (settings) {
            const nextSettings: AppSettings = { ...settings, posCashierActive: checked };
            setSettings(nextSettings);
            void protectAction(async (pwd) => {
                await settingsService.saveSettings(nextSettings, pwd || undefined);
            }).catch((err) => {
                logger.warn(`Failed to persist PoS cashier mode: ${stringifyError(err)}`);
            });
        }
    };

    const handleCheckPostNow = async () => {
        if (isCheckingPost) return;
        setIsCheckingPost(true);
        setPostCheckStatus(null);
        setFeedbackMsg('');
        try {
            const res = await postService.syncPostChannels();
            let stagedPosPayments = res.pendingPosPayments ?? [];
            if (isPosMode && stagedPosPayments.length === 0 && typeof postService.getPostInboxSummary === 'function') {
                const summary = await postService.getPostInboxSummary().catch(() => null);
                stagedPosPayments = summary?.pendingPosPayments ?? [];
            }
            if (isPosMode && stagedPosPayments.length > 0) {
                const nextPayment = stagedPosPayments[0];
                await executeReceiveDirectly(
                    nextPayment.bundleBytes,
                    'transfer',
                    `post-${nextPayment.id}.transfer`,
                    undefined,
                    true,
                );
                return;
            }
            const incomingCount =
                (res.importedPayments ?? 0) +
                (res.newSignRequests ?? 0) +
                (res.importedSignatures ?? 0);
            if (incomingCount > 0) {
                setPostCheckStatus({
                    variant: 'success',
                    message: t('transfer.receivePostFound', '{{count}} neue Eingänge im Postfach abgerufen.', {
                        count: incomingCount,
                    }),
                });
            } else if ((res.errors?.length ?? 0) > 0) {
                setPostCheckStatus({
                    variant: 'error',
                    message: res.errors[0],
                });
            } else {
                setPostCheckStatus({
                    variant: 'info',
                    message: t('transfer.receivePostEmpty', 'Postfach geprüft – keine neuen Zahlungen oder Anfragen gefunden.'),
                });
            }
        } catch (err) {
            setPostCheckStatus({
                variant: 'error',
                message: translateError(err, t),
            });
        } finally {
            setIsCheckingPost(false);
        }
    };

    const handlePasteRawInput = async (rawText: string): Promise<boolean> => {
        const trimmed = rawText.trim();
        if (!trimmed) {
            setFeedbackMsg(t('common.pasteEmptyInput', 'Bitte zuerst einen Link, Code oder Inhalt einfügen.'));
            return false;
        }
        setFeedbackMsg('');
        const deepLink = extractHumocoDeepLink(trimmed);
        if (deepLink) {
            window.dispatchEvent(new CustomEvent('humoco:deeplink', { detail: deepLink }));
            return true;
        }
        const bytes = Array.from(new TextEncoder().encode(trimmed));
        const handled = await processIncomingFile('eingefuegt.humoco', bytes);
        if (handled) {
            return true;
        }
        const contact = parseContactCard(trimmed);
        if (contact && navigate) {
            navigate({ view: 'address_book', initialContactPayload: trimmed });
            return true;
        }
        setFeedbackMsg(t('transfer.importFile.invalidFileType'));
        return false;
    };

    return {
        ownUserId,
        ownProfile,
        voucherStandards,
        bundleName,
        feedbackMsg,
        setFeedbackMsg,
        isProcessing,
        isCheckingPost,
        postCheckStatus,
        isDragOver,
        setIsDragOver,
        settings,
        isPosMode,
        purchaseId,
        setPurchaseId,
        handlePosModeToggle,
        handleFileSelect,
        handleDrop,
        handleCheckPostNow,
        handlePasteRawInput,
        clearSelection: resetAllSelections,
        closeImportStandardModal: () => modals.setImportStandardModalData(null),
        refreshStandards: () => {
            standardsService.getStandards().then(setVoucherStandards).catch(() => {});
        },
        ...modals,
        ...wot,
    };
}
