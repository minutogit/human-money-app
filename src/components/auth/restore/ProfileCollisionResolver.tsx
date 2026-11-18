import React from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';
import { BackupInspectionDto, LocalProfileCollisionDto } from '../../../types';
import { Button } from '../../ui/Button';
import { PasswordPairFields } from '../../ui/PasswordPairFields';

export interface ProfileCollisionWithBackupProps {
    mode: 'with_backup';
    collision: LocalProfileCollisionDto;
    inspection: BackupInspectionDto;
    collisionChoice: 'reset_pw' | 'replace';
    onChoiceChange: (choice: 'reset_pw' | 'replace') => void;
}

export interface ProfileCollisionWithoutBackupProps {
    mode: 'without_backup';
    collision: LocalProfileCollisionDto;
    newPassword: string;
    confirmNewPassword: string;
    isBusy: boolean;
    onNewPasswordChange: (value: string) => void;
    onConfirmNewPasswordChange: (value: string) => void;
    onSwitchToSubaccount: () => void;
    onSubmitResetPassword: (e: React.FormEvent) => void;
}

export type ProfileCollisionResolverProps =
    | ProfileCollisionWithBackupProps
    | ProfileCollisionWithoutBackupProps;

/**
 * Smart local profile collision card guiding the user between resetting the
 * local password (preserving newer local state), replacing with a backup, or
 * setting a subaccount prefix.
 */
export const ProfileCollisionResolver: React.FC<ProfileCollisionResolverProps> = (props) => {
    const { t } = useTranslation();
    const { collision } = props;

    if (props.mode === 'with_backup') {
        const { inspection, collisionChoice, onChoiceChange } = props;
        const localEpoch = collision.localEpoch ?? 0;
        const localNonce = collision.localTxNonce ?? 0;
        const localNewerOrEqual =
            localEpoch > inspection.epoch ||
            (localEpoch === inspection.epoch && localNonce >= inspection.txNonce);

        return (
            <div
                data-testid="local-collision-card"
                className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-4"
            >
                <div className="flex items-start gap-2.5">
                    <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                    <div className="space-y-1">
                        <h4 className="text-sm font-black text-amber-900">
                            {t('backup.collisionDetectedTitle', { name: collision.profileName })}
                        </h4>
                        <p className="text-xs text-amber-800 leading-relaxed">
                            {t('backup.collisionComparisonDesc', {
                                name: collision.profileName,
                                localEpoch,
                                localNonce,
                                backupEpoch: inspection.epoch,
                                backupNonce: inspection.txNonce,
                            })}
                        </p>
                    </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <button
                        type="button"
                        data-testid="collision-choice-reset-pw"
                        onClick={() => onChoiceChange('reset_pw')}
                        className={`p-3.5 rounded-xl border-2 text-left transition-all cursor-pointer ${
                            collisionChoice === 'reset_pw'
                                ? 'bg-white border-theme-primary shadow-sm'
                                : 'bg-white/60 border-amber-200 hover:bg-white'
                        }`}
                    >
                        <div className="flex items-center justify-between gap-2 mb-1">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('backup.collisionResetPasswordBtn', {
                                    name: collision.profileName,
                                })}
                            </span>
                            {localNewerOrEqual && !collision.isLockedOrConflicted && (
                                <span className="px-2 py-0.5 text-[10px] font-bold rounded-full bg-emerald-100 text-emerald-800">
                                    {t('common.recommended')}
                                </span>
                            )}
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('backup.collisionResetPasswordDesc')}
                        </p>
                    </button>

                    <button
                        type="button"
                        data-testid="collision-choice-replace"
                        onClick={() => onChoiceChange('replace')}
                        className={`p-3.5 rounded-xl border-2 text-left transition-all cursor-pointer ${
                            collisionChoice === 'replace'
                                ? 'bg-white border-theme-primary shadow-sm'
                                : 'bg-white/60 border-amber-200 hover:bg-white'
                        }`}
                    >
                        <div className="flex items-center justify-between gap-2 mb-1">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('backup.collisionReplaceProfileBtn', {
                                    name: collision.profileName,
                                })}
                            </span>
                            {(!localNewerOrEqual || collision.isLockedOrConflicted) && (
                                <span className="px-2 py-0.5 text-[10px] font-bold rounded-full bg-emerald-100 text-emerald-800">
                                    {t('common.recommended')}
                                </span>
                            )}
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('backup.collisionReplaceProfileDesc')}
                        </p>
                    </button>
                </div>
            </div>
        );
    }

    const {
        newPassword,
        confirmNewPassword,
        isBusy,
        onNewPasswordChange,
        onConfirmNewPasswordChange,
        onSwitchToSubaccount,
        onSubmitResetPassword,
    } = props;

    return (
        <div
            data-testid="local-collision-card"
            className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-5"
        >
            <div className="flex items-start gap-2.5">
                <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                <div className="space-y-1">
                    <h4 className="text-sm font-black text-amber-900">
                        {t('backup.collisionDetectedTitle', { name: collision.profileName })}
                    </h4>
                    <p className="text-xs text-amber-800 leading-relaxed">
                        {t('backup.collisionWithoutBackupDesc', { name: collision.profileName })}
                    </p>
                </div>
            </div>

            <form
                onSubmit={onSubmitResetPassword}
                className="space-y-3 bg-white p-4 rounded-2xl border border-amber-200"
            >
                <h5 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                    {t('backup.collisionResetPasswordBtn', { name: collision.profileName })}
                </h5>
                <PasswordPairFields
                    password={newPassword}
                    confirmPassword={confirmNewPassword}
                    onPasswordChange={onNewPasswordChange}
                    onConfirmPasswordChange={onConfirmNewPasswordChange}
                    passwordLabel={t('profile.backupRestoreNewPasswordLabel')}
                    confirmLabel={t('profile.backupRestoreConfirmPasswordLabel')}
                    passwordPlaceholder={t('profile.backupRestoreNewPasswordLabel')}
                    confirmPlaceholder={t('profile.backupRestoreConfirmPasswordLabel')}
                    passwordTestId="collision-new-password-input"
                    confirmTestId="collision-confirm-password-input"
                    minLengthHint={null}
                    showToggle
                    autoComplete="new-password"
                />
                <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        data-testid="collision-create-subaccount-btn"
                        onClick={onSwitchToSubaccount}
                    >
                        {t('backup.collisionCreateSubaccountBtn')}
                    </Button>
                    <Button
                        type="submit"
                        variant="primary"
                        size="sm"
                        data-testid="collision-reset-pw-direct-btn"
                        disabled={isBusy}
                    >
                        {isBusy ? t('common.loading') : t('backup.collisionResetPasswordAction')}
                    </Button>
                </div>
            </form>
        </div>
    );
};
