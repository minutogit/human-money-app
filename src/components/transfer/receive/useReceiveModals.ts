// src/components/transfer/receive/useReceiveModals.ts
import React, { useState, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../../../services/transferService';
import { isBackendError, stringifyError, translateError } from '../../../utils/errorHelper';
import { isAuthOrDecryptError } from './receiveImportService';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type { ReceiveImportKind } from '../../../utils/humocoFileClassifier';
import type {
    AppState,
    CheckoutReleaseVerdict,
    ReceiveSuccessPayload,
    VoucherDetails,
    VoucherStandardDefinition,
} from '../../../types';

export interface ResultModalState {
    isOpen: boolean;
    title: string;
    content: React.ReactNode;
    confirmText: string;
    voucherId?: string;
    proofId?: string;
}

export interface PendingPosFallbackState {
    fileData: number[];
    standardDefinitionsToml: Record<string, string>;
    outcomeKind: 'offlineOnly' | 'yellow';
    merchantWarning?: string;
    verdict: CheckoutReleaseVerdict;
    purchaseId: string;
    postPaymentId?: string;
}

export interface PendingPasswordImportState {
    fileData: number[];
    type: ReceiveImportKind;
    name: string;
}

export function useReceiveModals({
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
    onDismissPosPayment,
}: {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
    navigate?: (state: AppState) => void;
    protectAction: ProtectActionFn;
    accessMode?: 'full' | 'dailyPay' | 'receiveOnly';
    ensureStandardsToml: () => Promise<Record<string, string>>;
    droppedFileContent: number[] | null;
    setFeedbackMsg: (msg: string) => void;
    setIsProcessing: (v: boolean) => void;
    resetAllSelections: () => void;
    clearSelection: () => void;
    executeReceiveDirectly: (
        fileData: number[],
        type: ReceiveImportKind,
        name: string,
        password?: string,
    ) => Promise<void>;
    onDismissPosPayment?: (postPaymentId: string) => Promise<void>;
}) {
    const { t } = useTranslation();
    const [showConfirm, setShowConfirm] = useState(false);
    const [bundlePassword, setBundlePassword] = useState('');
    const [resultModal, setResultModal] = useState<ResultModalState | null>(null);
    const [toleranceModal, setToleranceModal] = useState<{
        type: 'Soft' | 'Critical';
        message: string;
    } | null>(null);
    const [confirmText, setConfirmText] = useState('');
    const [pendingPasswordImport, setPendingPasswordImport] =
        useState<PendingPasswordImportState | null>(null);
    const [importStandardModalData, setImportStandardModalData] = useState<{
        containerBytes: number[];
        definition?: VoucherStandardDefinition;
    } | null>(null);
    const [pendingPosFallback, setPendingPosFallback] =
        useState<PendingPosFallbackState | null>(null);
    const tolerancePosFallbackRef = useRef<PendingPosFallbackState | null>(null);

    const runReceiveCall = useCallback(
        async <T,>(fn: (walletPassword: string | null) => Promise<T>): Promise<T | void> => {
            if (accessMode === 'receiveOnly') {
                try {
                    return await fn(null);
                } catch (err) {
                    if (!isAuthOrDecryptError(err)) throw err;
                }
            }
            return await protectAction(fn);
        },
        [accessMode, protectAction]
    );

    const clearModalSelections = useCallback(() => {
        setPendingPasswordImport(null);
        setImportStandardModalData(null);
        setPendingPosFallback(null);
        tolerancePosFallbackRef.current = null;
    }, []);

    const handleConfirmPassword = async () => {
        if (!pendingPasswordImport) return;
        const { fileData, type, name } = pendingPasswordImport;
        setIsProcessing(true);
        setPendingPasswordImport(null);
        setShowConfirm(false);
        await executeReceiveDirectly(fileData, type, name, bundlePassword);
        setBundlePassword('');
    };

    const handleCancelPassword = () => {
        setPendingPasswordImport(null);
        setShowConfirm(false);
        setBundlePassword('');
        setIsProcessing(false);
        resetAllSelections();
    };

    const confirmToleranceImport = async () => {
        const posContext = tolerancePosFallbackRef.current;
        const bundleBytes = posContext?.fileData ?? droppedFileContent;
        if (!toleranceModal || !bundleBytes) return;
        setIsProcessing(true);
        try {
            const standardDefinitionsToml =
                posContext?.standardDefinitionsToml ?? (await ensureStandardsToml());
            const payload = await runReceiveCall(async (password) => {
                return await transferService.receiveBundle({
                    bundleData: bundleBytes,
                    standardDefinitionsToml,
                    password: password || undefined,
                    forceAcceptToleranceBundle: true,
                });
            });
            if (payload) {
                if (posContext) {
                    onReceiveSuccess({
                        ...payload,
                        purchaseId: posContext.purchaseId,
                        newlyCommitted: true,
                        isReplay: false,
                        checkoutVerdict: {
                            ...posContext.verdict,
                            isGoodsReleaseAuthorized: true,
                            requiresMerchantApproval: false,
                        },
                    });
                } else {
                    onReceiveSuccess(payload);
                }
            }
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        } finally {
            tolerancePosFallbackRef.current = null;
            setIsProcessing(false);
            setToleranceModal(null);
            setConfirmText('');
            setShowConfirm(false);
        }
    };

    const handleAcceptPosTrustFallback = async () => {
        if (!pendingPosFallback) return;
        const currentFallback = pendingPosFallback;
        setIsProcessing(true);
        try {
            const payload = await runReceiveCall(async (walletPassword) => {
                return await transferService.receiveBundle({
                    bundleData: currentFallback.fileData,
                    standardDefinitionsToml: currentFallback.standardDefinitionsToml,
                    password: walletPassword || undefined,
                    forceAcceptToleranceBundle: false,
                });
            });
            if (payload) {
                onReceiveSuccess({
                    ...payload,
                    purchaseId: currentFallback.purchaseId,
                    newlyCommitted: true,
                    isReplay: false,
                    checkoutVerdict: {
                        ...currentFallback.verdict,
                        isGoodsReleaseAuthorized: true,
                        requiresMerchantApproval: false,
                    },
                });
            }
        } catch (e) {
            const errorStr = stringifyError(e);
            const isTolerance =
                (isBackendError(e) &&
                    (e.code === 'error.transfer.bundleRecoveryZone' ||
                        e.code === 'error.transfer.bundleToleranceZone')) ||
                errorStr.includes('ToleranceZone');
            if (isTolerance) {
                const isCritical =
                    (isBackendError(e) && e.code === 'error.transfer.bundleRecoveryZone') ||
                    errorStr.includes('Extended');
                tolerancePosFallbackRef.current = currentFallback;
                setToleranceModal({
                    type: isCritical ? 'Critical' : 'Soft',
                    message: translateError(e, t),
                });
                return;
            }
            setFeedbackMsg(translateError(e, t));
        } finally {
            setIsProcessing(false);
            setPendingPosFallback(null);
        }
    };

    return {
        showConfirm,
        setShowConfirm,
        bundlePassword,
        setBundlePassword,
        resultModal,
        setResultModal,
        toleranceModal,
        setToleranceModal,
        confirmText,
        setConfirmText,
        pendingPasswordImport,
        setPendingPasswordImport,
        importStandardModalData,
        setImportStandardModalData,
        pendingPosFallback,
        setPendingPosFallback,
        clearModalSelections,
        handleConfirmPassword,
        handleCancelPassword,
        confirmToleranceImport,
        cancelToleranceImport: () => {
            const postPaymentId = tolerancePosFallbackRef.current?.postPaymentId;
            tolerancePosFallbackRef.current = null;
            setToleranceModal(null);
            setConfirmText('');
            setIsProcessing(false);
            setShowConfirm(false);
            if (postPaymentId && onDismissPosPayment) {
                void onDismissPosPayment(postPaymentId);
            }
        },
        handleAcceptPosTrustFallback,
        handleCancelPosFallback: () => {
            const postPaymentId = pendingPosFallback?.postPaymentId;
            setPendingPosFallback(null);
            clearSelection();
            if (postPaymentId && onDismissPosPayment) {
                void onDismissPosPayment(postPaymentId);
            }
        },
        confirmResultModal: () => {
            if (!resultModal) return;
            if (resultModal.proofId && navigate) {
                navigate({
                    view: 'conflict_details',
                    proofId: resultModal.proofId,
                    previousView: { view: 'receive_bundle' },
                });
            } else if (resultModal.voucherId) {
                onReceiveSuccess({
                    senderId: '',
                    transferSummary: { summableAmounts: {}, countableItems: {} },
                    involvedVouchers: [resultModal.voucherId],
                    isSignatureAttached: true,
                    voucherId: resultModal.voucherId,
                });
            }
            setResultModal(null);
        },
        dismissResultModal: () => {
            setResultModal(null);
            onBack();
        },
    };
}
