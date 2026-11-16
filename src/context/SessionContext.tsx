// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { authService, SessionAccessMode } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { logger } from '../utils/log';
import { AuthModal } from '../components/ui/AuthModal';
import { useTranslation } from 'react-i18next';
// AppSettings import removed as unused

import { startSealSyncLoop, stopSealSyncLoop } from '../utils/sealSync';
import { startAutoBackupLoop, stopAutoBackupLoop } from '../utils/autoBackup';
import { startPostSyncLoop, stopPostSyncLoop } from '../utils/postSync';
import { startMarketSyncLoop, stopMarketSyncLoop } from '../utils/marketSync';
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
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [modalErrorMsg, setModalErrorMsg] = useState<string>('');
    const [accessMode, setAccessMode] = useState<SessionAccessMode>('full');
    const [isSessionActive, setIsSessionActive] = useState(false);
    const [isAuthenticated, setIsAuthenticated] = useState(false);
    const [profileName, setProfileName] = useState<string>("");
    const [folderName, setFolderName] = useState<string>("");
    const [isForkLocked, setIsForkLocked] = useState(false);
    const [lockdownKind, setLockdownKind] = useState<SecurityLockdownKind>(null);
    const [lockdownDetails, setLockdownDetails] = useState<SecurityLockdownDetails | null>(null);
    const [isRecoveryRequired, setIsRecoveryRequired] = useState(false);

    const triggerSecurityLockdown = useCallback((kind: SecurityLockdownKind, details?: SecurityLockdownDetails) => {
        setLockdownKind(kind);
        setLockdownDetails(details || null);
        if (kind !== null) {
            setIsForkLocked(true);
        } else {
            setIsForkLocked(false);
        }
    }, []);

    // Refs für Throttling und Status
    const lastHeartbeatRef = useRef(0);
    const isSessionActiveRef = useRef(false);

    // Hooks für Side-Effects
    useWindowTitleSync(profileName);
    
    const onSessionExpired = useCallback(() => {
        setIsSessionActive(false);
        isSessionActiveRef.current = false;
        // DO NOT clear profileName and DO NOT set isAuthenticated to false
    }, []);

    useSessionHeartbeat(isSessionActive, onSessionExpired, lastHeartbeatRef);

    const { integrityReport, checkIntegrity } = useIntegrityCheck();

    // Sync State -> Ref (needed for protectAction which uses the ref)
    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    const [pendingAction, setPendingAction] = useState<{
        mode: 'session' | 'stepUp';
        resolve: (value: unknown) => void;
        reject: (reason: unknown) => void;
        action: (password: string | null) => Promise<unknown>;
    } | null>(null);

    // Hilfsfunktion zum Aktivieren der Session
    const activateSession = useCallback(() => {
        setIsSessionActive(true);
        isSessionActiveRef.current = true;
        lastHeartbeatRef.current = Date.now();
    }, []);

    // Wird von App.tsx nach Login aufgerufen
    const notifyLogin = useCallback((name?: string, folder?: string) => {
        setIsAuthenticated(true);
        if (name) setProfileName(name);
        if (folder) setFolderName(folder);
        activateSession();
        startSealSyncLoop();
        startAutoBackupLoop();
        startPostSyncLoop();
        startMarketSyncLoop();
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
                } else {
                    setIsSessionActive(false);
                    isSessionActiveRef.current = false;
                }
            })
            .catch(() => {
                setIsSessionActive(false);
                isSessionActiveRef.current = false;
            });
    }, [activateSession, checkIntegrity]);

    const notifyLogout = useCallback(() => {
        setIsAuthenticated(false);
        setIsSessionActive(false);
        setAccessMode('full');
        isSessionActiveRef.current = false;
        setProfileName("");
        setFolderName("");
        stopSealSyncLoop();
        stopAutoBackupLoop();
        stopPostSyncLoop();
        stopMarketSyncLoop();
    }, []);

    const elevateSession = useCallback(async (masterPassword: string) => {
        const nextMode = await authService.elevateSessionWithPassword(masterPassword);
        setAccessMode(nextMode);
        activateSession();
    }, [activateSession]);

    const requireFullAccess = useCallback(async <T,>(action: (masterPassword: string) => Promise<T>): Promise<T | void> => {
        return new Promise<T>((resolve, reject) => {
            setModalErrorMsg('');
            setPendingAction({
                mode: 'stepUp',
                resolve: resolve as (value: unknown) => void,
                reject,
                action: (pwd) => action(pwd || ''),
            });
            setIsModalOpen(true);
        });
    }, []);

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

            // MODUS A: Immer Passwort fragen (Timeout = 0)
            if (timeout === 0) {
                return new Promise<T>((resolve, reject) => {
                    setModalErrorMsg('');
                    setPendingAction({ 
                        mode: 'session',
                        resolve: resolve as (value: unknown) => void, 
                        reject, 
                        action: action as (password: string | null) => Promise<unknown> 
                    });
                    setIsModalOpen(true);
                });
            }

            // MODUS B: Session nutzen
            if (isSessionActiveRef.current) {
                try {
                    return await executeWithSessionCheck(null);
                } catch (e) {
                    if (e instanceof Error && e.message === "SESSION_EXPIRED_RETRY") {
                        // Session war im Backend abgelaufen -> Modal öffnen
                        return new Promise<T>((resolve, reject) => {
                            setModalErrorMsg('');
                            setPendingAction({ 
                                mode: 'session',
                                resolve: resolve as (value: unknown) => void, 
                                reject, 
                                action: action as (password: string | null) => Promise<unknown> 
                            });
                            setIsModalOpen(true);
                        });
                    }
                    throw e;
                }
            } else {
                // Session noch nicht aktiv -> Modal öffnen
                return new Promise<T>((resolve, reject) => {
                    setModalErrorMsg('');
                    setPendingAction({ 
                        mode: 'session',
                        resolve: resolve as (value: unknown) => void, 
                        reject, 
                        action: action as (password: string | null) => Promise<unknown> 
                    });
                    setIsModalOpen(true);
                });
            }
        } catch (e) {
            logger.error(`Error in protectAction: ${stringifyError(e)}`);
            throw e;
        }
    }, [triggerSecurityLockdown]);

    const handleModalConfirm = async (password: string) => {
        if (!pendingAction) return;

        if (pendingAction.mode === 'stepUp') {
            try {
                await elevateSession(password);
                setModalErrorMsg('');
                const currentPending = pendingAction;
                setIsModalOpen(false);
                setPendingAction(null);
                try {
                    const result = await currentPending.action(password);
                    currentPending.resolve(result);
                } catch (actionErr) {
                    currentPending.reject(actionErr);
                }
            } catch (err) {
                setModalErrorMsg(translateError(err, t));
            }
            return;
        }

        try {
            const settings = await settingsService.getSettings();
            const timeout = settings.sessionTimeoutSeconds;

            if (timeout > 0) {
                // Session im Backend starten
                await authService.unlockSession(password, timeout);

                // Frontend Status synchronisieren (via Helper)
                activateSession();

                logger.info(`Session unlocked via modal for ${timeout} seconds.`);

                const result = await pendingAction.action(null);
                pendingAction.resolve(result);
            } else {
                // Modus A: Passwort direkt verwenden
                const result = await pendingAction.action(password);
                pendingAction.resolve(result);
            }
        } catch (e) {
            pendingAction.reject(e);
        } finally {
            setIsModalOpen(false);
            setPendingAction(null);
        }
    };

    const handleModalCancel = () => {
        if (pendingAction) {
            pendingAction.reject(new Error("User cancelled authentication"));
        }
        setModalErrorMsg('');
        setIsModalOpen(false);
        setPendingAction(null);
    };

    const lockSession = useCallback(async () => {
        try {
            await authService.lockSession();
        } catch (e) {
            logger.warn(`Failed to lock session in backend: ${stringifyError(e)}`);
        } finally {
            setIsSessionActive(false);
            isSessionActiveRef.current = false;
        }
    }, []);

    const clearLocks = useCallback(() => {
        setIsForkLocked(false);
        setIsRecoveryRequired(false);
        setLockdownKind(null);
        setLockdownDetails(null);
    }, []);

    useEffect(() => {
        return tourBridge.subscribe((partial) => {
            if (partial.isAuthenticated !== undefined) {
                setIsAuthenticated(partial.isAuthenticated);
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

    return (
        <SessionContext.Provider value={{ protectAction, requireFullAccess, elevateSession, accessMode, isSessionActive, isAuthenticated, profileName, folderName, setProfileName, setFolderName, notifyLogin, notifyLogout, isForkLocked, lockdownKind, lockdownDetails, triggerSecurityLockdown, isRecoveryRequired, integrityReport, checkIntegrity, clearLocks, lockSession }}>
            {children}
            <AuthModal
                isOpen={isModalOpen}
                onConfirm={handleModalConfirm}
                onCancel={handleModalCancel}
                title={pendingAction?.mode === 'stepUp' ? t('auth.stepUpTitle') : undefined}
                description={pendingAction?.mode === 'stepUp' ? t('auth.stepUpDesc') : undefined}
                errorMsg={modalErrorMsg || undefined}
            />
        </SessionContext.Provider>
    );
}