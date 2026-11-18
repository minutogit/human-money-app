// src/components/settings/DangerZoneCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
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
        protectAction,
        accessMode = 'full',
        requireFullAccess,
    } = useSession();
    const nav = useOptionalNavigation();

    const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteError, setDeleteError] = useState('');

    const handleDeleteProfile = async () => {
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
            await protectAction(async (pwd) => {
                await authService.deleteProfile({
                    folderName: active.folderName,
                    password: pwd || undefined,
                });
            });
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
            setIsDeleteModalOpen(false);
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

                {deleteError && <AlertBanner variant="error">{deleteError}</AlertBanner>}

                <div className="pt-2">
                    <Button
                        type="button"
                        variant="danger"
                        onClick={() => {
                            if (accessMode !== 'full' && requireFullAccess) {
                                void requireFullAccess(async () => {
                                    setIsDeleteModalOpen(true);
                                }).catch(() => undefined);
                                return;
                            }
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
                description={t('settings.deleteProfileConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeleting}
                onConfirm={handleDeleteProfile}
                onCancel={() => {
                    setIsDeleteModalOpen(false);
                    setDeleteError('');
                }}
            />
        </>
    );
};

export default DangerZoneCard;
