// src/context/SessionContext.tsx
import { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { AuthModal } from '../components/ui/AuthModal';
import { AppSettings } from '../types';

interface SessionContextType {
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>;
    isSessionActive: boolean;
    notifyLogin: () => void;
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

    // Ref für aktuellen Status (um Stale Closures zu vermeiden)
    const isSessionActiveRef = useRef(false);
    // Ref für Throttling der Heartbeats (Drosselung auf alle 2s)
    const lastHeartbeatRef = useRef(0);

    // Sync State -> Ref
    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    const [pendingAction, setPendingAction] = useState<{
        resolve: (value: any) => void;
        reject: (reason: any) => void;
        action: (password: string | null) => Promise<any>;
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
                    invoke("refresh_session_activity").catch((e) => logger.warn(`Heartbeat failed: ${e}`));
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

    // Wird von App.tsx nach Login aufgerufen
    const notifyLogin = useCallback(() => {
        activateSession();
    }, [activateSession]);

    const protectAction = useCallback(async <T,>(action: (password: string | null) => Promise<T>): Promise<T | void> => {
        const executeWithSessionCheck = async (pwd: string | null): Promise<T> => {
            try {
                return await action(pwd);
            } catch (error: any) {
                const errMsg = String(error);
                // Prüfen auf typische Backend-Fehler bei abgelaufener Session
                if (errMsg.includes("Password required") || errMsg.includes("Session timed out")) {
                    logger.warn("Backend rejected session (expired). Resetting frontend session state.");
                    setIsSessionActive(false);
                    isSessionActiveRef.current = false;
                    throw new Error("SESSION_EXPIRED_RETRY");
                }
                throw error;
            }
        };

        try {
            const settings = await invoke<AppSettings>('get_app_settings');
            const timeout = settings.session_timeout_seconds;

            // MODUS A: Immer Passwort fragen (Timeout = 0)
            if (timeout === 0) {
                return new Promise<T>((resolve, reject) => {
                    setPendingAction({ resolve, reject, action });
                    setIsModalOpen(true);
                });
            }

            // MODUS B: Session nutzen
            if (isSessionActiveRef.current) {
                try {
                    return await executeWithSessionCheck(null);
                } catch (e: any) {
                    if (e.message === "SESSION_EXPIRED_RETRY") {
                        // Session war im Backend abgelaufen -> Modal öffnen
                        return new Promise<T>((resolve, reject) => {
                            setPendingAction({ resolve, reject, action });
                            setIsModalOpen(true);
                        });
                    }
                    throw e;
                }
            } else {
                // Session noch nicht aktiv -> Modal öffnen
                return new Promise<T>((resolve, reject) => {
                    setPendingAction({ resolve, reject, action });
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
            const settings = await invoke<AppSettings>('get_app_settings');
            const timeout = settings.session_timeout_seconds;

            if (timeout > 0) {
                // Session im Backend starten
                await invoke("unlock_session", { password, durationSeconds: timeout });

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

    return (
        <SessionContext.Provider value={{ protectAction, isSessionActive, notifyLogin }}>
            {children}
            <AuthModal
                isOpen={isModalOpen}
                onConfirm={handleModalConfirm}
                onCancel={handleModalCancel}
            />
        </SessionContext.Provider>
    );
}