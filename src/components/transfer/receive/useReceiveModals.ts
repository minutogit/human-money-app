// src/components/transfer/receive/useReceiveModals.ts
import React, { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { translateError } from '../../../utils/errorHelper';
import { commitPendingReceiveBundle } from './receiveImportService';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type { ReceiveImportKind } from '../../../utils/humocoFileClassifier';
import type {
    AppState,
    ReceiveSuccessPayload,
    ToleranceReceiveConfirmation,
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
    onFinishPendingLaunchItem,
    navigate,
    protectAction,
    setFeedbackMsg,
    setIsProcessing,
    resetAllSelections,
    executeReceiveDirectly,
}: {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload | VoucherDetails) => void;
    onFinishPendingLaunchItem?: () => void;
    navigate?: (state: AppState) => void;
    protectAction: ProtectActionFn;
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
    const [toleranceModal, setToleranceModal] = useState<ToleranceReceiveConfirmation | null>(null);
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
        onFinishPendingLaunchItem?.();
    };

    const confirmToleranceImport = async () => {
        const modal = toleranceModal;
        if (!modal) return;
        const { pendingCommit } = modal;
        setIsProcessing(true);
        try {
            const payload = await commitPendingReceiveBundle({
                pendingCommit,
                forceAcceptToleranceBundle: true,
                commitMode: 'forceTrustOffline',
                protectAction,
            });
            if (payload) {
                onReceiveSuccess(payload);
                onFinishPendingLaunchItem?.();
            }
        } catch (e) {
            setFeedbackMsg(translateError(e, t));
            onFinishPendingLaunchItem?.();
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
            onFinishPendingLaunchItem?.();
        },
        confirmResultModal: () => {
            if (!resultModal) return;
            onFinishPendingLaunchItem?.();
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
            onFinishPendingLaunchItem?.();
            onBack();
        },
    };
}
