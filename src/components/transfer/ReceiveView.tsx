// src/components/transfer/ReceiveView.tsx
import { useTranslation } from 'react-i18next';
import { PageLayout } from '../ui/PageLayout';
import { AlertBanner } from '../ui/AlertBanner';
import type { PendingReceiveCommit, ReceiveSuccessPayload, VoucherDetails } from '../../types';
import { useReceiveFlow } from './receive/useReceiveFlow';
import { ReceivePaymentRequestCard } from './receive/ReceivePaymentRequestCard';
import { ReceiveInputStep } from './receive/ReceiveInputStep';
import { ReceivePreviewStep } from './receive/ReceivePreviewStep';

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (
        payload: ReceiveSuccessPayload | VoucherDetails,
        pendingCommit?: PendingReceiveCommit,
    ) => void;
    droppedFile?: { name: string; bytes: number[] };
}

export function ReceiveView({ onBack, onReceiveSuccess, droppedFile }: ReceiveViewProps) {
    const { t } = useTranslation();
    const flow = useReceiveFlow({ onBack, onReceiveSuccess, droppedFile });

    return (
        <PageLayout
            title={t('transfer.receiveTitle')}
            description={t('transfer.myCodeDesc')}
            onBack={onBack}
        >
            <div className="max-w-3xl mx-auto space-y-8">
                {flow.feedbackMsg && (
                    <AlertBanner variant="error">{flow.feedbackMsg}</AlertBanner>
                )}

                <ReceivePaymentRequestCard
                    ownUserId={flow.ownUserId}
                    ownProfile={flow.ownProfile}
                    voucherStandards={flow.voucherStandards}
                    onCopyError={flow.setFeedbackMsg}
                />

                <ReceiveInputStep
                    isProcessing={flow.isProcessing}
                    isCheckingPost={flow.isCheckingPost}
                    postCheckStatus={flow.postCheckStatus}
                    onCheckPostNow={() => void flow.handleCheckPostNow()}
                    onPasteRawInput={flow.handlePasteRawInput}
                    isDragOver={flow.isDragOver}
                    bundleName={flow.bundleName}
                    onDragOver={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        flow.setIsDragOver(true);
                    }}
                    onDragLeave={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        flow.setIsDragOver(false);
                    }}
                    onDrop={flow.handleDrop}
                    onSelectFile={flow.handleFileSelect}
                    onClearSelection={flow.clearSelection}
                />
            </div>

            <ReceivePreviewStep
                showConfirm={flow.showConfirm}
                pendingPasswordImport={flow.pendingPasswordImport}
                bundlePassword={flow.bundlePassword}
                onBundlePasswordChange={flow.setBundlePassword}
                onConfirmPassword={flow.handleConfirmPassword}
                onCancelPassword={flow.handleCancelPassword}
                isProcessing={flow.isProcessing}
                toleranceModal={flow.toleranceModal}
                confirmText={flow.confirmText}
                onConfirmTextChange={flow.setConfirmText}
                onConfirmTolerance={flow.confirmToleranceImport}
                onCancelTolerance={flow.cancelToleranceImport}
                resultModal={flow.resultModal}
                onSetResultModal={flow.setResultModal}
                onConfirmResult={flow.confirmResultModal}
                onDismissResult={flow.dismissResultModal}
                importStandardModalData={flow.importStandardModalData}
                onCloseImportStandard={flow.closeImportStandardModal}
                onRefreshStandards={flow.refreshStandards}
                wotImportModalData={flow.wotImportModalData}
                selectedProviderLevel={flow.selectedProviderLevel}
                onSelectProviderLevel={flow.setSelectedProviderLevel}
                isSavingProviderTrust={flow.isSavingProviderTrust}
                onSaveProviderTrust={flow.handleSaveProviderTrust}
                onOpenContactFromWot={flow.handleOpenContactFromWot}
                onCloseWotModal={flow.handleCloseWotModal}
                wotContactDialogOpen={flow.wotContactDialogOpen}
                onCloseWotContactDialog={() => flow.setWotContactDialogOpen(false)}
                wotExistingContact={flow.wotExistingContact}
                onSaveContactFromWot={flow.handleSaveContactFromWot}
            />
        </PageLayout>
    );
}
