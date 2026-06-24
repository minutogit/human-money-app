// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { authService } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { logger } from '../utils/log';
import { AuthModal } from '../components/ui/AuthModal';
// AppSettings import removed as unused

import { startSealSyncLoop, stopSealSyncLoop } from '../utils/sealSync';
import { useWindowTitleSync } from '../hooks/useWindowTitleSync';
import { useIntegrityCheck } from '../hooks/useIntegrityCheck';
import { useSessionHeartbeat } from '../hooks/useSessionHeartbeat';
import { isBackendError, stringifyError } from '../utils/errorHelper';

export interface SessionContextType {
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>;
    isSessionActive: boolean;
    profileName: string;
    setProfileName: (name: string) => void;
    notifyLogin: (name?: string) => void;
    notifyLogout: () => void;
    isForkLocked: boolean;
    isRecoveryRequired: boolean;
    integrityReport: import('../types').IntegrityReport | null;
    checkIntegrity: () => Promise<void>;
    clearLocks: () => void;
}

const SessionContext = createContext<SessionContextType | null>(null);

export function useSession() {
    const context = useContext(SessionContext);
    if (!context) throw new Error("useSession must be used within a SessionProvider");
    return context;
}

export function SessionProvider({ children }: { children: ReactNode }) {
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [isSessionActive, setIsSessionActive] = useState(false);
    const [profileName, setProfileName] = useState<string>("");
    const [isForkLocked, setIsForkLocked] = useState(false);
    const [isRecoveryRequired, setIsRecoveryRequired] = useState(false);

    // Refs für Throttling und Status
    const lastHeartbeatRef = useRef(0);
    const isSessionActiveRef = useRef(false);

    // Hooks für Side-Effects
    useWindowTitleSync(profileName);
    
    const onSessionExpired = useCallback(() => {
        setIsSessionActive(false);
        isSessionActiveRef.current = false;
        setProfileName("");
    }, []);

    useSessionHeartbeat(isSessionActive, onSessionExpired, lastHeartbeatRef);

    const { integrityReport, checkIntegrity } = useIntegrityCheck();

    // Sync State -> Ref (needed for protectAction which uses the ref)
    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    const [pendingAction, setPendingAction] = useState<{
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
    const notifyLogin = useCallback((name?: string) => {
        if (name) setProfileName(name);
        activateSession();
        startSealSyncLoop();
        checkIntegrity();
    }, [activateSession, checkIntegrity]);

    const notifyLogout = useCallback(() => {
        setIsSessionActive(false);
        isSessionActiveRef.current = false;
        setProfileName("");
        stopSealSyncLoop();
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
                if (errCode === 'error.security.walletLockedDueToFork'
                    || errMsg.includes("WalletLockedDueToFork") || errMsg.includes("SealForkDetected") || errMsg.includes("Security Lockdown")) {
                    logger.error("CRITICAL: Wallet is locked due to a fork.");
                    setIsForkLocked(true);
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
                    setPendingAction({ 
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
                            setPendingAction({ 
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
                    setPendingAction({ 
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
    }, []);

    const handleModalConfirm = async (password: string) => {
        if (!pendingAction) return;

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
        setIsModalOpen(false);
        setPendingAction(null);
    };

    const clearLocks = useCallback(() => {
        setIsForkLocked(false);
        setIsRecoveryRequired(false);
    }, []);

    return (
        <SessionContext.Provider value={{ protectAction, isSessionActive, profileName, setProfileName, notifyLogin, notifyLogout, isForkLocked, isRecoveryRequired, integrityReport, checkIntegrity, clearLocks }}>
            {children}
            <AuthModal
                isOpen={isModalOpen}
                onConfirm={handleModalConfirm}
                onCancel={handleModalCancel}
            />
        </SessionContext.Provider>
    );
}