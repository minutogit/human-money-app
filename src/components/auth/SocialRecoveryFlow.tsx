// src/components/auth/SocialRecoveryFlow.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Users } from 'lucide-react';
import type { ActiveRecoverySessionDto, RecoveryCompletionResultDto } from '../../types';
import { AlertBanner } from '../ui/AlertBanner';
import { QrScannerModal } from '../ui/QrScannerModal';
import { RecoveryCompletionStep } from './social-recovery/RecoveryCompletionStep';
import { RecoveryRequestAndSharesStep } from './social-recovery/RecoveryRequestAndSharesStep';
import { StartRecoveryStep } from './social-recovery/StartRecoveryStep';
import { useSocialRecoveryFlow } from './social-recovery/useSocialRecoveryFlow';

export interface SocialRecoveryFlowProps {
    mode: 'restore_new_device' | 'reset_local_password';
    targetFolderName?: string;
    initialPayload?: string;
    existingSession?: ActiveRecoverySessionDto | null;
    onSuccess: (result: RecoveryCompletionResultDto) => void;
    onBack?: () => void;
}

export const SocialRecoveryFlow: React.FC<SocialRecoveryFlowProps> = ({
    mode,
    targetFolderName,
    initialPayload,
    existingSession,
    onSuccess,
    onBack,
}) => {
    const { t } = useTranslation();
    const flow = useSocialRecoveryFlow({
        mode,
        targetFolderName,
        initialPayload,
        existingSession,
        onSuccess,
        t,
    });

    return (
        <div className="space-y-5 animate-in fade-in duration-300">
            <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                <div className="flex items-center gap-2">
                    <Users className="text-theme-primary" size={20} />
                    <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                        {t('backup.restorePrimaryContacts')}
                    </h3>
                </div>
                {onBack && (
                    <button
                        type="button"
                        onClick={onBack}
                        data-testid="social-flow-back-btn"
                        className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                    >
                        <ArrowLeft size={14} />
                        <span>{t('common.back')}</span>
                    </button>
                )}
            </div>

            {flow.errorMsg && (
                <AlertBanner variant="error" onDismiss={() => flow.setErrorMsg('')}>
                    {flow.errorMsg}
                </AlertBanner>
            )}
            {flow.successMsg && (
                <AlertBanner variant="success" onDismiss={() => flow.setSuccessMsg('')}>
                    {flow.successMsg}
                </AlertBanner>
            )}
            {flow.infoMsg && (
                <AlertBanner variant="info" onDismiss={() => flow.setInfoMsg('')}>
                    {flow.infoMsg}
                </AlertBanner>
            )}

            {!flow.session && (
                <StartRecoveryStep
                    conflictingSession={flow.conflictingSession}
                    isSubmitting={flow.isSubmitting}
                    isLoadingSession={flow.isLoadingSession}
                    onStartSession={flow.handleStartSession}
                />
            )}

            {flow.session && (
                <div className="space-y-5">
                    <RecoveryRequestAndSharesStep
                        session={flow.session}
                        copiedChatMsg={flow.copiedChatMsg}
                        copiedRequest={flow.copiedRequest}
                        showRequestQr={flow.showRequestQr}
                        requestQrSvg={flow.requestQrSvg}
                        confirmCancel={flow.confirmCancel}
                        replyInput={flow.replyInput}
                        isSubmitting={flow.isSubmitting}
                        onBack={onBack}
                        onCopyChatMessage={flow.handleCopyRequestChatMessage}
                        onCopyRequestLink={flow.handleCopyRequestLink}
                        onToggleRequestQr={flow.handleToggleRequestQr}
                        onCopySasCode={flow.handleCopySasCode}
                        onSetConfirmCancel={flow.setConfirmCancel}
                        onCancelSession={flow.handleCancelSession}
                        onReplyInputChange={flow.setReplyInput}
                        onOpenReplyQr={() => flow.setIsReplyQrOpen(true)}
                        onImportReply={() => void flow.handleImportReply()}
                    />

                    <RecoveryCompletionStep
                        mode={mode}
                        session={flow.session}
                        unreachableSyncError={flow.unreachableSyncError}
                        syncEmptyResult={flow.syncEmptyResult}
                        collisionResult={flow.collisionResult}
                        answers={flow.answers}
                        showPassphraseInput={flow.showPassphraseInput}
                        bip39Passphrase={flow.bip39Passphrase}
                        profileName={flow.profileName}
                        newPassword={flow.newPassword}
                        confirmNewPassword={flow.confirmNewPassword}
                        isPasswordValid={flow.isPasswordValid}
                        isProfileNameValid={flow.isProfileNameValid}
                        isSubmitting={flow.isSubmitting}
                        onClearSyncEmpty={() => flow.setSyncEmptyResult(null)}
                        onAnswersChange={flow.setAnswers}
                        onBip39PassphraseChange={flow.setBip39Passphrase}
                        onProfileNameChange={flow.setProfileName}
                        onNewPasswordChange={flow.setNewPassword}
                        onConfirmNewPasswordChange={flow.setConfirmNewPassword}
                        onSubmitCompletion={(opts) => void flow.submitRecoveryCompletion(opts)}
                        onFormSubmit={flow.handleFormSubmit}
                    />
                </div>
            )}

            <QrScannerModal
                isOpen={flow.isReplyQrOpen}
                onClose={() => flow.setIsReplyQrOpen(false)}
                onScan={(decoded) => {
                    flow.setReplyInput(decoded);
                    void flow.handleImportReply(decoded);
                }}
            />
        </div>
    );
};
