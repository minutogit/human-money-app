// src/components/auth/RestoreBackupView.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    ArrowLeft,
    ArrowRight,
    FileArchive,
    KeyRound,
    Users,
} from 'lucide-react';
import { RestoreOutcomeDto } from '../../types';
import { AlertBanner } from '../ui/AlertBanner';
import { BackButton } from '../ui/BackButton';
import { Button } from '../ui/Button';
import { AuthLayout } from './AuthLayout';
import { ResumeSocialRecoveryBanner } from './ResumeSocialRecoveryBanner';
import { SocialRecoveryFlow } from './SocialRecoveryFlow';
import {
    FileSourceDropzoneStep,
    SyncUnreachableCard,
    WordsAndSyncSourceForm,
} from './restore/FileAndSyncSourceStep';
import { PostRestoreSummary } from './restore/PostRestoreSummary';
import { ProfileCollisionResolver } from './restore/ProfileCollisionResolver';
import {
    BackupManifestCard,
    DevicePasswordRestoreForm,
    DirectProfileSetupForm,
    FileUnlockCredentialsCard,
} from './restore/RestoreCredentialsStep';
import { useRestoreBackupFlow } from './restore/useRestoreBackupFlow';

export interface RestoreBackupViewProps {
    onRestoreSuccess: (outcome: RestoreOutcomeDto) => void;
    onBack: () => void;
    initialPayload?: string;
    initialBackupPath?: string;
    initialMethod?: 'social' | 'words_or_sheet' | 'file_or_folder';
}

export const RestoreBackupView: React.FC<RestoreBackupViewProps> = ({
    onRestoreSuccess,
    onBack,
    initialPayload,
    initialBackupPath,
    initialMethod,
}) => {
    const { t } = useTranslation();
    const flow = useRestoreBackupFlow({
        onRestoreSuccess,
        onBack,
        initialPayload,
        initialBackupPath,
        initialMethod,
    });

    const renderInspectedBackupStep = (isSyncSource: boolean) => {
        if (!flow.inspection) return null;
        return (
            <div className="space-y-5">
                <BackupManifestCard
                    inspection={flow.inspection}
                    matchedSyncKeyKind={isSyncSource ? flow.matchedSyncKeyKind : null}
                    syncSourceUrl={isSyncSource ? flow.syncProbe?.sourceUrl : null}
                />
                {flow.activeCollision && (
                    <ProfileCollisionResolver
                        mode="with_backup"
                        collision={flow.activeCollision}
                        inspection={flow.inspection}
                        collisionChoice={flow.collisionChoice}
                        onChoiceChange={flow.setCollisionChoice}
                    />
                )}
                <DevicePasswordRestoreForm
                    collision={flow.activeCollision}
                    collisionChoice={flow.collisionChoice}
                    newPassword={flow.newPassword}
                    confirmNewPassword={flow.confirmNewPassword}
                    isBusy={flow.isBusy}
                    onNewPasswordChange={flow.setNewPassword}
                    onConfirmNewPasswordChange={flow.setConfirmNewPassword}
                    onSubmit={flow.handleRestoreFromInspectedBackup}
                />
            </div>
        );
    };

    const renderDirectOrCollisionWithoutBackup = (showNoSyncHint: boolean) =>
        flow.activeCollision ? (
            <ProfileCollisionResolver
                mode="without_backup"
                collision={flow.activeCollision}
                newPassword={flow.newPassword}
                confirmNewPassword={flow.confirmNewPassword}
                isBusy={flow.isBusy}
                onNewPasswordChange={flow.setNewPassword}
                onConfirmNewPasswordChange={flow.setConfirmNewPassword}
                onSwitchToSubaccount={flow.handleSwitchToSubaccountFromCollision}
                onSubmitResetPassword={flow.handleDirectCollisionResetPassword}
            />
        ) : (
            <DirectProfileSetupForm
                showNoSyncHint={showNoSyncHint}
                userPrefix={flow.seedState.userPrefix}
                profileName={flow.profileName}
                newPassword={flow.newPassword}
                confirmNewPassword={flow.confirmNewPassword}
                isBusy={flow.isBusy}
                onProfileNameChange={flow.setProfileName}
                onNewPasswordChange={flow.setNewPassword}
                onConfirmNewPasswordChange={flow.setConfirmNewPassword}
                onSwitchToSocial={() => flow.setMethod('social')}
                onSubmit={flow.handleDirectOpenProfile}
            />
        );

    return (
        <AuthLayout maxWidth="max-w-3xl">
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2 border-b border-theme-subtle">
                <div data-testid="restore-back-btn">
                    <BackButton onClick={flow.handleHeaderBack} label={t('common.back')} />
                </div>
                <div className="text-right">
                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-primary">
                        {t('backup.openRestoreBadge')}
                    </span>
                </div>
            </div>

            <div className="text-center space-y-1">
                <h1 className="text-2xl font-black text-theme-secondary tracking-tight">
                    {t('profile.backupRestoreTitle')}
                </h1>
                <p className="text-xs text-theme-light font-medium">
                    {t('profile.backupRestoreSubtitle')}
                </p>
            </div>

            {flow.feedback && (
                <AlertBanner variant={flow.feedback.type} onDismiss={() => flow.setFeedback(null)}>
                    {flow.feedback.message}
                </AlertBanner>
            )}

            {flow.method === 'choose' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    <ResumeSocialRecoveryBanner
                        session={flow.activeSocialSession}
                        bannerTestId="resume-social-session-banner"
                        buttonTestId="resume-social-session-btn"
                        onResume={() => flow.setMethod('social')}
                    />

                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                        <button
                            type="button"
                            onClick={() => flow.setMethod('social')}
                            data-testid="choose-social-recovery-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <Users size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restorePrimaryContacts')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restorePrimaryContactsDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-theme-primary pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => {
                                flow.setWordsPhase('input');
                                flow.setMethod('words_or_sheet');
                            }}
                            data-testid="choose-words-or-sheet-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-600 flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <KeyRound size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restoreCardWordsTitle')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restoreCardWordsDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-amber-600 pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => flow.setMethod('file_or_folder')}
                            data-testid="choose-file-or-folder-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <FileArchive size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restoreCardFileTitle')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restoreCardFileDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-theme-primary pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>
                    </div>
                </div>
            )}

            {flow.method === 'social' && (
                <SocialRecoveryFlow
                    mode="restore_new_device"
                    initialPayload={initialPayload}
                    existingSession={flow.activeSocialSession}
                    onBack={() => flow.setMethod('choose')}
                    onSuccess={(res) => {
                        const isSeedOnly = res.outcome !== 'restored_from_sync';
                        const outcome: RestoreOutcomeDto = {
                            folderName: res.folderName,
                            profileName: res.profileName,
                            activeVoucherQueriesCount: res.activeVoucherQueriesCount,
                            reconciledSpentCount: res.reconciledSpentCount,
                            restoredStandardsCount: res.restoredStandardsCount,
                            l2ReconciliationStatus: res.l2ReconciliationStatus,
                            seedOnly: isSeedOnly,
                        };
                        flow.setRestoreOutcome(outcome);
                        flow.setMethod('completed');
                    }}
                />
            )}

            {flow.method === 'words_or_sheet' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <KeyRound className="text-amber-600" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restoreCardWordsTitle')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={flow.handleHeaderBack}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    {flow.wordsPhase === 'input' && (
                        <WordsAndSyncSourceForm
                            initialPayload={initialPayload}
                            focusPrefixTrigger={flow.focusPrefixTrigger}
                            seedIsValid={flow.seedState.isValid}
                            syncInput={flow.syncInput}
                            isBusy={flow.isBusy}
                            onSeedChange={flow.handleSeedStateChange}
                            onSyncInputChange={(val) => {
                                flow.setSyncInput(val);
                                flow.setSyncEdited(true);
                            }}
                            onSubmit={flow.handleWordsContinue}
                        />
                    )}

                    {flow.wordsPhase === 'direct_setup' && (
                        <div className="space-y-5">
                            {renderDirectOrCollisionWithoutBackup(
                                !flow.seedState.userPrefix.trim() && !flow.syncInput.trim()
                            )}
                        </div>
                    )}

                    {flow.wordsPhase === 'sync_not_found' && (
                        <div className="space-y-5">
                            <AlertBanner
                                variant="info"
                                data-testid="sync-not-found-banner"
                                action={
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="xs"
                                        data-testid="check-passphrase-prefix-btn"
                                        onClick={() => flow.setWordsPhase('input')}
                                    >
                                        {t('backup.checkPassphraseOrPrefixBtn')}
                                    </Button>
                                }
                            >
                                {t('backup.syncNotFoundDesc')}
                            </AlertBanner>
                            {renderDirectOrCollisionWithoutBackup(false)}
                        </div>
                    )}

                    {flow.wordsPhase === 'sync_unreachable' && (
                        <SyncUnreachableCard
                            errorMessage={flow.syncProbe?.error}
                            isBusy={flow.isBusy}
                            onRetry={() => void flow.runSyncProbe(flow.lastSyncInputs)}
                            onEditAddress={() => flow.setWordsPhase('input')}
                            onProceedWithoutBackup={() => {
                                flow.setResolvedSyncTargets([]);
                                flow.setWordsPhase('direct_setup');
                            }}
                        />
                    )}

                    {flow.wordsPhase === 'sync_found' && renderInspectedBackupStep(true)}
                </div>
            )}

            {flow.method === 'file_or_folder' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <FileArchive className="text-theme-primary" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restoreCardFileTitle')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={flow.handleHeaderBack}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    {!flow.probe && (
                        <FileSourceDropzoneStep
                            isDragging={flow.isDraggingBackup}
                            onDragChange={flow.setIsDraggingBackup}
                            onDrop={(e) => void flow.handleInlineDrop(e)}
                            onSelectFile={() => void flow.handleSelectFile()}
                            onSelectFolder={() => void flow.handleSelectFolder()}
                        />
                    )}

                    {flow.probe && !flow.inspection && (
                        <FileUnlockCredentialsCard
                            probe={flow.probe}
                            sourcePath={flow.sourcePath}
                            credentialMode={flow.credentialMode}
                            backupPassword={flow.backupPassword}
                            isBusy={flow.isBusy}
                            onChangeSource={() => {
                                flow.setProbe(null);
                                flow.setInspection(null);
                                flow.setActiveCollision(null);
                                flow.setSourcePath('');
                            }}
                            onCredentialModeChange={flow.setCredentialMode}
                            onBackupPasswordChange={flow.setBackupPassword}
                            onFileSeedChange={flow.setFileSeedState}
                            onInspect={() => void flow.handleInspectFileBackup()}
                        />
                    )}

                    {flow.probe && flow.inspection && renderInspectedBackupStep(false)}
                </div>
            )}

            {flow.method === 'completed' && flow.restoreOutcome && (
                <PostRestoreSummary
                    outcome={flow.restoreOutcome}
                    isReconcilingL2={flow.isReconcilingL2}
                    onReconcileL2={() => void flow.handleReconcileL2()}
                    onOpenWallet={() => {
                        if (flow.restoreOutcome) {
                            onRestoreSuccess(flow.restoreOutcome);
                        }
                    }}
                />
            )}
        </AuthLayout>
    );
};
