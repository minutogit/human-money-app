import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { 
    Smartphone, 
    ShieldAlert, 
    AlertTriangle, 
    Lock, 
    RefreshCw, 
    LogOut, 
    Trash2, 
    ChevronDown, 
    ChevronUp, 
    Key 
} from "lucide-react";
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { useSession } from '../context/SessionContext';
import { useNavigation } from '../context/NavigationContext';
import { authService } from '../services/authService';
import { profileService } from '../services/profileService';
import { ProfileInfo } from '../types';
import { translateError } from '../utils/errorHelper';

interface ForkLockOverlayProps {
    onStartRecovery: () => void;
}

export function ForkLockOverlay({ onStartRecovery }: ForkLockOverlayProps) {
    const { t } = useTranslation();
    const { lockdownKind, lockdownDetails, profileName, clearLocks, notifyLogout, notifyLogin } = useSession();
    const { navigate } = useNavigation();

    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [showReclaim, setShowReclaim] = useState(false);
    const [reclaimPassword, setReclaimPassword] = useState("");
    const [isReclaiming, setIsReclaiming] = useState(false);
    const [reclaimError, setReclaimError] = useState("");

    const [showDeleteModal, setShowDeleteModal] = useState(false);
    const [deletePassword, setDeletePassword] = useState("");
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteError, setDeleteError] = useState("");

    useEffect(() => {
        authService.listProfiles()
            .then(setProfiles)
            .catch(() => {});
    }, []);

    const activeProfile = profiles.find(p => p.profileName === profileName) || profiles[0];
    const folderName = activeProfile?.folderName || "";
    const displayName = activeProfile?.profileName || profileName || t('profile.unknown');

    const handleLogout = async () => {
        try {
            await profileService.logout();
        } catch {
            // Ignore logout failure when already locked
        }
        notifyLogout();
        clearLocks();
        navigate({ view: "needs_login" });
    };

    const handleReclaim = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!folderName || !reclaimPassword) return;

        setIsReclaiming(true);
        setReclaimError("");

        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await authService.handoverToThisDevice({
                folderName,
                password: reclaimPassword,
                targetInstanceId: localInstanceId,
            });
            clearLocks();
            notifyLogin(displayName);
        } catch (err) {
            setReclaimError(translateError(err, t));
        } finally {
            setIsReclaiming(false);
        }
    };

    const handleDeleteProfile = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!folderName || !deletePassword) return;

        setIsDeleting(true);
        setDeleteError("");

        try {
            await authService.deleteProfile({
                folderName,
                password: deletePassword,
            });
            clearLocks();
            notifyLogout();
            const remaining = await authService.listProfiles().catch(() => []);
            if (remaining.length > 0) {
                navigate({ view: "needs_login" });
            } else {
                navigate({ view: "needs_profile" });
            }
        } catch (err) {
            setDeleteError(translateError(err, t));
        } finally {
            setIsDeleting(false);
        }
    };

    // 1. Handover View: Calm, supportive deactivation
    if (lockdownKind === 'handover') {
        const targetEpoch = lockdownDetails?.targetEpoch;
        return (
            <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 animate-in fade-in duration-200">
                <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border border-amber-500/30">
                    <div className="bg-amber-500 p-6 flex justify-center items-center text-white">
                        <Smartphone className="h-14 w-14" />
                    </div>

                    <div className="p-6 sm:p-8 space-y-6">
                        <div className="text-center space-y-2">
                            <h2 className="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">
                                {t('auth.handoverTitle', 'Wallet Active on Another Device')}
                            </h2>
                            <p className="text-sm text-gray-600 dark:text-gray-300 leading-relaxed">
                                {t('auth.handoverDesc', 'This wallet was transferred to another device (Epoch {{targetEpoch}}). On this device, transactions are deactivated to prevent conflicting spends.', {
                                    targetEpoch: targetEpoch !== undefined ? targetEpoch : '?'
                                })}
                            </p>
                        </div>

                        <div className="space-y-3">
                            <Button 
                                variant="primary" 
                                size="lg" 
                                className="w-full bg-amber-600 hover:bg-amber-700 text-white flex items-center justify-center gap-2 py-3"
                                onClick={handleLogout}
                            >
                                <LogOut size={18} />
                                {t('auth.handoverLogoutBtn', 'Switch Profile / Log Out')}
                            </Button>

                            <Button 
                                variant="secondary" 
                                size="sm" 
                                className="w-full text-rose-600 dark:text-rose-400 border border-rose-200 dark:border-rose-900/40 hover:bg-rose-50 dark:hover:bg-rose-950/20 flex items-center justify-center gap-2"
                                onClick={() => setShowDeleteModal(true)}
                            >
                                <Trash2 size={16} />
                                {t('auth.handoverDeleteBtn', 'Remove Local Profile from This Device')}
                            </Button>
                        </div>

                        {/* Collapsible Reclaim Section */}
                        <div className="pt-2 border-t border-gray-100 dark:border-gray-700">
                            <button
                                type="button"
                                onClick={() => setShowReclaim(!showReclaim)}
                                className="w-full flex items-center justify-between text-xs font-semibold text-gray-500 dark:text-gray-400 hover:text-amber-600 transition-colors py-2"
                            >
                                <span className="flex items-center gap-1.5">
                                    <Key size={14} />
                                    {t('auth.handoverReclaimTitle', 'Reactivate Wallet on This Device')}
                                </span>
                                {showReclaim ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                            </button>

                            {showReclaim && (
                                <form onSubmit={handleReclaim} className="mt-3 space-y-3 text-left animate-in slide-in-from-top-2 duration-150">
                                    <p className="text-xs text-gray-500 dark:text-gray-400">
                                        {t('auth.handoverReclaimDesc', 'To reactivate this device, enter your password. This transfers the active lock to this device and deactivates the other device.')}
                                    </p>
                                    <Input
                                        type="password"
                                        placeholder={t('auth.enterAccessPassword', 'Password')}
                                        value={reclaimPassword}
                                        onChange={(e) => {
                                            setReclaimPassword(e.target.value);
                                            if (reclaimError) setReclaimError("");
                                        }}
                                        className="text-sm py-2"
                                        autoFocus
                                    />
                                    {reclaimError && (
                                        <div className="p-2.5 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/40 rounded-lg text-xs text-rose-600 dark:text-rose-400 flex items-center gap-2">
                                            <AlertTriangle size={14} className="shrink-0" />
                                            <span>{reclaimError}</span>
                                        </div>
                                    )}
                                    <Button
                                        type="submit"
                                        disabled={isReclaiming || !reclaimPassword}
                                        className="w-full bg-amber-600 hover:bg-amber-700 text-white text-xs py-2.5 flex items-center justify-center gap-2"
                                    >
                                        {isReclaiming ? <RefreshCw size={14} className="animate-spin" /> : <Lock size={14} />}
                                        {t('auth.handoverReclaimBtn', 'Reclaim Wallet on This Device')}
                                    </Button>
                                </form>
                            )}
                        </div>
                    </div>
                </div>

                {/* Delete Profile Modal */}
                {showDeleteModal && (
                    <div className="fixed inset-0 z-[110] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
                        <div className="max-w-sm w-full bg-white dark:bg-gray-800 rounded-2xl p-6 shadow-2xl border border-gray-200 dark:border-gray-700 space-y-4">
                            <h3 className="text-lg font-bold text-gray-900 dark:text-white flex items-center gap-2">
                                <Trash2 size={18} className="text-rose-600" />
                                {t('auth.handoverDeleteBtn', 'Remove Local Profile')}
                            </h3>
                            <p className="text-xs text-gray-600 dark:text-gray-300">
                                {t('profile.restoreWarning', 'Deleting this profile removes local voucher data from this device.')}
                            </p>
                            <form onSubmit={handleDeleteProfile} className="space-y-3">
                                <Input
                                    type="password"
                                    placeholder={t('auth.enterAccessPassword', 'Password')}
                                    value={deletePassword}
                                    onChange={(e) => {
                                        setDeletePassword(e.target.value);
                                        if (deleteError) setDeleteError("");
                                    }}
                                    autoFocus
                                />
                                {deleteError && (
                                    <p className="text-xs text-rose-600 dark:text-rose-400 font-medium">
                                        {deleteError}
                                    </p>
                                )}
                                <div className="flex gap-2 pt-2">
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        className="flex-1"
                                        onClick={() => {
                                            setShowDeleteModal(false);
                                            setDeletePassword("");
                                            setDeleteError("");
                                        }}
                                        disabled={isDeleting}
                                    >
                                        {t('common.cancel', 'Cancel')}
                                    </Button>
                                    <Button
                                        type="submit"
                                        className="flex-1 bg-rose-600 hover:bg-rose-700 text-white"
                                        disabled={isDeleting || !deletePassword}
                                    >
                                        {isDeleting ? t('profile.deleting', 'Deleting...') : t('profile.deleteProfile', 'Delete Profile')}
                                    </Button>
                                </div>
                            </form>
                        </div>
                    </div>
                )}
            </div>
        );
    }

    // 2. Clone View: Clear security warning
    if (lockdownKind === 'clone') {
        return (
            <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 animate-in fade-in duration-200">
                <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border border-rose-500/40">
                    <div className="bg-rose-600 p-6 flex justify-center items-center text-white">
                        <ShieldAlert className="h-16 w-16 animate-pulse" />
                    </div>

                    <div className="p-8 text-center space-y-6">
                        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
                            {t('auth.concurrentCloneTitle', 'Security Alert: Concurrent Clone Detected')}
                        </h2>
                        <p className="text-gray-600 dark:text-gray-300 leading-relaxed text-sm">
                            {t('auth.concurrentCloneDesc', 'Multiple devices are actively signing transactions for this wallet with the same credentials. To protect your funds against double-spend penalties, all operations have been halted. Please recover your wallet with your recovery seed to establish a clean state.')}
                        </p>

                        <div className="space-y-3 pt-2">
                            <Button 
                                variant="primary" 
                                size="lg" 
                                className="w-full bg-rose-600 hover:bg-rose-700 text-white border-none py-4 text-base"
                                onClick={onStartRecovery}
                            >
                                {t('auth.recoverWalletSeed', 'Recover Wallet with Seed Phrase')}
                            </Button>

                            <Button
                                variant="secondary"
                                size="sm"
                                className="w-full"
                                onClick={handleLogout}
                            >
                                {t('auth.handoverLogoutBtn', 'Switch Profile / Log Out')}
                            </Button>
                        </div>
                    </div>
                </div>
            </div>
        );
    }

    // 3. Fork / Default Security Lockdown View
    return (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border border-red-500/30">
                <div className="bg-red-500 p-6 flex justify-center">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-16 w-16 text-white animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                    </svg>
                </div>
                
                <div className="p-8 text-center">
                    <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-4">{t('auth.securityLockdown')}</h2>
                    <p className="text-gray-600 dark:text-gray-300 mb-6 leading-relaxed">
                        {t('auth.forkDetected')}
                    </p>
                    
                    <div className="bg-red-50 dark:bg-red-900/20 rounded-lg p-4 mb-8 text-left border border-red-100 dark:border-red-900/30">
                        <p className="text-sm text-red-700 dark:text-red-400 font-medium flex items-center gap-2">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
                            </svg>
                            {t('auth.actionRequired')}
                        </p>
                        <p className="text-sm text-red-600 dark:text-red-500 mt-1">
                            {t('auth.forkRecoveryRequired')}
                        </p>
                    </div>
                    
                    <Button 
                        variant="primary" 
                        size="lg" 
                        className="w-full bg-red-600 hover:bg-red-700 text-white border-none py-4 text-lg"
                        onClick={onStartRecovery}
                    >
                        {t('auth.recoverWalletSeed')}
                    </Button>
                    
                    <p className="text-xs text-theme-subtle mt-4">
                        {t('auth.functionsSuspended')}
                    </p>
                </div>
            </div>
        </div>
    );
}
