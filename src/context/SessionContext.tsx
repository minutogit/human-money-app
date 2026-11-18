// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { authService, SessionAccessMode } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { marketService } from '../services/marketService';
import { logger } from '../utils/log';
import { MY_PROFILE_UPDATED_EVENT } from '../utils/syncEvents';
import { FrontendMyMarketProfile } from '../types';
import { AuthModal } from '../components/ui/AuthModal';
import { useTranslation } from 'react-i18next';

import { startSealSyncLoop, stopSealSyncLoop } from '../sync/sealSync';
import { startAutoBackupLoop, stopAutoBackupLoop } from '../sync/autoBackup';
import { startPostSyncLoop, stopPostSyncLoop } from '../sync/postSync';
import { startMarketSyncLoop, stopMarketSyncLoop } from '../sync/marketSync';
import { channelMembersSyncManager } from '../sync/channelMembersSyncManager';
import { rateLimitManager } from '../sync/rateLimitManager';
import { APP_EVENTS, emitAppEvent } from '../utils/appEvents';
import { useWindowTitleSync } from '../hooks/useWindowTitleSync';
import { useIntegrityCheck } from '../hooks/useIntegrityCheck';
import { useSessionHeartbeat } from '../hooks/useSessionHeartbeat';
import { isBackendError, stringifyError, translateError } from '../utils/errorHelper';
import { tourBridge } from '../utils/tourBridge';

export type SecurityLockdownKind = 'handover' | 'clone' | 'fork' | null;

export interface SecurityLockdownDetails {
    targetEpoch?: number;
    isSeedRecovery?: boolean;
}

export interface SessionContextType {
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>;
    requireFullAccess: <T>(action: (masterPassword: string) => Promise<T>) => Promise<T | void>;
    elevateSession: (masterPassword: string) => Promise<void>;
    accessMode: SessionAccessMode;
    isSessionActive: boolean;
    isAuthenticated?: boolean;
    profileName: string;
    displayProfileName?: string;
    userAvatarUrl?: string | null;
    folderName?: string;
    setProfileName: (name: string) => void;
    setFolderName?: (folder: string) => void;
    notifyLogin: (name?: string, folder?: string) => void;
    notifyLogout: () => void;
    isForkLocked: boolean;
    lockdownKind: SecurityLockdownKind;
    lockdownDetails: SecurityLockdownDetails | null;
    triggerSecurityLockdown: (kind: SecurityLockdownKind, details?: SecurityLockdownDetails) => void;
    isRecoveryRequired: boolean;
    integrityReport: import('../types').IntegrityReport | null;
    checkIntegrity: () => Promise<void>;
    clearLocks: () => void;
    lockSession: () => Promise<void>;
}

interface PendingModalRequest {
    id: number;
    mode: 'session' | 'stepUp';
    resolve: (value: unknown) => void;
    reject: (reason: unknown) => void;
    action: (password: string | null) => Promise<unknown>;
}

const SessionContext = createContext<SessionContextType | null>(null);

export function useSession() {
    const context = useContext(SessionContext);
    if (!context) throw new Error("useSession must be used within a SessionProvider");
    return context;
}

export function useOptionalSession() {
    return useContext(SessionContext);
}

export function SessionProvider({ children }: { children: ReactNode }) {
    const { t } = useTranslation();
    const [modalErrorMsg, setModalErrorMsg] = useState<string>('');
    const [accessMode, setAccessMode] = useState<SessionAccessMode>('full');
    const [isSessionActive, setIsSessionActive] = useState(false);
    const [isAuthenticated, setIsAuthenticated] = useState(false);
    const [profileName, setProfileName] = useState<string>("");
    const [displayProfileName, setDisplayProfileName] = useState<string>("");
    const [userAvatarUrl, setUserAvatarUrl] = useState<string | null>(null);
    const [folderName, setFolderName] = useState<string>("");
    const [isForkLocked, setIsForkLocked] = useState(false);
    const [lockdownKind, setLockdownKind] = useState<SecurityLockdownKind>(null);
    const [lockdownDetails, setLockdownDetails] = useState<SecurityLockdownDetails | null>(null);
    const [isRecoveryRequired, setIsRecoveryRequired] = useState(false);
    const [activePending, setActivePending] = useState<PendingModalRequest | null>(null);

    const triggerSecurityLockdown = useCallback((kind: SecurityLockdownKind, details?: SecurityLockdownDetails) => {
        setLockdownKind(kind);
        setLockdownDetails(details || null);
        if (kind !== null) {
            setIsForkLocked(true);
        } else {
            setIsForkLocked(false);
        }
    }, []);

    // Refs for throttling, session state, and modal prompt queue
    const lastHeartbeatRef = useRef(0);
    const isSessionActiveRef = useRef(false);
    const isAuthenticatedRef = useRef(false);
    const pendingQueueRef = useRef<PendingModalRequest[]>([]);
    const nextPendingIdRef = useRef(1);
    const isModalSubmittingRef = useRef(false);
    const syncLoopsRunningRef = useRef(false);

    const startSyncLoops = useCallback((_syncPassword?: string) => {
        if (syncLoopsRunningRef.current) {
            return;
        }
        syncLoopsRunningRef.current = true;
        startSealSyncLoop();
        startAutoBackupLoop();
        startPostSyncLoop();
        startMarketSyncLoop();
    }, []);

    const stopSyncLoops = useCallback(() => {
        syncLoopsRunningRef.current = false;
        stopSealSyncLoop();
        stopAutoBackupLoop();
        stopPostSyncLoop();
        stopMarketSyncLoop();
    }, []);

    // Side-effect hooks
    useWindowTitleSync(profileName);

    const onSessionExpired = useCallback(() => {
        setIsSessionActive(false);
        isSessionActiveRef.current = false;
        stopSyncLoops();
        // DO NOT clear profileName and DO NOT set isAuthenticated to false
    }, [stopSyncLoops]);

    useSessionHeartbeat(isSessionActive, onSessionExpired, lastHeartbeatRef);

    const { integrityReport, checkIntegrity } = useIntegrityCheck();

    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    useEffect(() => {
        isAuthenticatedRef.current = isAuthenticated;
    }, [isAuthenticated]);

    useEffect(() => {
        return () => {
            stopSyncLoops();
            if (pendingQueueRef.current.length > 0) {
                const remaining = pendingQueueRef.current;
                pendingQueueRef.current = [];
                remaining.forEach((entry) => entry.reject(new Error("SessionProvider unmounted")));
            }
        };
    }, [stopSyncLoops]);

    const activateSession = useCallback(() => {
        setIsSessionActive(true);
        isSessionActiveRef.current = true;
        lastHeartbeatRef.current = Date.now();
    }, []);

    const enqueuePendingRequest = useCallback(
        <T,>(
            mode: 'session' | 'stepUp',
            action: (password: string | null) => Promise<T>,
        ): Promise<T> => {
            return new Promise<T>((resolve, reject) => {
                const entry: PendingModalRequest = {
                    id: nextPendingIdRef.current++,
                    mode,
                    resolve: resolve as (value: unknown) => void,
                    reject,
                    action: action as (password: string | null) => Promise<unknown>,
                };
                pendingQueueRef.current.push(entry);
                if (pendingQueueRef.current.length === 1) {
                    setModalErrorMsg('');
                    setActivePending(entry);
                }
            });
        },
        [],
    );

    const notifyLogin = useCallback((name?: string, folder?: string) => {
        setIsAuthenticated(true);
        isAuthenticatedRef.current = true;
        if (name) setProfileName(name);
        if (folder) setFolderName(folder);
        activateSession();
        startSyncLoops();
        checkIntegrity();

        authService.getSessionAccessMode?.()
            ?.then((mode) => {
                if (mode === 'full' || mode === 'dailyPay' || mode === 'receiveOnly') {
                    setAccessMode(mode);
                } else {
                    setAccessMode('full');
                }
            })
            ?.catch(() => setAccessMode('full'));

        authService.isSessionActive()
            .then((active) => {
                if (active) {
                    activateSession();
                    startSyncLoops();
                } else {
                    setIsSessionActive(false);
                    isSessionActiveRef.current = false;
                    stopSyncLoops();
                }
            })
            .catch(() => {
                setIsSessionActive(false);
                isSessionActiveRef.current = false;
                stopSyncLoops();
            });
    }, [activateSession, checkIntegrity, startSyncLoops, stopSyncLoops]);

    const notifyLogout = useCallback(() => {
        setIsAuthenticated(false);
        isAuthenticatedRef.current = false;
        setIsSessionActive(false);
        isSessionActiveRef.current = false;
        setAccessMode('full');
        setProfileName("");
        setDisplayProfileName("");
        setUserAvatarUrl(null);
        setFolderName("");
        stopSyncLoops();
        channelMembersSyncManager.reset();
        rateLimitManager.clearCooldown();
        emitAppEvent(APP_EVENTS.SESSION_LOCKED);

        if (pendingQueueRef.current.length > 0) {
            const remaining = pendingQueueRef.current;
            pendingQueueRef.current = [];
            setActivePending(null);
            setModalErrorMsg('');
            remaining.forEach((entry) => entry.reject(new Error("User logged out")));
        }
    }, [stopSyncLoops]);

    const elevateSession = useCallback(async (masterPassword: string) => {
        const nextMode = await authService.elevateSessionWithPassword(masterPassword);
        setAccessMode(nextMode);
        activateSession();
        if (isAuthenticatedRef.current) {
            startSyncLoops(masterPassword);
        }
    }, [activateSession, startSyncLoops]);

    const requireFullAccess = useCallback(async <T,>(action: (masterPassword: string) => Promise<T>): Promise<T | void> => {
        return enqueuePendingRequest<T>('stepUp', (pwd) => action(pwd || ''));
    }, [enqueuePendingRequest]);

    const protectAction = useCallback(async <T,>(action: (password: string | null) => Promise<T>): Promise<T | void> => {
        const executeWithSessionCheck = async (pwd: string | null): Promise<T> => {
            try {
                return await action(pwd);
            } catch (error) {
                const errMsg = stringifyError(error);
                const errCode = isBackendError(error) ? error.code : '';

                if (errCode === 'error.wallet.locked' || errCode === 'error.auth.sessionTimeout' || errCode === 'error.auth.passwordRequired'
                    || errMsg.includes("Password required") || errMsg.includes("Session timed out") || errMsg.includes("Wallet is locked")) {
                    logger.warn("Backend rejected action (wallet locked/session expired). Resetting frontend session state.");
                    setIsSessionActive(false);
                    isSessionActiveRef.current = false;
                    stopSyncLoops();
                    throw new Error("SESSION_EXPIRED_RETRY");
                }
                if (errCode === 'error.security.supersededByHandover'
                    || errMsg.includes("SupersededByHandover") || errMsg.includes("transferred to another device")) {
                    logger.warn("Wallet superseded by handover on another device.");
                    const targetEpoch = isBackendError(error) && error.details?.targetEpoch ? parseInt(error.details.targetEpoch, 10) : undefined;
                    const isSeedRecovery = isBackendError(error) && error.details?.isSeedRecovery === 'true';
                    triggerSecurityLockdown('handover', { targetEpoch, isSeedRecovery });
                } else if (errCode === 'error.security.concurrentCloneDetected'
                    || errMsg.includes("Concurrent clone detected") || errMsg.includes("WalletConcurrentCloneDetected")) {
                    logger.error("CRITICAL: Concurrent clone detected.");
                    triggerSecurityLockdown('clone');
                } else if (errCode === 'error.security.walletLockedDueToFork'
                    || errMsg.includes("WalletLockedDueToFork") || errMsg.includes("SealForkDetected") || errMsg.includes("Security Lockdown")) {
                    logger.error("CRITICAL: Wallet is locked due to a fork.");
                    triggerSecurityLockdown('fork');
                }
                if (errCode === 'error.security.sealRecoveryRequired'
                    || errMsg.includes("RequiresSealRecovery") || errMsg.includes("No local security seal found")) {
                    logger.error("CRITICAL: Wallet requires seal recovery.");
                    setIsRecoveryRequired(true);
                }
                throw error;
            }
        };

        try {
            const settings = await settingsService.getSettings();
            const timeout = settings.sessionTimeoutSeconds;

            // Mode A: Always prompt for password (timeout = 0)
            if (timeout === 0) {
                return enqueuePendingRequest<T>('session', action);
            }

            // Mode B: Reuse active session when unlocked
            if (isSessionActiveRef.current) {
                try {
                    return await executeWithSessionCheck(null);
                } catch (e) {
                    if (e instanceof Error && e.message === "SESSION_EXPIRED_RETRY") {
                        return enqueuePendingRequest<T>('session', action);
                    }
                    throw e;
                }
            } else {
                return enqueuePendingRequest<T>('session', action);
            }
        } catch (e) {
            logger.error(`Error in protectAction: ${stringifyError(e)}`);
            throw e;
        }
    }, [enqueuePendingRequest, stopSyncLoops, triggerSecurityLockdown]);

    const runPendingEntry = async (entry: PendingModalRequest, password: string | null) => {
        try {
            const result = await entry.action(password);
            entry.resolve(result);
        } catch (actionErr) {
            entry.reject(actionErr);
        }
    };

    const handleModalConfirm = async (password: string) => {
        const currentPending = pendingQueueRef.current[0];
        if (!currentPending || isModalSubmittingRef.current) return;
        isModalSubmittingRef.current = true;

        try {
            if (currentPending.mode === 'stepUp') {
                try {
                    await elevateSession(password);
                } catch (err) {
                    isModalSubmittingRef.current = false;
                    setModalErrorMsg(translateError(err, t));
                    return;
                }

                setModalErrorMsg('');
                pendingQueueRef.current.shift();

                let timeout = 0;
                try {
                    const settings = await settingsService.getSettings();
                    timeout = settings.sessionTimeoutSeconds;
                } catch {
                    timeout = 0;
                }

                const toRun: Array<{ entry: PendingModalRequest; pwd: string | null }> = [
                    { entry: currentPending, pwd: password },
                ];

                while (pendingQueueRef.current.length > 0) {
                    const next = pendingQueueRef.current[0];
                    if (next.mode === 'stepUp') {
                        pendingQueueRef.current.shift();
                        toRun.push({ entry: next, pwd: password });
                    } else if (next.mode === 'session' && timeout > 0 && isSessionActiveRef.current) {
                        pendingQueueRef.current.shift();
                        toRun.push({ entry: next, pwd: null });
                    } else {
                        break;
                    }
                }

                setActivePending(pendingQueueRef.current[0] ?? null);
                isModalSubmittingRef.current = false;
                await Promise.all(toRun.map((item) => runPendingEntry(item.entry, item.pwd)));
                return;
            }

            let timeout = 0;
            try {
                const settings = await settingsService.getSettings();
                timeout = settings.sessionTimeoutSeconds;
            } catch (err) {
                isModalSubmittingRef.current = false;
                setModalErrorMsg(translateError(err, t));
                return;
            }

            if (timeout > 0) {
                try {
                    const unlocked = await authService.unlockSession(password, timeout);
                    if ((unlocked as unknown) === false) {
                        throw new Error('Invalid password');
                    }
                } catch (err) {
                    isModalSubmittingRef.current = false;
                    setModalErrorMsg(translateError(err, t));
                    return;
                }

                setModalErrorMsg('');
                activateSession();
                if (isAuthenticatedRef.current) {
                    startSyncLoops(password);
                }
                logger.info(`Session unlocked via modal for ${timeout} seconds.`);

                pendingQueueRef.current.shift();
                const toRun: PendingModalRequest[] = [currentPending];
                while (pendingQueueRef.current.length > 0 && pendingQueueRef.current[0].mode === 'session') {
                    toRun.push(pendingQueueRef.current.shift()!);
                }

                setActivePending(pendingQueueRef.current[0] ?? null);
                isModalSubmittingRef.current = false;
                await Promise.all(toRun.map((entry) => runPendingEntry(entry, null)));
            } else {
                // Mode A (timeout === 0): prompt once per queued request
                setModalErrorMsg('');
                pendingQueueRef.current.shift();
                setActivePending(pendingQueueRef.current[0] ?? null);
                isModalSubmittingRef.current = false;
                await runPendingEntry(currentPending, password);
            }
        } finally {
            isModalSubmittingRef.current = false;
        }
    };

    const handleModalCancel = () => {
        const cancelled = pendingQueueRef.current;
        pendingQueueRef.current = [];
        isModalSubmittingRef.current = false;
        setModalErrorMsg('');
        setActivePending(null);
        for (const entry of cancelled) {
            entry.reject(new Error("User cancelled authentication"));
        }
    };

    const lockSession = useCallback(async () => {
        try {
            await authService.lockSession();
        } catch (e) {
            logger.warn(`Failed to lock session in backend: ${stringifyError(e)}`);
        } finally {
            setIsSessionActive(false);
            isSessionActiveRef.current = false;
            stopSyncLoops();
            emitAppEvent(APP_EVENTS.SESSION_LOCKED);
        }
    }, [stopSyncLoops]);

    const clearLocks = useCallback(() => {
        setIsForkLocked(false);
        setIsRecoveryRequired(false);
        setLockdownKind(null);
        setLockdownDetails(null);
    }, []);

    useEffect(() => {
        const applySummary = (mp: FrontendMyMarketProfile | undefined | null) => {
            if (!mp?.profile) return;
            const fullName = [mp.profile.firstName, mp.profile.lastName]
                .map((s) => s?.trim())
                .filter(Boolean)
                .join(' ');
            setDisplayProfileName(fullName || mp.profile.organization?.trim() || '');
            setUserAvatarUrl(mp.profile.pictureUrl?.trim() || null);
        };

        const loadSummary = () => {
            if (!isSessionActive || accessMode === 'receiveOnly') return;
            if (typeof marketService?.getMyMarketProfile !== 'function') return;
            Promise.resolve(marketService.getMyMarketProfile())
                .then((mp) => applySummary(mp))
                .catch(() => {});
        };

        loadSummary();

        const onProfileUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendMyMarketProfile | undefined>;
            if (custom.detail?.profile) {
                applySummary(custom.detail);
            } else {
                loadSummary();
            }
        };

        window.addEventListener(MY_PROFILE_UPDATED_EVENT, onProfileUpdated);
        return () => {
            window.removeEventListener(MY_PROFILE_UPDATED_EVENT, onProfileUpdated);
        };
    }, [isSessionActive, accessMode]);

    useEffect(() => {
        return tourBridge.subscribe((partial) => {
            if (partial.isAuthenticated !== undefined) {
                setIsAuthenticated(partial.isAuthenticated);
                isAuthenticatedRef.current = partial.isAuthenticated;
            }
            if (partial.isSessionActive !== undefined) {
                setIsSessionActive(partial.isSessionActive);
                isSessionActiveRef.current = partial.isSessionActive;
            }
            if (partial.isForkLocked !== undefined) {
                setIsForkLocked(partial.isForkLocked);
                setLockdownKind(partial.isForkLocked ? 'fork' : null);
            }
            if (partial.isRecoveryRequired !== undefined) {
                setIsRecoveryRequired(partial.isRecoveryRequired);
            }
            if (partial.profileName !== undefined) {
                setProfileName(partial.profileName);
            }
        });
    }, []);

    const isModalOpen = activePending !== null;

    return (
        <SessionContext.Provider value={{ protectAction, requireFullAccess, elevateSession, accessMode, isSessionActive, isAuthenticated, profileName, displayProfileName, userAvatarUrl, folderName, setProfileName, setFolderName, notifyLogin, notifyLogout, isForkLocked, lockdownKind, lockdownDetails, triggerSecurityLockdown, isRecoveryRequired, integrityReport, checkIntegrity, clearLocks, lockSession }}>
            {children}
            <AuthModal
                key={activePending?.id ?? 'closed'}
                isOpen={isModalOpen}
                onConfirm={handleModalConfirm}
                onCancel={handleModalCancel}
                title={activePending?.mode === 'stepUp' ? t('auth.stepUpTitle') : undefined}
                description={activePending?.mode === 'stepUp' ? t('auth.stepUpDesc') : undefined}
                errorMsg={modalErrorMsg || undefined}
            />
        </SessionContext.Provider>
    );
}
