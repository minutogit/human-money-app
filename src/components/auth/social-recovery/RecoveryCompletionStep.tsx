import React, { type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, CheckCircle2, KeyRound } from 'lucide-react';
import type { ActiveRecoverySessionDto, RecoveryCompletionResultDto } from '../../../types';
import { AlertBanner } from '../../ui/AlertBanner';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { QUESTION_CATALOG } from './useSocialRecoveryFlow';

interface RecoveryCompletionStepProps {
    mode: 'restore_new_device' | 'reset_local_password';
    session: ActiveRecoverySessionDto;
    unreachableSyncError: boolean;
    syncEmptyResult: RecoveryCompletionResultDto | null;
    collisionResult: RecoveryCompletionResultDto | null;
    answers: [string, string, string];
    showPassphraseInput: boolean;
    bip39Passphrase: string;
    profileName: string;
    newPassword: string;
    confirmNewPassword: string;
    isPasswordValid: boolean;
    isProfileNameValid: boolean;
    isSubmitting: boolean;
    onClearSyncEmpty: () => void;
    onAnswersChange: (answers: [string, string, string]) => void;
    onBip39PassphraseChange: (val: string) => void;
    onProfileNameChange: (val: string) => void;
    onNewPasswordChange: (val: string) => void;
    onConfirmNewPasswordChange: (val: string) => void;
    onSubmitCompletion: (options?: {
        overrideMode?: 'restore_new_device' | 'reset_local_password';
        overrideTargetFolder?: string;
        allowEmptyFallback?: boolean;
        replaceFolderName?: string;
    }) => void;
    onFormSubmit: (e: FormEvent) => void;
}

export const RecoveryCompletionStep: React.FC<RecoveryCompletionStepProps> = ({
    mode,
    session,
    unreachableSyncError,
    syncEmptyResult,
    collisionResult,
    answers,
    showPassphraseInput,
    bip39Passphrase,
    profileName,
    newPassword,
    confirmNewPassword,
    isPasswordValid,
    isProfileNameValid,
    isSubmitting,
    onClearSyncEmpty,
    onAnswersChange,
    onBip39PassphraseChange,
    onProfileNameChange,
    onNewPasswordChange,
    onConfirmNewPasswordChange,
    onSubmitCompletion,
    onFormSubmit,
}) => {
    const { t } = useTranslation();

    const collision = collisionResult?.localCollision;
    const hasOnlineBackup =
        collisionResult?.backupEpoch !== null && collisionResult?.backupEpoch !== undefined;

    return (
        <>
            {/* Unreachable Sync Offer Secondary Empty Start */}
            {unreachableSyncError && (
                <AlertBanner
                    variant="warning"
                    data-testid="social-unreachable-warning"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            size="xs"
                            data-testid="social-unreachable-empty-btn"
                            disabled={isSubmitting}
                            onClick={() => onSubmitCompletion({ allowEmptyFallback: true })}
                        >
                            {t('backup.proceedWithoutBackupLink')}
                        </Button>
                    }
                >
                    {t('backup.socialSyncUnreachableHint')}
                </AlertBanner>
            )}

            {/* Calm Confirm when Sync Service had no backup */}
            {syncEmptyResult && (
                <div
                    data-testid="social-sync-empty-confirm"
                    className="p-4 bg-blue-50 border border-blue-200 rounded-2xl space-y-3"
                >
                    <p className="text-xs font-semibold text-blue-900 leading-relaxed">
                        {t('backup.socialSyncEmptyNotice')}
                    </p>
                    <div className="flex justify-end gap-2">
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={onClearSyncEmpty}
                        >
                            {t('common.back')}
                        </Button>
                        <Button
                            type="button"
                            variant="primary"
                            size="sm"
                            data-testid="social-confirm-empty-btn"
                            disabled={isSubmitting}
                            onClick={() => onSubmitCompletion({ allowEmptyFallback: true })}
                        >
                            {t('backup.openWithoutBackupAction')}
                        </Button>
                    </div>
                </div>
            )}

            {/* Local Collision Choice Card */}
            {collisionResult && collision && (
                <div
                    data-testid="social-collision-card"
                    className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-4"
                >
                    <div className="flex items-start gap-2.5">
                        <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                        <div className="space-y-1">
                            <h4 className="text-sm font-black text-amber-900">
                                {t('backup.collisionDetectedTitle', {
                                    name: collision.profileName,
                                })}
                            </h4>
                            <p className="text-xs text-amber-800 leading-relaxed">
                                {hasOnlineBackup
                                    ? t('backup.collisionComparisonDesc', {
                                          name: collision.profileName,
                                          localEpoch: collision.localEpoch ?? 0,
                                          localNonce: collision.localTxNonce ?? 0,
                                          backupEpoch: collisionResult.backupEpoch ?? 0,
                                          backupNonce: collisionResult.backupTxNonce ?? 0,
                                      })
                                    : t('backup.collisionLocalOnlyDesc', {
                                          name: collision.profileName,
                                      })}
                            </p>
                        </div>
                    </div>

                    <div className="flex flex-col sm:flex-row gap-2 justify-end">
                        <Button
                            type="button"
                            variant="primary"
                            size="sm"
                            data-testid="social-collision-reset-pw-btn"
                            disabled={isSubmitting}
                            onClick={() =>
                                onSubmitCompletion({
                                    overrideMode: 'reset_local_password',
                                    overrideTargetFolder: collision.folderName,
                                })
                            }
                        >
                            <KeyRound size={14} className="mr-1.5" />
                            {t('backup.collisionResetPasswordBtn', {
                                name: collision.profileName,
                            })}
                        </Button>
                        {hasOnlineBackup && (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                data-testid="social-collision-replace-btn"
                                disabled={isSubmitting}
                                onClick={() =>
                                    onSubmitCompletion({
                                        replaceFolderName: collision.folderName,
                                    })
                                }
                            >
                                {t('backup.collisionReplaceProfileBtn', {
                                    name: collision.profileName,
                                })}
                            </Button>
                        )}
                    </div>
                </div>
            )}

            {/* Step 3 Form */}
            {session.hasEnoughShares && session.questionIds && (
                <form
                    onSubmit={onFormSubmit}
                    className="space-y-4 pt-2 border-t border-theme-subtle/60"
                >
                    <h4 className="text-xs font-bold uppercase tracking-wider text-theme-secondary">
                        {t('backup.socialStep3Title')}
                    </h4>

                    {session.questionIds.map((qId, idx) => (
                        <div key={idx} className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary block">
                                {idx + 1}. {t(QUESTION_CATALOG[qId] || 'backup.recoveryQuestion1')}
                            </label>
                            <Input
                                type="text"
                                data-testid={`social-answer-${idx}`}
                                value={answers[idx]}
                                onChange={(e) => {
                                    const next = [...answers] as [string, string, string];
                                    next[idx] = e.target.value;
                                    onAnswersChange(next);
                                }}
                                className="text-xs"
                            />
                        </div>
                    ))}

                    {showPassphraseInput && (
                        <div className="space-y-1.5 p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl">
                            <label className="text-xs font-bold text-theme-secondary block">
                                {t('backup.bip39PassphraseRequired')}
                            </label>
                            <Input
                                type="password"
                                showToggle
                                data-testid="social-passphrase-input"
                                value={bip39Passphrase}
                                onChange={(e) => onBip39PassphraseChange(e.target.value)}
                                className="text-xs"
                            />
                        </div>
                    )}

                    {mode === 'restore_new_device' && (
                        <div className="space-y-1.5 pt-2">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.profileNameLabel')}
                            </label>
                            <Input
                                type="text"
                                data-testid="social-profile-name-input"
                                value={profileName}
                                onChange={(e) => onProfileNameChange(e.target.value)}
                                placeholder={t('profile.profileNamePlaceholder')}
                                className="text-xs"
                            />
                            <p className="text-[11px] text-theme-light">
                                {t('backup.localProfileNameHint')}
                            </p>
                        </div>
                    )}

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.newPasswordLabel')}
                            </label>
                            <Input
                                type="password"
                                showToggle
                                data-testid="social-new-password-input"
                                value={newPassword}
                                onChange={(e) => onNewPasswordChange(e.target.value)}
                                placeholder={t('auth.passwordMinChars')}
                                className="text-xs"
                            />
                        </div>
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.confirmNewPasswordLabel')}
                            </label>
                            <Input
                                type="password"
                                showToggle
                                data-testid="social-confirm-password-input"
                                value={confirmNewPassword}
                                onChange={(e) => onConfirmNewPasswordChange(e.target.value)}
                                placeholder={t('auth.repeatPassword')}
                                className="text-xs"
                            />
                        </div>
                    </div>

                    <div className="flex justify-end pt-3">
                        <Button
                            type="submit"
                            variant="primary"
                            data-testid="social-complete-submit-btn"
                            disabled={
                                !isPasswordValid ||
                                !isProfileNameValid ||
                                !answers[0] ||
                                !answers[1] ||
                                !answers[2] ||
                                isSubmitting
                            }
                            className="flex items-center gap-2"
                        >
                            <CheckCircle2 size={16} />
                            {isSubmitting
                                ? t('common.loading')
                                : t('backup.restoreActionComplete')}
                        </Button>
                    </div>
                </form>
            )}
        </>
    );
};
