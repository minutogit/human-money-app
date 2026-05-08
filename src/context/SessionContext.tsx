// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { authService } from '../services/authService';
import { settingsService } from '../services/settingsService';
import { integrityService } from '../services/integrityService';
import { logger } from '../utils/log';
import { AuthModal } from '../components/ui/AuthModal';
// AppSettings import removed as unused

import { startSealSyncLoop, stopSealSyncLoop } from '../utils/sealSync';

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
    const [integrityReport, setIntegrityReport] = useState<import('../types').IntegrityReport | null>(null);

    // Ref für aktuellen Status (um Stale Closures zu vermeiden)
    const isSessionActiveRef = useRef(false);
    // Ref für Throttling der Heartbeats (Drosselung auf alle 2s)
    const lastHeartbeatRef = useRef(0);

    // Sync State -> Ref
    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    // Update window title when profile name changes
    useEffect(() => {
        const updateTitle = async () => {
            try {
                const { getCurrentWindow } = await import("@tauri-apps/api/window");
                const win = getCurrentWindow();
                if (profileName) {
                    await win.setTitle(`Human Money App - ${profileName}`);
                } else {
                    await win.setTitle('Human Money App');
                }
            } catch (e) {
                logger.warn(`Failed to update window title: ${e}`);
            }
        };

        updateTitle();
    }, [profileName]);

    const [pendingAction, setPendingAction] = useState<{
        resolve: (value: unknown) => void;
        reject: (reason: unknown) => void;
        action: (password: string | null) => Promise<unknown>;
    } | null>(null);

    // Globaler Activity Listener
    useEffect(() => {
        const handleActivity = () => {
            // Nur wenn Session aktiv ist
            if (isSessionActiveRef.current) {
                const now = Date.now();
                // Throttling: Nur senden, wenn seit dem letzten Mal > 2000ms vergangen sind
                if (now - lastHeartbeatRef.current > 2000) {
                    lastHeartbeatRef.current = now;
                    // Fire and forget refresh
                    authService.refreshSessionActivity().catch((e) => logger.warn(`Heartbeat failed: ${e}`));
                }
            }
        };

        window.addEventListener('mousemove', handleActivity);
        window.addEventListener('keydown', handleActivity);
        window.addEventListener('click', handleActivity);

        return () => {
            window.removeEventListener('mousemove', handleActivity);
            window.removeEventListener('keydown', handleActivity);
            window.removeEventListener('click', handleActivity);
        };
    }, []); // Empty dependency array -> Event Listeners only attached once

    // Polling für Session-Status (erkennt Timeout im Hintergrund)
    useEffect(() => {
        const checkStatus = async () => {
            // Nur prüfen, wenn wir glauben, die Session sei aktiv
            if (isSessionActiveRef.current) {
                try {
                    const active = await authService.isSessionActive();
                    if (!active) {
                        logger.info("Session background check: Session expired.");
                        setIsSessionActive(false);
                        isSessionActiveRef.current = false;
                        setProfileName("");
                    }
                } catch (e) {
                    logger.error(`Session check failed: ${e}`);
                }
            }
        };

        const interval = setInterval(checkStatus, 10000); // Alle 10s prüfen
        return () => clearInterval(interval);
    }, []);

    // Hilfsfunktion zum Aktivieren der Session
    // Setzt Status auf True und resettet den Heartbeat-Timer
    const activateSession = useCallback(() => {
        setIsSessionActive(true);
        isSessionActiveRef.current = true;
        // WICHTIG: Wir setzen den Heartbeat-Timer auf "Jetzt",
        // damit nicht sofort beim ersten Mauswackler ein Request rausgeht.
        // Das gibt dem Backend Zeit und verhindert Race Conditions.
        lastHeartbeatRef.current = Date.now();
    }, []);

    const checkIntegrity = useCallback(async () => {
        try {
            const report = await integrityService.checkIntegrity();
            setIntegrityReport(report);
            if (report.type !== 'valid') {
                logger.warn(`Integrity issue detected: ${report.type}`);
            }
        } catch (e) {
            logger.error(`Failed to check integrity: ${e}`);
        }
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
                const errMsg = String(error);
                // Prüfen auf typische Backend-Fehler bei abgelaufener Session
                if (errMsg.includes("Password required") || errMsg.includes("Session timed out") || errMsg.includes("Wallet is locked")) {
                    logger.warn("Backend rejected action (wallet locked/session expired). Resetting frontend session state.");
                    setIsSessionActive(false);
                    isSessionActiveRef.current = false;
                    throw new Error("SESSION_EXPIRED_RETRY");
                }
                if (errMsg.includes("WalletLockedDueToFork") || errMsg.includes("SealForkDetected") || errMsg.includes("Security Lockdown")) {
                    logger.error("CRITICAL: Wallet is locked due to a fork.");
                    setIsForkLocked(true);
                }
                if (errMsg.includes("RequiresSealRecovery") || errMsg.includes("No local security seal found")) {
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
            logger.error(`Error in protectAction: ${e}`);
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