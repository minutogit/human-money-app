// src/components/transfer/receive/useReceiveModals.ts
import React, { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../../../services/transferService';
import { translateError } from '../../../utils/errorHelper';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type { ReceiveImportKind } from '../../../utils/humocoFileClassifier';
import type {
    AppState,
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
    ensureStandardsToml,
    droppedFileContent,
    setFeedbackMsg,
    setIsProcessing,
    resetAllSelections,
    executeReceiveDirectly,
}: {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
    navigate?: (state: AppState) => void;
    protectAction: ProtectActionFn;
    ensureStandardsToml: () => Promise<Record<string, string>>;
    droppedFileContent: number[] | null;
    setFeedbackMsg: (msg: string) => void;
    setIsProcessing: (v: boolean) => void;
    resetAllSelections: () => void;
    executeReceiveDirectly: (
        fileData: number[],
        type: ReceiveImportKind,
        name: string,
        password?: string,
    ) => Promise<void>;
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

    const clearModalSelections = useCallback(() => {
        setPendingPasswordImport(null);
        setImportStandardModalData(null);
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
        const bundleBytes = droppedFileContent;
        if (!toleranceModal || !bundleBytes) return;
        setIsProcessing(true);
        try {
            const standardDefinitionsToml = await ensureStandardsToml();
            const payload = await protectAction(async (password) => {
                return await transferService.receiveBundle({
                    bundleData: bundleBytes,
                    standardDefinitionsToml,
                    password: password || undefined,
                    forceAcceptToleranceBundle: true,
                });
            });
            if (payload) {
                onReceiveSuccess(payload);
            }
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
        } finally {
            setIsProcessing(false);
            setToleranceModal(null);
            setConfirmText('');
            setShowConfirm(false);
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
        clearModalSelections,
        handleConfirmPassword,
        handleCancelPassword,
        confirmToleranceImport,
        cancelToleranceImport: () => {
            setToleranceModal(null);
            setConfirmText('');
            setIsProcessing(false);
            setShowConfirm(false);
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
