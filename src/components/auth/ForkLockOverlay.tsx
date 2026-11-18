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

    const activeProfile =
        (sessionFolderName ? profiles.find((p) => p.folderName === sessionFolderName) : undefined) ??
        (ownUserId ? profiles.find((p) => p.userId && p.userId === ownUserId) : undefined) ??
        (profiles.length === 1 ? profiles.find((p) => p.profileName === profileName) : undefined);
    const folderName = activeProfile?.folderName || '';
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
        if (!folderName || !reclaimPassword) return;

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
        if (!folderName || !deletePassword) return;

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

    return (
        <>
            <DeviceLockoutModal
                kind={resolvedKind}
                isOpen={true}
                targetEpoch={lockdownDetails?.targetEpoch}
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
