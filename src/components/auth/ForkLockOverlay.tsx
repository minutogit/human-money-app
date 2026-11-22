import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useSession } from '../../context/SessionContext';
import { useNavigation } from '../../context/NavigationContext';
import { authService } from '../../services/authService';
import { profileService } from '../../services/profileService';
import { ProfileInfo } from '../../types';
import { translateError } from '../../utils/errorHelper';
import { DeviceLockoutModal } from './DeviceLockoutModals';
import { QuickDeleteProfileModal } from './ProfileDeleteModals';

interface ForkLockOverlayProps {
    onStartRecovery: () => void;
}

export function ForkLockOverlay({ onStartRecovery }: ForkLockOverlayProps) {
    const { t } = useTranslation();
    const {
        lockdownKind,
        lockdownDetails,
        profileName,
        folderName: sessionFolderName,
        clearLocks,
        notifyLogout,
        notifyLogin,
    } = useSession();
    const { navigate } = useNavigation();

    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [ownUserId, setOwnUserId] = useState<string | null>(null);
    const [selectedFolderName, setSelectedFolderName] = useState('');
    const [reclaimPassword, setReclaimPassword] = useState('');
    const [isReclaiming, setIsReclaiming] = useState(false);
    const [reclaimError, setReclaimError] = useState('');

    const [showDeleteModal, setShowDeleteModal] = useState(false);
    const [deletePassword, setDeletePassword] = useState('');
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteError, setDeleteError] = useState('');

    useEffect(() => {
        let active = true;
        authService
            .listProfiles()
            .then((res) => {
                if (active) setProfiles(res);
            })
            .catch(() => {});
        profileService
            .getUserId()
            .then((id) => {
                if (active && id) setOwnUserId(id);
            })
            .catch(() => {});
        return () => {
            active = false;
        };
    }, []);

    const uniqueNameMatches = profileName
        ? profiles.filter((p) => p.profileName === profileName)
        : [];
    const autoResolvedProfile =
        (sessionFolderName ? profiles.find((p) => p.folderName === sessionFolderName) : undefined) ??
        (ownUserId ? profiles.find((p) => p.userId && p.userId === ownUserId) : undefined) ??
        (uniqueNameMatches.length === 1 ? uniqueNameMatches[0] : undefined) ??
        (profiles.length === 1 ? profiles[0] : undefined);
    const folderName = autoResolvedProfile?.folderName || selectedFolderName || '';
    const activeProfile = autoResolvedProfile ?? profiles.find((p) => p.folderName === folderName);
    const displayName = activeProfile?.profileName || profileName || t('profile.unknown');

    const handleLogout = async () => {
        try {
            await profileService.logout();
        } catch {
            // Ignore logout failure when already locked
        }
        notifyLogout();
        clearLocks();
        navigate({ view: 'needs_login' });
    };

    const handleReclaim = async () => {
        if (!folderName) {
            setReclaimError(t('auth.forkLockProfileNotResolved'));
            return;
        }
        if (!reclaimPassword) return;

        setIsReclaiming(true);
        setReclaimError('');

        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await authService.handoverToThisDevice({
                folderName,
                password: reclaimPassword,
                targetInstanceId: localInstanceId,
            });
            clearLocks();
            notifyLogin(displayName, folderName || undefined);
        } catch (err) {
            setReclaimError(translateError(err, t));
        } finally {
            setIsReclaiming(false);
        }
    };

    const handleDeleteProfile = async () => {
        if (!folderName) {
            setDeleteError(t('auth.forkLockProfileNotResolved'));
            return;
        }
        if (!deletePassword) return;

        setIsDeleting(true);
        setDeleteError('');

        try {
            await authService.deleteProfile({
                folderName,
                password: deletePassword,
            });
            clearLocks();
            notifyLogout();
            const remaining = await authService.listProfiles().catch(() => []);
            if (remaining.length > 0) {
                navigate({ view: 'needs_login' });
            } else {
                navigate({ view: 'needs_profile' });
            }
        } catch (err) {
            setDeleteError(translateError(err, t));
        } finally {
            setIsDeleting(false);
        }
    };

    const resolvedKind =
        lockdownKind === 'handover' ? 'handover' : lockdownKind === 'clone' ? 'clone' : 'fork';

    const profileSelectorNode =
        !autoResolvedProfile && profiles.length > 1 ? (
            <div className="space-y-1.5 text-left">
                <label
                    htmlFor="fork-lock-profile-select"
                    className="block text-xs font-semibold text-gray-600 dark:text-gray-300"
                >
                    {t('auth.forkLockSelectProfileLabel')}
                </label>
                <select
                    id="fork-lock-profile-select"
                    data-testid="fork-lock-profile-select"
                    value={selectedFolderName}
                    onChange={(e) => {
                        setSelectedFolderName(e.target.value);
                        setReclaimError('');
                        setDeleteError('');
                    }}
                    className="w-full rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 px-3 py-2 text-xs text-gray-800 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-amber-500/40"
                >
                    <option value="">{t('auth.forkLockSelectProfilePlaceholder')}</option>
                    {profiles.map((p) => (
                        <option key={p.folderName} value={p.folderName}>
                            {p.profileName} ({p.folderName})
                        </option>
                    ))}
                </select>
            </div>
        ) : undefined;

    return (
        <>
            <DeviceLockoutModal
                kind={resolvedKind}
                isOpen={true}
                targetEpoch={lockdownDetails?.targetEpoch}
                profileSelector={profileSelectorNode}
                reclaimPassword={reclaimPassword}
                onReclaimPasswordChange={setReclaimPassword}
                onReclaim={handleReclaim}
                isReclaiming={isReclaiming}
                reclaimError={reclaimError}
                onClearReclaimError={() => setReclaimError('')}
                onSwitchOrLogout={handleLogout}
                onStartRecovery={onStartRecovery}
                onRequestDeleteLocal={() => setShowDeleteModal(true)}
                zIndexClass="z-[100]"
            />

            <QuickDeleteProfileModal
                isOpen={showDeleteModal}
                password={deletePassword}
                error={deleteError}
                isDeleting={isDeleting}
                onPasswordChange={setDeletePassword}
                onClearError={() => setDeleteError('')}
                onCancel={() => {
                    setShowDeleteModal(false);
                    setDeletePassword('');
                    setDeleteError('');
                }}
                onConfirm={handleDeleteProfile}
            />
        </>
    );
}
