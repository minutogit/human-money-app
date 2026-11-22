import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { authService, ProfilePinStatusDto } from '../../services/authService';
import { recoveryService } from '../../services/recoveryService';
import { profileService } from '../../services/profileService';
import { ActiveRecoverySessionDto, ProfileInfo } from '../../types';
import { translateError, isBackendError, stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';

export interface UseLoginFlowParams {
    onLoginSuccess: (profileName: string, folderName?: string) => void;
    onSwitchToCreate: () => void;
}

/**
 * Manages profile loading, PIN vs master password login, device handover,
 * clone alerts, and two-step profile deletion state for the Login view.
 */
export function useLoginFlow({ onLoginSuccess, onSwitchToCreate }: UseLoginFlowParams) {
    const { t } = useTranslation();

    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [selectedProfile, setSelectedProfile] = useState<string>('');
    const [password, setPassword] = useState('');
    const [pinStatus, setPinStatus] = useState<ProfilePinStatusDto | null>(null);
    const [authMethod, setAuthMethod] = useState<'pin' | 'password'>('password');
    const [pin, setPin] = useState('');
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [feedbackType, setFeedbackType] = useState<'error' | 'success' | 'info' | null>(null);

    const [isLoading, setIsLoading] = useState(false);
    const [isLoggingIn, setIsLoggingIn] = useState(false);
    const [showHandoverUI, setShowHandoverUI] = useState(false);
    const [showPostHandoverWarning, setShowPostHandoverWarning] = useState(false);
    const [handoverUserId, setHandoverUserId] = useState('');
    const [showSupersededUI, setShowSupersededUI] = useState(false);
    const [supersededTargetEpoch, setSupersededTargetEpoch] = useState<number | undefined>(
        undefined
    );
    const [showCloneUI, setShowCloneUI] = useState(false);

    const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
    const [showDeletePasswordPrompt, setShowDeletePasswordPrompt] = useState(false);
    const [deletePassword, setDeletePassword] = useState('');
    const [deleteUserId, setDeleteUserId] = useState('');
    const [isDeleting, setIsDeleting] = useState(false);
    const [isVerifyingDelete, setIsVerifyingDelete] = useState(false);
    const [deleteError, setDeleteError] = useState('');
    const [activeSocialSession, setActiveSocialSession] =
        useState<ActiveRecoverySessionDto | null>(null);

    const passwordInputRef = useRef<HTMLInputElement>(null);
    const pinInputRef = useRef<HTMLInputElement>(null);

    const setFeedback = useCallback((msg: string, type: 'error' | 'success' | 'info' | null) => {
        setFeedbackMsg(msg);
        setFeedbackType(type);
    }, []);

    const refreshProfiles = useCallback(async () => {
        setIsLoading(true);
        try {
            const availableProfiles = await authService.listProfiles();
            setProfiles(availableProfiles);
            if (availableProfiles.length > 0 && !selectedProfile) {
                setSelectedProfile(availableProfiles[0].folderName);
            }
        } catch (e) {
            setFeedback(`${t('profile.accessError')}: ${translateError(e, t)}`, 'error');
        } finally {
            setIsLoading(false);
        }
    }, [selectedProfile, setFeedback, t]);

    useEffect(() => {
        let active = true;
        logger.info('Login component displayed');
        (async () => {
            setIsLoading(true);
            try {
                const availableProfiles = await authService.listProfiles();
                if (!active) return;
                setProfiles(availableProfiles);
                if (availableProfiles.length > 0) {
                    setSelectedProfile((prev) => prev || availableProfiles[0].folderName);
                }
            } catch (e) {
                if (!active) return;
                setFeedback(`${t('profile.accessError')}: ${translateError(e, t)}`, 'error');
            } finally {
                if (active) setIsLoading(false);
            }
        })();
        return () => {
            active = false;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useEffect(() => {
        let active = true;
        recoveryService
            .getActiveRecoverySession?.()
            ?.then((session) => {
                if (active && session) {
                    setActiveSocialSession(session);
                }
            })
            ?.catch(() => {});
        return () => {
            active = false;
        };
    }, []);

    useEffect(() => {
        if (!selectedProfile) {
            setPinStatus(null);
            setAuthMethod('password');
            setPin('');
            return;
        }
        let active = true;
        Promise.resolve(authService.getProfilePinStatus?.(selectedProfile))
            .then((dto) => {
                if (!active) return;
                if (dto && dto.enabled) {
                    setPinStatus(dto);
                    setAuthMethod('pin');
                } else {
                    setPinStatus(dto ?? null);
                    setAuthMethod('password');
                }
                setPin('');
            })
            .catch(() => {
                if (!active) return;
                setPinStatus(null);
                setAuthMethod('password');
            });
        return () => {
            active = false;
        };
    }, [selectedProfile]);

    const handleSharedLoginError = useCallback(
        (e: unknown) => {
            const msg = stringifyError(e);
            const code = isBackendError(e) ? e.code : '';
            if (
                code === 'error.security.supersededByHandover' ||
                msg.includes('transferred to another device') ||
                msg.includes('SupersededByHandover')
            ) {
                const epoch =
                    isBackendError(e) && e.details?.targetEpoch
                        ? parseInt(e.details.targetEpoch, 10)
                        : undefined;
                setSupersededTargetEpoch(epoch);
                setShowSupersededUI(true);
            } else if (
                code === 'error.security.concurrentCloneDetected' ||
                msg.includes('Concurrent clone detected') ||
                msg.includes('WalletConcurrentCloneDetected')
            ) {
                setShowCloneUI(true);
            } else if (
                code === 'error.auth.deviceMismatch' ||
                msg.includes('Device Mismatch') ||
                msg.includes('different device')
            ) {
                setFeedback(translateError(e, t), 'error');
                setShowHandoverUI(true);
            } else {
                setFeedback(`${t('profile.verificationFailure')}: ${translateError(e, t)}`, 'error');
            }
        },
        [setFeedback, t]
    );

    const handlePinLogin = async () => {
        if (!selectedProfile || pin.length < 6) {
            setFeedback(t('settings.pinValidationDigits'), 'error');
            return;
        }

        setIsLoggingIn(true);
        setFeedback('', null);
        const profileToLogin = selectedProfile;

        setTimeout(async () => {
            try {
                const localInstanceId = await authService.getLocalInstanceId();
                await authService.loginWithPin({
                    folderName: profileToLogin,
                    pin,
                    cleanupOnLogin: true,
                    localInstanceId,
                });
                const loggedInProfile = profiles.find((p) => p.folderName === profileToLogin);
                if (loggedInProfile) {
                    onLoginSuccess(loggedInProfile.profileName, loggedInProfile.folderName);
                }
            } catch (e) {
                const msg = stringifyError(e);
                const code = isBackendError(e) ? e.code : '';
                if (
                    code === 'error.auth.pinWipedMaxAttempts' ||
                    msg.includes('PIN slot wiped') ||
                    msg.includes('pinWipedMaxAttempts')
                ) {
                    setPinStatus((prev) =>
                        prev
                            ? {
                                  ...prev,
                                  enabled: false,
                                  remainingAttempts: 0,
                                  failedAttempts: prev.maxAttempts,
                              }
                            : null
                    );
                    setAuthMethod('password');
                    setPin('');
                    setFeedback(translateError(e, t), 'error');
                    passwordInputRef.current?.focus();
                } else if (
                    code === 'error.auth.invalidPin' ||
                    msg.includes('Invalid PIN') ||
                    msg.includes('invalidPin')
                ) {
                    setPinStatus((prev) => {
                        if (!prev) return prev;
                        const nextFailed = prev.failedAttempts + 1;
                        return {
                            ...prev,
                            failedAttempts: nextFailed,
                            remainingAttempts: Math.max(0, prev.maxAttempts - nextFailed),
                        };
                    });
                    setPin('');
                    setFeedback(translateError(e, t), 'error');
                    pinInputRef.current?.focus();
                } else {
                    handleSharedLoginError(e);
                }
            } finally {
                setIsLoggingIn(false);
            }
        }, 150);
    };

    const handleLogin = async () => {
        if (!selectedProfile || !password) {
            setFeedback(t('profile.loginSelectProfile'), 'error');
            return;
        }

        setIsLoggingIn(true);
        setFeedback('', null);
        const profileToLogin = selectedProfile;

        setTimeout(async () => {
            try {
                const localInstanceId = await authService.getLocalInstanceId();
                await authService.login({
                    folderName: profileToLogin,
                    password,
                    cleanupOnLogin: true,
                    localInstanceId,
                });
                const loggedInProfile = profiles.find((p) => p.folderName === profileToLogin);
                if (loggedInProfile) {
                    onLoginSuccess(loggedInProfile.profileName, loggedInProfile.folderName);
                }
            } catch (e) {
                handleSharedLoginError(e);
                const errStr = stringifyError(e);
                const errCode = isBackendError(e) ? e.code : '';
                const isDeviceMismatch =
                    errCode === 'error.auth.deviceMismatch' ||
                    errStr.includes('Device Mismatch') ||
                    errStr.includes('different device');
                if (!isDeviceMismatch) {
                    setPassword('');
                    passwordInputRef.current?.focus();
                }
            } finally {
                setIsLoggingIn(false);
            }
        }, 150);
    };

    const handleHandover = async () => {
        setIsLoggingIn(true);
        setFeedback(t('profile.linkingDevice'), 'info');
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await authService.handoverToThisDevice({
                folderName: selectedProfile,
                password,
                targetInstanceId: localInstanceId,
            });
            const userId = await profileService.getUserId();
            setHandoverUserId(userId);
            setShowPostHandoverWarning(true);
        } catch (e) {
            setFeedback(`${t('profile.handoverFailure')}: ${translateError(e, t)}`, 'error');
        } finally {
            setIsLoggingIn(false);
        }
    };

    const handleVerifyDeletePassword = async () => {
        setIsVerifyingDelete(true);
        setDeleteError('');
        try {
            const userId = await authService.verifyProfilePassword({
                folderName: selectedProfile,
                password: deletePassword,
            });
            setDeleteUserId(userId);
            setShowDeletePasswordPrompt(false);
            setShowDeleteConfirm(true);
        } catch (e) {
            setDeleteError(`${t('profile.authenticationError')}: ${translateError(e, t)}`);
        } finally {
            setIsVerifyingDelete(false);
        }
    };

    const handleDeleteProfile = async () => {
        setIsDeleting(true);
        setDeleteError('');
        setFeedback(t('profile.deleting'), 'info');
        try {
            await authService.deleteProfile({
                folderName: selectedProfile,
                password: deletePassword,
            });
            setFeedback(t('profile.deletedSuccess'), 'success');
            setShowDeleteConfirm(false);
            setDeletePassword('');
            await refreshProfiles();
            const availableProfiles = await authService.listProfiles();
            if (availableProfiles.length > 0) {
                setSelectedProfile(availableProfiles[0].folderName);
            } else {
                onSwitchToCreate();
            }
        } catch (e) {
            const errorMsg = `${t('profile.purgeFailure')}: ${translateError(e, t)}`;
            setDeleteError(errorMsg);
            setFeedback(errorMsg, 'error');
        } finally {
            setIsDeleting(false);
        }
    };

    const activeProfile = profiles.find((p) => p.folderName === selectedProfile);
    const activeProfileName = activeProfile?.profileName || t('profile.unknown');

    return {
        profiles,
        selectedProfile,
        setSelectedProfile,
        activeProfileName,
        password,
        setPassword,
        pinStatus,
        authMethod,
        setAuthMethod,
        pin,
        setPin,
        feedbackMsg,
        feedbackType,
        setFeedback,
        isLoading,
        isLoggingIn,
        showHandoverUI,
        setShowHandoverUI,
        showPostHandoverWarning,
        handoverUserId,
        showSupersededUI,
        setShowSupersededUI,
        supersededTargetEpoch,
        showCloneUI,
        setShowCloneUI,
        showDeleteConfirm,
        setShowDeleteConfirm,
        showDeletePasswordPrompt,
        setShowDeletePasswordPrompt,
        deletePassword,
        setDeletePassword,
        deleteUserId,
        isDeleting,
        isVerifyingDelete,
        deleteError,
        setDeleteError,
        activeSocialSession,
        passwordInputRef,
        pinInputRef,
        handlePinLogin,
        handleLogin,
        handleHandover,
        handleVerifyDeletePassword,
        handleDeleteProfile,
    };
}
