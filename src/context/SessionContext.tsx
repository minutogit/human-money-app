// src/context/SessionContext.tsx
import { createContext, ReactNode, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AuthModal } from '../components/ui/AuthModal';
import { clearAdImageCache } from '../components/marketplace/AdCard';
import { useIntegrityCheck } from '../hooks/useIntegrityCheck';
import { useSessionHeartbeat } from '../hooks/useSessionHeartbeat';
import { useWindowTitleSync } from '../hooks/useWindowTitleSync';
import { authService, SessionAccessMode, SessionStatusDto } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { rateLimitManager } from '../sync/rateLimitManager';
import { APP_EVENTS, emitAppEvent } from '../utils/appEvents';
import { isBackendError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';
import type {
    ProtectActionOptions,
    SecurityLockdownDetails,
    SecurityLockdownKind,
    SessionContextType,
} from './session/types';
import { useSessionEventBridge } from './session/useSessionEventBridge';
import { useSessionModalQueue } from './session/useSessionModalQueue';

export type {
    ProtectActionOptions,
    SecurityLockdownDetails,
    SecurityLockdownKind,
    SessionContextType,
} from './session/types';

const SessionContext = createContext<SessionContextType | null>(null);

export function useSession() {
    const context = useContext(SessionContext);
    if (!context) throw new Error('useSession must be used within a SessionProvider');
    return context;
}

export function useOptionalSession() {
    return useContext(SessionContext);
}

export function SessionProvider({ children }: { children: ReactNode }) {
    const { t } = useTranslation();
    const [accessMode, setAccessMode] = useState<SessionAccessMode>('full');
    const [baseAccessMode, setBaseAccessMode] = useState<SessionAccessMode>('full');
    const [hasCachedSession, setHasCachedSession] = useState(false);
    const [isAuthenticated, setIsAuthenticated] = useState(false);
    const [profileName, setProfileName] = useState<string>('');
    const [displayProfileName, setDisplayProfileName] = useState<string>('');
    const [userAvatarUrl, setUserAvatarUrl] = useState<string | null>(null);
    const [folderName, setFolderNameState] = useState<string>('');
    const [isForkLocked, setIsForkLocked] = useState(false);
    const [lockdownKind, setLockdownKind] = useState<SecurityLockdownKind>(null);
    const [lockdownDetails, setLockdownDetails] = useState<SecurityLockdownDetails | null>(null);
    const [isRecoveryRequired, setIsRecoveryRequired] = useState(false);
    const [sessionTimeoutSeconds, setSessionTimeoutSeconds] = useState<number | null>(null);

    const lastHeartbeatRef = useRef(0);
    const accessModeRef = useRef<SessionAccessMode>('full');
    const baseAccessModeRef = useRef<SessionAccessMode>('full');
    const hasCachedSessionRef = useRef(false);
    const isAuthenticatedRef = useRef(false);
    const folderNameRef = useRef<string>('');
    const sessionGenRef = useRef(0);
    const pendingSessionSyncRef = useRef<Promise<void> | null>(null);

    const setFolderName = useCallback((folder: string) => {
        folderNameRef.current = folder;
        setFolderNameState(folder);
    }, []);

    const triggerSecurityLockdown = useCallback(
        (kind: SecurityLockdownKind, details?: SecurityLockdownDetails) => {
            setLockdownKind(kind);
            setLockdownDetails(details || null);
            setIsForkLocked(kind !== null);
        },
        [],
    );

    useEffect(() => {
        hasCachedSessionRef.current = hasCachedSession;
    }, [hasCachedSession]);

    useEffect(() => {
        accessModeRef.current = accessMode;
    }, [accessMode]);

    useEffect(() => {
        baseAccessModeRef.current = baseAccessMode;
    }, [baseAccessMode]);

    useEffect(() => {
        isAuthenticatedRef.current = isAuthenticated;
    }, [isAuthenticated]);

    useEffect(() => {
        folderNameRef.current = folderName;
    }, [folderName]);

    useWindowTitleSync(profileName);

    const { syncNetworkStatusNow } = useSessionEventBridge({
        isAuthenticated,
        isAuthenticatedRef,
        hasCachedSessionRef,
        sessionGenRef,
        setIsAuthenticated,
        setHasCachedSession,
        setIsForkLocked,
        setLockdownKind,
        setIsRecoveryRequired,
        setProfileName,
        setDisplayProfileName,
        setUserAvatarUrl,
    });

    const applySessionStatus = useCallback(
        (status: SessionStatusDto) => {
            const nextAccess: SessionAccessMode =
                status.accessMode === 'dailyPay' ? 'dailyPay' : 'full';
            const nextBase: SessionAccessMode =
                status.baseAccessMode === 'dailyPay' ? 'dailyPay' : 'full';
            const nextCached = Boolean(status.isUnlocked && status.hasCachedSession);

            accessModeRef.current = nextAccess;
            baseAccessModeRef.current = nextBase;
            hasCachedSessionRef.current = nextCached;

            setAccessMode(nextAccess);
            setBaseAccessMode(nextBase);
            setHasCachedSession(nextCached);

            if (nextCached && isAuthenticatedRef.current) {
                lastHeartbeatRef.current = Date.now();
                syncNetworkStatusNow();
            }
        },
        [syncNetworkStatusNow],
    );

    const onStatusSync = useCallback(
        (status: SessionStatusDto) => {
            applySessionStatus(status);
            if (!status.hasCachedSession) {
                emitAppEvent(APP_EVENTS.SESSION_LOCKED);
            }
            // DO NOT clear profileName and DO NOT set isAuthenticated to false
        },
        [applySessionStatus],
    );

    useSessionHeartbeat(
        hasCachedSession,
        onStatusSync,
        lastHeartbeatRef,
        () => sessionGenRef.current,
    );

    const { integrityReport, checkIntegrity, resetIntegrity } = useIntegrityCheck({
        getSessionGen: () => sessionGenRef.current,
    });

    const executeWithSessionCheck = useCallback(
        async <T,>(fn: (pwd: string | null) => Promise<T>, pwd: string | null): Promise<T> => {
            try {
                return await fn(pwd);
            } catch (error) {
                const errMsg = stringifyError(error);
                const errCode = isBackendError(error) ? error.code : '';

                if (
                    errCode === 'error.wallet.locked' ||
                    errCode === 'error.auth.sessionTimeout' ||
                    errCode === 'error.auth.sessionNotActive' ||
                    errCode === 'error.auth.passwordRequired' ||
                    errMsg.includes('Password required') ||
                    errMsg.includes('Session timed out') ||
                    errMsg.includes('Wallet is locked')
                ) {
                    logger.warn(
                        'Backend rejected action (wallet locked/session expired). Resetting frontend session state.',
                    );
                    hasCachedSessionRef.current = false;
                    accessModeRef.current = baseAccessModeRef.current;
                    setHasCachedSession(false);
                    setAccessMode(baseAccessModeRef.current);
                    emitAppEvent(APP_EVENTS.SESSION_LOCKED);
                    throw new Error('SESSION_EXPIRED_RETRY');
                }
                if (
                    errCode === 'error.auth.insufficientAccessMode' ||
                    errMsg.includes('InsufficientAccessMode')
                ) {
                    logger.info(
                        'Backend requires elevated session access mode; prompting step-up auth.',
                    );
                    throw new Error('STEP_UP_REQUIRED_RETRY');
                }
                if (
                    errCode === 'error.security.supersededByHandover' ||
                    errMsg.includes('SupersededByHandover') ||
                    errMsg.includes('transferred to another device')
                ) {
                    logger.warn('Wallet superseded by handover on another device.');
                    const targetEpoch =
                        isBackendError(error) && error.details?.targetEpoch
                            ? parseInt(error.details.targetEpoch, 10)
                            : undefined;
                    const isSeedRecovery =
                        isBackendError(error) && error.details?.isSeedRecovery === 'true';
                    triggerSecurityLockdown('handover', { targetEpoch, isSeedRecovery });
                } else if (
                    errCode === 'error.security.concurrentCloneDetected' ||
                    errMsg.includes('Concurrent clone detected') ||
                    errMsg.includes('WalletConcurrentCloneDetected')
                ) {
                    logger.error('CRITICAL: Concurrent clone detected.');
                    triggerSecurityLockdown('clone');
                } else if (
                    errCode === 'error.security.walletLockedDueToFork' ||
                    errMsg.includes('WalletLockedDueToFork') ||
                    errMsg.includes('SealForkDetected') ||
                    errMsg.includes('Security Lockdown')
                ) {
                    logger.error('CRITICAL: Wallet is locked due to a fork.');
                    triggerSecurityLockdown('fork');
                }
                if (
                    errCode === 'error.security.sealRecoveryRequired' ||
                    errMsg.includes('RequiresSealRecovery') ||
                    errMsg.includes('No local security seal found')
                ) {
                    logger.error('CRITICAL: Wallet requires seal recovery.');
                    setIsRecoveryRequired(true);
                }
                throw error;
            }
        },
        [triggerSecurityLockdown],
    );

    const {
        modalErrorMsg,
        activePending,
        isModalSubmitting,
        enqueuePendingRequest,
        switchActivePendingToPassword,
        handleModalConfirm,
        handleModalCancel,
        rejectAllPending,
    } = useSessionModalQueue({
        t,
        accessModeRef,
        baseAccessModeRef,
        hasCachedSessionRef,
        folderNameRef,
        applySessionStatus,
        executeWithSessionCheck,
    });

    const clearLocks = useCallback(() => {
        setIsForkLocked(false);
        setIsRecoveryRequired(false);
        setLockdownKind(null);
        setLockdownDetails(null);
    }, []);

    const refreshSessionStatus = useCallback(async () => {
        const expectedGen = sessionGenRef.current;
        try {
            const status = await authService.getSessionStatus();
            if (sessionGenRef.current !== expectedGen || !isAuthenticatedRef.current) return;
            applySessionStatus(status);
        } catch {
            // Non-blocking
        }
        try {
            const cfg = await settingsService.getConfig();
            if (sessionGenRef.current !== expectedGen || !isAuthenticatedRef.current) return;
            const timeout = cfg?.securityPolicy?.sessionTimeoutSeconds;
            if (typeof timeout === 'number') {
                setSessionTimeoutSeconds(timeout);
            }
        } catch {
            // Non-blocking
        }
    }, [applySessionStatus]);

    const notifyLogin = useCallback(
        (name?: string, folder?: string) => {
            const loginGen = ++sessionGenRef.current;
            const isCurrent = () => sessionGenRef.current === loginGen && isAuthenticatedRef.current;
            setIsAuthenticated(true);
            isAuthenticatedRef.current = true;
            accessModeRef.current = 'full';
            baseAccessModeRef.current = 'full';
            setAccessMode('full');
            setBaseAccessMode('full');
            setSessionTimeoutSeconds(null);
            clearAdImageCache();
            if (name) setProfileName(name);
            if (folder) {
                folderNameRef.current = folder;
                setFolderNameState(folder);
            }

            const syncPromise = authService
                .getSessionStatus()
                .then((status) => {
                    if (!isCurrent()) return;
                    applySessionStatus(status);
                    void checkIntegrity({
                        allowSilentRepair: status.accessMode === 'full' && status.hasCachedSession,
                    });
                })
                .catch(() => {
                    if (!isCurrent()) return;
                    hasCachedSessionRef.current = false;
                    setHasCachedSession(false);
                    void checkIntegrity({ allowSilentRepair: false });
                });
            pendingSessionSyncRef.current = syncPromise;
            void syncPromise.finally(() => {
                if (pendingSessionSyncRef.current === syncPromise) {
                    pendingSessionSyncRef.current = null;
                }
            });

            Promise.resolve(settingsService.getConfig())
                .then((cfg) => {
                    if (!isCurrent()) return;
                    const timeout = cfg?.securityPolicy?.sessionTimeoutSeconds;
                    if (typeof timeout === 'number') {
                        setSessionTimeoutSeconds(timeout);
                    }
                })
                .catch(() => {});
        },
        [applySessionStatus, checkIntegrity],
    );

    const notifyLogout = useCallback(() => {
        sessionGenRef.current += 1;
        setIsAuthenticated(false);
        isAuthenticatedRef.current = false;
        setHasCachedSession(false);
        hasCachedSessionRef.current = false;
        accessModeRef.current = 'full';
        baseAccessModeRef.current = 'full';
        setAccessMode('full');
        setBaseAccessMode('full');
        setSessionTimeoutSeconds(null);
        clearAdImageCache();
        setProfileName('');
        setDisplayProfileName('');
        setUserAvatarUrl(null);
        folderNameRef.current = '';
        setFolderNameState('');
        clearLocks();
        resetIntegrity();
        rateLimitManager.clearCooldown();
        emitAppEvent(APP_EVENTS.SESSION_LOCKED);
        rejectAllPending('User logged out');
    }, [clearLocks, rejectAllPending, resetIntegrity]);

    const elevateSession = useCallback(
        async (masterPassword: string) => {
            const status = await authService.elevateSessionWithPassword(masterPassword);
            applySessionStatus(status);
        },
        [applySessionStatus],
    );

    const requireFullAccess = useCallback(
        async <T,>(action: (masterPassword: string) => Promise<T>): Promise<T | void> => {
            return enqueuePendingRequest<T>('stepUp', 'password', (pwd) => action(pwd || ''));
        },
        [enqueuePendingRequest],
    );

    const protectAction = useCallback(
        async <T,>(
            action: (password: string | null) => Promise<T>,
            options?: ProtectActionOptions,
        ): Promise<T | void> => {
            try {
                const expectedGen = sessionGenRef.current;
                if (pendingSessionSyncRef.current) {
                    await pendingSessionSyncRef.current;
                    if (sessionGenRef.current !== expectedGen) {
                        throw new Error('Session changed');
                    }
                }
                const config = await settingsService.getConfig();
                if (sessionGenRef.current !== expectedGen) {
                    throw new Error('Session changed');
                }
                const timeout = config.securityPolicy.sessionTimeoutSeconds;
                setSessionTimeoutSeconds(timeout);
                const needsStepUp = Boolean(
                    options?.requireFullAccess &&
                        (accessModeRef.current !== 'full' ||
                            (timeout === 0 && baseAccessModeRef.current !== 'full')),
                );
                const sessionCredentialKind: 'password' | 'pin' =
                    baseAccessModeRef.current === 'dailyPay' ? 'pin' : 'password';

                // Mode A: Always prompt per action (timeout = 0)
                if (timeout === 0) {
                    return enqueuePendingRequest<T>(
                        needsStepUp ? 'stepUp' : 'session',
                        needsStepUp ? 'password' : sessionCredentialKind,
                        action,
                    );
                }

                if (needsStepUp) {
                    return enqueuePendingRequest<T>('stepUp', 'password', action);
                }

                // Mode B: Reuse active session when unlocked and cached
                if (hasCachedSessionRef.current) {
                    try {
                        return await executeWithSessionCheck(action, null);
                    } catch (e) {
                        if (e instanceof Error && e.message === 'SESSION_EXPIRED_RETRY') {
                            return enqueuePendingRequest<T>(
                                'session',
                                baseAccessModeRef.current === 'dailyPay' ? 'pin' : 'password',
                                action,
                            );
                        }
                        if (e instanceof Error && e.message === 'STEP_UP_REQUIRED_RETRY') {
                            return enqueuePendingRequest<T>('stepUp', 'password', action);
                        }
                        throw e;
                    }
                } else {
                    return enqueuePendingRequest<T>('session', sessionCredentialKind, action);
                }
            } catch (e) {
                logger.error(`Error in protectAction: ${stringifyError(e)}`);
                throw e;
            }
        },
        [enqueuePendingRequest, executeWithSessionCheck],
    );

    const lockSession = useCallback(async () => {
        try {
            await authService.lockSession();
        } catch (e) {
            logger.warn(`Failed to lock session in backend: ${stringifyError(e)}`);
        } finally {
            const fallbackBase = baseAccessModeRef.current;
            hasCachedSessionRef.current = false;
            accessModeRef.current = fallbackBase;
            setHasCachedSession(false);
            setAccessMode(fallbackBase);
            emitAppEvent(APP_EVENTS.SESSION_LOCKED);
        }
    }, []);

    const isModalOpen = activePending !== null;

    return (
        <SessionContext.Provider
            value={{
                protectAction,
                requireFullAccess,
                elevateSession,
                accessMode,
                baseAccessMode,
                hasCachedSession,
                isAuthenticated,
                profileName,
                displayProfileName,
                userAvatarUrl,
                folderName,
                setProfileName,
                setFolderName,
                notifyLogin,
                notifyLogout,
                isForkLocked,
                lockdownKind,
                lockdownDetails,
                triggerSecurityLockdown,
                isRecoveryRequired,
                integrityReport,
                checkIntegrity,
                clearLocks,
                lockSession,
                sessionTimeoutSeconds,
                refreshSessionStatus,
            }}
        >
            {children}
            <AuthModal
                key={activePending?.id ?? 'closed'}
                isOpen={isModalOpen}
                onConfirm={handleModalConfirm}
                onCancel={handleModalCancel}
                credentialKind={activePending?.credentialKind ?? 'password'}
                isSubmitting={isModalSubmitting}
                canSwitchToPassword={
                    activePending?.mode === 'session' && activePending?.credentialKind === 'pin'
                }
                onSwitchToPassword={() => switchActivePendingToPassword()}
                title={activePending?.mode === 'stepUp' ? t('auth.stepUpTitle') : undefined}
                description={activePending?.mode === 'stepUp' ? t('auth.stepUpDesc') : undefined}
                errorMsg={modalErrorMsg || undefined}
            />
        </SessionContext.Provider>
    );
}
