// src/components/settings/DangerZoneCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Input } from '../ui/Input';
import { useSession } from '../../context/SessionContext';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { authService } from '../../services/authService';
import { profileService } from '../../services/profileService';
import { translateError } from '../../utils/errorHelper';

export const DangerZoneCard: React.FC = () => {
    const { t } = useTranslation();
    const {
        profileName,
        folderName,
        notifyLogout,
        clearLocks,
    } = useSession();
    const nav = useOptionalNavigation();

    const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
    const [deletePassword, setDeletePassword] = useState('');
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteError, setDeleteError] = useState('');

    const handleDeleteProfile = async () => {
        if (!deletePassword.trim()) return;
        setIsDeleting(true);
        setDeleteError('');
        try {
            const profiles = await authService.listProfiles();
            let active = folderName ? profiles.find((p) => p.folderName === folderName) : undefined;
            if (!active) {
                try {
                    const ownUserId = await profileService.getUserId();
                    if (ownUserId) {
                        active = profiles.find((p) => p.userId && p.userId === ownUserId);
                    }
                } catch {
                    // Wallet locked or unavailable; fall back only if unambiguous single profile
                }
            }
            if (!active && profiles.length === 1) {
                active = profiles.find((p) => p.profileName === profileName);
            }
            if (!active) {
                throw new Error(t('settings.activeProfileNotFound'));
            }
            await authService.deleteProfile({
                folderName: active.folderName,
                password: deletePassword,
            });
            setIsDeleteModalOpen(false);
            setDeletePassword('');
            clearLocks();
            notifyLogout();
            const remaining = await authService.listProfiles().catch(() => []);
            if (remaining.length > 0) {
                nav?.navigate({ view: 'needs_login' });
            } else {
                nav?.navigate({ view: 'needs_profile' });
            }
        } catch (err) {
            setDeleteError(translateError(err, t));
        } finally {
            setIsDeleting(false);
        }
    };

    return (
        <>
            <div className="p-6 rounded-2xl border-2 border-rose-500/30 bg-rose-500/5 space-y-4">
                <div className="flex items-start gap-3">
                    <AlertTriangle size={20} className="text-rose-600 dark:text-rose-400 mt-0.5 shrink-0" />
                    <div className="space-y-1">
                        <h4 className="text-xs font-black uppercase tracking-wider text-rose-700 dark:text-rose-300">
                            {t('settings.dangerZoneTitle')}
                        </h4>
                        <p className="text-xs text-rose-800/80 dark:text-rose-200/80 leading-relaxed font-medium">
                            {t('settings.dangerZoneDesc')}
                        </p>
                    </div>
                </div>

                {deleteError && !isDeleteModalOpen && (
                    <AlertBanner variant="error">{deleteError}</AlertBanner>
                )}

                <div className="pt-2">
                    <Button
                        type="button"
                        variant="danger"
                        onClick={() => {
                            setDeleteError('');
                            setDeletePassword('');
                            setIsDeleteModalOpen(true);
                        }}
                        className="gap-2 text-xs"
                    >
                        <Trash2 size={14} />
                        <span>{t('settings.deleteProfileLocalBtn')}</span>
                    </Button>
                </div>
            </div>

            <ConfirmationModal
                isOpen={isDeleteModalOpen}
                title={t('settings.deleteProfileConfirmTitle')}
                description={
                    <div className="space-y-4 pt-1 text-left">
                        <p>{t('settings.deleteProfileConfirmDesc')}</p>
                        <div className="space-y-1.5">
                            <label
                                htmlFor="danger-zone-delete-password"
                                className="block text-xs font-bold text-theme-secondary"
                            >
                                {t('auth.enterPasswordToContinue')}
                            </label>
                            <Input
                                id="danger-zone-delete-password"
                                type="password"
                                value={deletePassword}
                                onChange={(e) => {
                                    setDeletePassword(e.target.value);
                                    if (deleteError) setDeleteError('');
                                }}
                                placeholder={t('auth.passwordPlaceholder')}
                                autoComplete="current-password"
                                disabled={isDeleting}
                            />
                        </div>
                        {deleteError && <AlertBanner variant="error">{deleteError}</AlertBanner>}
                    </div>
                }
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeleting}
                confirmDisabled={isDeleting || !deletePassword.trim()}
                onConfirm={handleDeleteProfile}
                onCancel={() => {
                    setIsDeleteModalOpen(false);
                    setDeletePassword('');
                    setDeleteError('');
                }}
            />
        </>
    );
};

export default DangerZoneCard;
