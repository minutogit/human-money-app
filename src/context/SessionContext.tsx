// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { authService, SessionAccessMode, SessionStatusDto } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { marketService } from '../services/marketService';
import { logger } from '../utils/log';
import { MY_PROFILE_UPDATED_EVENT, notifyAppNoticesUpdated, notifySyncStatusUpdated } from '../utils/syncEvents';
import {
    FrontendAutoBackupStatusDto,
    FrontendBackupSyncResult,
    FrontendMarketSyncResult,
    FrontendMyMarketProfile,
    FrontendPostSyncResult,
} from '../types';
import { AuthModal } from '../components/ui/AuthModal';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';

import { dispatchBackupUpdated, updateNetworkStatus } from '../sync/syncBridge';
import { detectIsWifiConnection } from '../utils/networkStatus';
import { channelMembersSyncManager } from '../sync/channelMembersSyncManager';
import { rateLimitManager } from '../sync/rateLimitManager';
import { APP_EVENTS, emitAppEvent } from '../utils/appEvents';
import { useWindowTitleSync } from '../hooks/useWindowTitleSync';
import { useIntegrityCheck, CheckIntegrityOptions } from '../hooks/useIntegrityCheck';
import { useSessionHeartbeat } from '../hooks/useSessionHeartbeat';
import { isBackendError, stringifyError, translateError } from '../utils/errorHelper';
import { tourBridge } from '../utils/tourBridge';

export type SecurityLockdownKind = 'handover' | 'clone' | 'fork' | null;

export interface SecurityLockdownDetails {
    targetEpoch?: number;
    isSeedRecovery?: boolean;
}

export interface ProtectActionOptions {
    requireFullAccess?: boolean;
}

export interface SessionContextType {
    protectAction: <T>(
        action: (password: string | null) => Promise<T>,
        options?: ProtectActionOptions,
    ) => Promise<T | void>;
    requireFullAccess: <T>(action: (masterPassword: string) => Promise<T>) => Promise<T | void>;
    elevateSession: (masterPassword: string) => Promise<void>;
    accessMode: SessionAccessMode;
    baseAccessMode: SessionAccessMode;
    hasCachedSession: boolean;
    isAuthenticated: boolean;
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
    checkIntegrity: (options?: CheckIntegrityOptions) => Promise<void>;
    clearLocks: () => void;
    lockSession: () => Promise<void>;
}

interface PendingModalRequest {
    id: number;
    mode: 'session' | 'stepUp';
    credentialKind: 'password' | 'pin';
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
    const [baseAccessMode, setBaseAccessMode] = useState<SessionAccessMode>('full');
    const [hasCachedSession, setHasCachedSession] = useState(false);
    const [isAuthenticated, setIsAuthenticated] = useState(false);
    const [profileName, setProfileName] = useState<string>("");
    const [displayProfileName, setDisplayProfileName] = useState<string>("");
    const [userAvatarUrl, setUserAvatarUrl] = useState<string | null>(null);
    const [folderName, setFolderNameState] = useState<string>("");
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
    const accessModeRef = useRef<SessionAccessMode>('full');
    const baseAccessModeRef = useRef<SessionAccessMode>('full');
    const hasCachedSessionRef = useRef(false);
    const isAuthenticatedRef = useRef(false);
    const folderNameRef = useRef<string>("");
    const pendingQueueRef = useRef<PendingModalRequest[]>([]);
    const nextPendingIdRef = useRef(1);
    const isModalSubmittingRef = useRef(false);
    const modalAttemptGenRef = useRef(0);
    const [isModalSubmitting, setIsModalSubmitting] = useState(false);

    const setFolderName = useCallback((folder: string) => {
        folderNameRef.current = folder;
        setFolderNameState(folder);
    }, []);

    const syncNetworkStatusNow = useCallback(() => {
        if (typeof navigator === 'undefined') return;
        const isOnline = navigator.onLine !== false;
        const isWifi = detectIsWifiConnection();
        const isVisible =
            typeof document === 'undefined' ? true : document.visibilityState !== 'hidden';
        void updateNetworkStatus(isOnline, isWifi, isVisible);
    }, []);

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

    // Side-effect hooks
    useWindowTitleSync(profileName);

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

    useSessionHeartbeat(hasCachedSession, onStatusSync, lastHeartbeatRef);

    const { integrityReport, checkIntegrity } = useIntegrityCheck();

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

    useEffect(() => {
        return () => {
            modalAttemptGenRef.current += 1;
            isModalSubmittingRef.current = false;

            if (pendingQueueRef.current.length > 0) {
                const remaining = pendingQueueRef.current;
                pendingQueueRef.current = [];
                remaining.forEach((entry) => entry.reject(new Error("SessionProvider unmounted")));
            }
        };
    }, []);

    // Lightweight network & visibility bridge feeding the backend BackgroundSyncCoordinator.
    useEffect(() => {
        if (typeof window === 'undefined') return;
        syncNetworkStatusNow();
        const handleStateChange = () => syncNetworkStatusNow();
        window.addEventListener('online', handleStateChange);
        window.addEventListener('offline', handleStateChange);
        if (typeof document !== 'undefined') {
            document.addEventListener('visibilitychange', handleStateChange);
        }
        const navConn = (navigator as Navigator & {
            connection?: {
                addEventListener?: (t: string, cb: () => void) => void;
                removeEventListener?: (t: string, cb: () => void) => void;
            };
        }).connection;
        navConn?.addEventListener?.('change', handleStateChange);

        return () => {
            window.removeEventListener('online', handleStateChange);
            window.removeEventListener('offline', handleStateChange);
            if (typeof document !== 'undefined') {
                document.removeEventListener('visibilitychange', handleStateChange);
            }
            navConn?.removeEventListener?.('change', handleStateChange);
        };
    }, [syncNetworkStatusNow]);

    // Subscribe to backend coordinator events so UI components refresh immediately.
    useEffect(() => {
        if (typeof window === 'undefined') return;
        let cancelled = false;
        const unlisteners: Array<() => void> = [];

        const setup = async () => {
            const register = async <T,>(
                event: string,
                handler: (payload: T) => void,
            ): Promise<void> => {
                try {
                    const unlisten = await Promise.resolve(
                        listen<T>(event, (ev) => {
                            if (!cancelled) {
                                handler(ev.payload);
                            }
                        }),
                    );
                    if (typeof unlisten === 'function') {
                        if (cancelled) {
                            unlisten();
                        } else {
                            unlisteners.push(unlisten);
                        }
                    }
                } catch {
                    // Ignore in non-Tauri / unit-test environments without event bridge
                }
            };

            await Promise.all([
                register<FrontendAutoBackupStatusDto | null>(
                    'humoco://backup-updated',
                    (status) => {
                        dispatchBackupUpdated(status ?? undefined);
                    },
                ),
                register<FrontendBackupSyncResult | null>(
                    'humoco://sync-status-updated',
                    (res) => {
                        notifySyncStatusUpdated();
                        if (res) {
                            const retrySecs = rateLimitManager.extractRetryAfter(res);
                            if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        }
                    },
                ),
                register<FrontendPostSyncResult | null>(
                    'humoco://post-synced',
                    (res) => {
                        if (!res) return;
                        const retrySecs = rateLimitManager.extractRetryAfter(res);
                        if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        const hasInboxChanges =
                            res.uploadedTo > 0 ||
                            res.importedPayments > 0 ||
                            res.importedReceipts > 0 ||
                            res.importedSignatures > 0 ||
                            res.newSignRequests > 0 ||
                            res.newWotHandshakes > 0 ||
                            res.newInquiries > 0 ||
                            res.newInquiryMessages > 0 ||
                            res.prunedBoxes > 0;
                        if (hasInboxChanges) {
                            emitAppEvent(APP_EVENTS.POST_INBOX_UPDATED);
                            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'post' });
                        }
                        if (res.newWotHandshakes > 0) {
                            emitAppEvent(APP_EVENTS.WOT_UPDATED);
                        }
                        if (hasInboxChanges || res.polledSyncs > 0) {
                            notifyAppNoticesUpdated();
                        }
                    },
                ),
                register<FrontendMarketSyncResult | null>(
                    'humoco://market-synced',
                    (res) => {
                        notifyAppNoticesUpdated();
                        if (!res) return;
                        const retrySecs = rateLimitManager.extractRetryAfter(res);
                        if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        if (res.importedCards > 0 || (res.newAds ?? 0) > 0) {
                            emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
                        }
                        if (res.importedCards > 0) {
                            emitAppEvent(APP_EVENTS.CONTACTS_UPDATED);
                        }
                        if (res.importedWotLists > 0) {
                            emitAppEvent(APP_EVENTS.WOT_UPDATED);
                        }
                        if (
                            res.uploadedTo > 0 ||
                            res.polledSyncs > 0 ||
                            res.importedCards > 0 ||
                            res.importedWotLists > 0
                        ) {
                            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'market' });
                        }
                    },
                ),
                register<unknown>('humoco://conflicts-updated', () => {
                    emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
                }),
            ]);
        };

        void setup();
        return () => {
            cancelled = true;
            for (const unlisten of unlisteners) {
                unlisten();
            }
        };
    }, []);

    const enqueuePendingRequest = useCallback(
        <T,>(
            mode: 'session' | 'stepUp',
            credentialKind: 'password' | 'pin',
            action: (password: string | null) => Promise<T>,
        ): Promise<T> => {
            return new Promise<T>((resolve, reject) => {
                const entry: PendingModalRequest = {
                    id: nextPendingIdRef.current++,
                    mode,
                    credentialKind,
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

    const switchActivePendingToPassword = useCallback((errorText?: string) => {
        modalAttemptGenRef.current += 1;
        isModalSubmittingRef.current = false;
        setIsModalSubmitting(false);
        const current = pendingQueueRef.current[0];
        if (!current) return;
        current.credentialKind = 'password';
        setModalErrorMsg(errorText ?? '');
        setActivePending({ ...current });
    }, []);

    const notifyLogin = useCallback((name?: string, folder?: string) => {
        channelMembersSyncManager.reset();
        setIsAuthenticated(true);
        isAuthenticatedRef.current = true;
        accessModeRef.current = 'full';
        baseAccessModeRef.current = 'full';
        setAccessMode('full');
        setBaseAccessMode('full');
        if (name) setProfileName(name);
        if (folder) {
            folderNameRef.current = folder;
            setFolderNameState(folder);
        }

        authService
            .getSessionStatus()
            .then((status) => {
                if (!isAuthenticatedRef.current) return;
                applySessionStatus(status);
                void checkIntegrity({
                    allowSilentRepair: status.accessMode === 'full' && status.hasCachedSession,
                });
            })
            .catch(() => {
                if (!isAuthenticatedRef.current) return;
                hasCachedSessionRef.current = false;
                setHasCachedSession(false);
                void checkIntegrity({ allowSilentRepair: false });
            });
    }, [applySessionStatus, checkIntegrity]);

    const notifyLogout = useCallback(() => {
        setIsAuthenticated(false);
        isAuthenticatedRef.current = false;
        setHasCachedSession(false);
        hasCachedSessionRef.current = false;
        accessModeRef.current = 'full';
        baseAccessModeRef.current = 'full';
        setAccessMode('full');
        setBaseAccessMode('full');
        setProfileName("");
        setDisplayProfileName("");
        setUserAvatarUrl(null);
        folderNameRef.current = "";
        setFolderNameState("");
        channelMembersSyncManager.reset();
        rateLimitManager.clearCooldown();
        emitAppEvent(APP_EVENTS.SESSION_LOCKED);

        modalAttemptGenRef.current += 1;
        isModalSubmittingRef.current = false;
        setIsModalSubmitting(false);

        if (pendingQueueRef.current.length > 0) {
            const remaining = pendingQueueRef.current;
            pendingQueueRef.current = [];
            setActivePending(null);
            setModalErrorMsg('');
            remaining.forEach((entry) => entry.reject(new Error("User logged out")));
        }
    }, []);

    const elevateSession = useCallback(async (masterPassword: string) => {
        const status = await authService.elevateSessionWithPassword(masterPassword);
        applySessionStatus(status);
    }, [applySessionStatus]);

    const requireFullAccess = useCallback(async <T,>(action: (masterPassword: string) => Promise<T>): Promise<T | void> => {
        return enqueuePendingRequest<T>('stepUp', 'password', (pwd) => action(pwd || ''));
    }, [enqueuePendingRequest]);

    const protectAction = useCallback(async <T,>(
        action: (password: string | null) => Promise<T>,
        options?: ProtectActionOptions,
    ): Promise<T | void> => {
        const executeWithSessionCheck = async (pwd: string | null): Promise<T> => {
            try {
                return await action(pwd);
            } catch (error) {
                const errMsg = stringifyError(error);
                const errCode = isBackendError(error) ? error.code : '';

                if (errCode === 'error.wallet.locked'
                    || errCode === 'error.auth.sessionTimeout'
                    || errCode === 'error.auth.sessionNotActive'
                    || errCode === 'error.auth.passwordRequired'
                    || errMsg.includes("Password required")
                    || errMsg.includes("Session timed out")
                    || errMsg.includes("Wallet is locked")) {
                    logger.warn("Backend rejected action (wallet locked/session expired). Resetting frontend session state.");
                    hasCachedSessionRef.current = false;
                    accessModeRef.current = baseAccessModeRef.current;
                    setHasCachedSession(false);
                    setAccessMode(baseAccessModeRef.current);
                    emitAppEvent(APP_EVENTS.SESSION_LOCKED);
                    throw new Error("SESSION_EXPIRED_RETRY");
                }
                if (errCode === 'error.auth.insufficientAccessMode' || errMsg.includes("InsufficientAccessMode")) {
                    logger.info("Backend requires elevated session access mode; prompting step-up auth.");
                    throw new Error("STEP_UP_REQUIRED_RETRY");
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
            const config = await settingsService.getConfig();
            const timeout = config.securityPolicy.sessionTimeoutSeconds;
            const needsStepUp = Boolean(options?.requireFullAccess && accessModeRef.current !== 'full');
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
                    return await executeWithSessionCheck(null);
                } catch (e) {
                    if (e instanceof Error && e.message === "SESSION_EXPIRED_RETRY") {
                        return enqueuePendingRequest<T>(
                            'session',
                            baseAccessModeRef.current === 'dailyPay' ? 'pin' : 'password',
                            action,
                        );
                    }
                    if (e instanceof Error && e.message === "STEP_UP_REQUIRED_RETRY") {
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
    }, [enqueuePendingRequest, triggerSecurityLockdown]);

    const runPendingEntry = async (entry: PendingModalRequest, password: string | null) => {
        try {
            const result = await entry.action(password);
            entry.resolve(result);
        } catch (actionErr) {
            const msg = stringifyError(actionErr);
            const code = isBackendError(actionErr) ? actionErr.code : '';
            const isInsufficient =
                code === 'error.auth.insufficientAccessMode' ||
                msg.includes('InsufficientAccessMode');
            if (entry.mode === 'session' && isInsufficient) {
                enqueuePendingRequest('stepUp', 'password', entry.action)
                    .then(entry.resolve, entry.reject);
                return;
            }
            entry.reject(actionErr);
        }
    };

    const handleModalConfirm = async (credential: string) => {
        const currentPending = pendingQueueRef.current[0];
        if (!currentPending || isModalSubmittingRef.current) return;
        const gen = ++modalAttemptGenRef.current;
        isModalSubmittingRef.current = true;
        setIsModalSubmitting(true);

        const isAttemptCurrent = () =>
            modalAttemptGenRef.current === gen && pendingQueueRef.current[0]?.id === currentPending.id;

        try {
            if (currentPending.mode === 'stepUp') {
                try {
                    const status = await authService.elevateSessionWithPassword(credential);
                    if (!isAttemptCurrent()) return;
                    applySessionStatus(status);
                } catch (err) {
                    if (!isAttemptCurrent()) return;
                    setModalErrorMsg(translateError(err, t));
                    return;
                }

                let timeout = 0;
                try {
                    const config = await settingsService.getConfig();
                    if (!isAttemptCurrent()) return;
                    timeout = config.securityPolicy.sessionTimeoutSeconds;
                } catch {
                    if (!isAttemptCurrent()) return;
                    timeout = 0;
                }

                setModalErrorMsg('');
                pendingQueueRef.current.shift();

                const toRun: Array<{ entry: PendingModalRequest; pwd: string | null }> = [
                    { entry: currentPending, pwd: credential },
                ];

                if (timeout > 0) {
                    while (pendingQueueRef.current.length > 0) {
                        const next = pendingQueueRef.current[0];
                        if (next.mode === 'stepUp') {
                            pendingQueueRef.current.shift();
                            toRun.push({ entry: next, pwd: credential });
                        } else if (next.mode === 'session' && hasCachedSessionRef.current) {
                            pendingQueueRef.current.shift();
                            toRun.push({ entry: next, pwd: null });
                        } else {
                            break;
                        }
                    }
                }

                setActivePending(pendingQueueRef.current[0] ?? null);
                isModalSubmittingRef.current = false;
                setIsModalSubmitting(false);
                await Promise.all(toRun.map((item) => runPendingEntry(item.entry, item.pwd)));
                return;
            }

            let timeout = 0;
            try {
                const config = await settingsService.getConfig();
                if (!isAttemptCurrent()) return;
                timeout = config.securityPolicy.sessionTimeoutSeconds;
            } catch (err) {
                if (!isAttemptCurrent()) return;
                setModalErrorMsg(translateError(err, t));
                return;
            }

            const folderArg = folderNameRef.current || undefined;

            if (timeout > 0) {
                if (currentPending.credentialKind === 'pin') {
                    try {
                        const status = await authService.unlockSessionWithPin(
                            credential,
                            folderArg,
                        );
                        if (!isAttemptCurrent()) return;
                        applySessionStatus(status);
                        setModalErrorMsg('');
                    } catch (err) {
                        if (!isAttemptCurrent()) return;
                        const code = isBackendError(err) ? err.code : '';
                        const msg = stringifyError(err);
                        const isPinWiped =
                            code === 'error.auth.pinWipedMaxAttempts' ||
                            msg.includes('pinWipedMaxAttempts');
                        if (isPinWiped) {
                            switchActivePendingToPassword(translateError(err, t));
                        } else {
                            setModalErrorMsg(translateError(err, t));
                        }
                        return;
                    }
                } else {
                    try {
                        if (baseAccessModeRef.current !== 'full') {
                            const status = await authService.elevateSessionWithPassword(credential);
                            if (!isAttemptCurrent()) return;
                            applySessionStatus(status);
                        } else {
                            const status = await authService.unlockSession(credential);
                            if (!isAttemptCurrent()) return;
                            if ((status as unknown) === false) {
                                throw new Error('Invalid password');
                            }
                            applySessionStatus(status);
                        }
                        setModalErrorMsg('');
                        logger.info(`Session unlocked via modal for ${timeout} seconds.`);
                    } catch (err) {
                        if (!isAttemptCurrent()) return;
                        const msg = stringifyError(err);
                        const code = isBackendError(err) ? err.code : '';
                        const isInsufficient =
                            code === 'error.auth.insufficientAccessMode' ||
                            msg.includes('InsufficientAccessMode');
                        if (isInsufficient && accessModeRef.current !== 'full') {
                            try {
                                const status = await authService.elevateSessionWithPassword(credential);
                                if (!isAttemptCurrent()) return;
                                applySessionStatus(status);
                                setModalErrorMsg('');
                            } catch (elevErr) {
                                if (!isAttemptCurrent()) return;
                                setModalErrorMsg(translateError(elevErr, t));
                                return;
                            }
                        } else {
                            setModalErrorMsg(translateError(err, t));
                            return;
                        }
                    }
                }

                if (!isAttemptCurrent()) return;
                pendingQueueRef.current.shift();
                const toRun: PendingModalRequest[] = [currentPending];
                while (pendingQueueRef.current.length > 0 && pendingQueueRef.current[0].mode === 'session') {
                    toRun.push(pendingQueueRef.current.shift()!);
                }

                setActivePending(pendingQueueRef.current[0] ?? null);
                isModalSubmittingRef.current = false;
                setIsModalSubmitting(false);
                await Promise.all(toRun.map((entry) => runPendingEntry(entry, null)));
            } else {
                // Mode A (timeout === 0): prompt per action; PIN uses a single-use backend grant token
                if (currentPending.credentialKind === 'pin') {
                    let status: Awaited<ReturnType<typeof authService.unlockSessionWithPin>>;
                    try {
                        status = await authService.unlockSessionWithPin(credential, folderArg);
                        if (!isAttemptCurrent()) return;
                    } catch (err) {
                        if (!isAttemptCurrent()) return;
                        const code = isBackendError(err) ? err.code : '';
                        const msg = stringifyError(err);
                        const isPinWiped =
                            code === 'error.auth.pinWipedMaxAttempts' ||
                            msg.includes('pinWipedMaxAttempts');
                        if (isPinWiped) {
                            switchActivePendingToPassword(translateError(err, t));
                        } else {
                            setModalErrorMsg(translateError(err, t));
                        }
                        return;
                    }

                    setModalErrorMsg('');
                    pendingQueueRef.current.shift();
                    setActivePending(pendingQueueRef.current[0] ?? null);
                    isModalSubmittingRef.current = false;
                    setIsModalSubmitting(false);
                    await runPendingEntry(currentPending, status.oneShotGrant ?? null);
                } else {
                    if (baseAccessModeRef.current !== 'full') {
                        try {
                            const status = await authService.elevateSessionWithPassword(credential);
                            if (!isAttemptCurrent()) return;
                            applySessionStatus(status);
                        } catch (err) {
                            if (!isAttemptCurrent()) return;
                            setModalErrorMsg(translateError(err, t));
                            return;
                        }
                    } else {
                        try {
                            const status = await authService.unlockSession(credential);
                            if (!isAttemptCurrent()) return;
                            if ((status as unknown) === false) {
                                throw new Error('Invalid password');
                            }
                            applySessionStatus(status);
                        } catch (err) {
                            if (!isAttemptCurrent()) return;
                            setModalErrorMsg(translateError(err, t));
                            return;
                        }
                    }
                    if (!isAttemptCurrent()) return;
                    setModalErrorMsg('');
                    pendingQueueRef.current.shift();
                    setActivePending(pendingQueueRef.current[0] ?? null);
                    isModalSubmittingRef.current = false;
                    setIsModalSubmitting(false);
                    await runPendingEntry(currentPending, credential);
                }
            }
        } finally {
            if (modalAttemptGenRef.current === gen) {
                isModalSubmittingRef.current = false;
                setIsModalSubmitting(false);
            }
        }
    };

    const handleModalCancel = () => {
        modalAttemptGenRef.current += 1;
        const cancelled = pendingQueueRef.current;
        pendingQueueRef.current = [];
        isModalSubmittingRef.current = false;
        setIsModalSubmitting(false);
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
            const fallbackBase = baseAccessModeRef.current;
            hasCachedSessionRef.current = false;
            accessModeRef.current = fallbackBase;
            setHasCachedSession(false);
            setAccessMode(fallbackBase);
            emitAppEvent(APP_EVENTS.SESSION_LOCKED);
        }
    }, []);

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
            if (!isAuthenticated) return;
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
    }, [isAuthenticated]);

    useEffect(() => {
        return tourBridge.subscribe((partial) => {
            if (partial.isAuthenticated !== undefined) {
                setIsAuthenticated(partial.isAuthenticated);
                isAuthenticatedRef.current = partial.isAuthenticated;
            }
            if (partial.hasCachedSession !== undefined) {
                setHasCachedSession(partial.hasCachedSession);
                hasCachedSessionRef.current = partial.hasCachedSession;
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
