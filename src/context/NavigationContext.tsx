// src/context/NavigationContext.tsx
import { createContext, useContext, useState, ReactNode, useEffect, useCallback, useRef } from 'react';
import { AppState } from '../types';
import { getVersion } from "@tauri-apps/api/app";
import { profileService } from "../services/profileService";
import { authService } from "../services/authService";
import { logger } from "../utils/log";
import { useSession } from './SessionContext';
import { error } from "@tauri-apps/plugin-log";
import { stringifyError } from '../utils/errorHelper';

export interface NavigationContextType {
    appState: AppState;
    navigate: (newState: AppState) => void;
    goBack: () => void;
    isSidebarOpen: boolean;
    setSidebarOpen: (open: boolean) => void;
    appVersion: string;
    checkProfile: () => Promise<void>;
}

const NavigationContext = createContext<NavigationContextType | null>(null);

export function useNavigation() {
    const context = useContext(NavigationContext);
    if (!context) throw new Error("useNavigation must be used within a NavigationProvider");
    return context;
}

export function useOptionalNavigation() {
    return useContext(NavigationContext);
}

export function NavigationProvider({ children }: { children: ReactNode }) {
    const [appState, setAppState] = useState<AppState>({ view: "loading" });
    const [isSidebarOpen, setSidebarOpen] = useState(false);
    const [appVersion, setAppVersion] = useState<string>("");
    const { notifyLogin } = useSession();

    // Store state in ref for checkProfile to avoid stale closures
    const notifyLoginRef = useRef(notifyLogin);
    useEffect(() => {
        notifyLoginRef.current = notifyLogin;
    }, [notifyLogin]);

    const historyRef = useRef<AppState[]>([]);
    const appStateRef = useRef<AppState>(appState);
    appStateRef.current = appState;

    const navigate = useCallback((newState: AppState) => {
        const current = appStateRef.current;
        if (newState.view === 'logged_in' || newState.view === 'needs_login' || newState.view === 'loading') {
            historyRef.current = [];
        } else if (current.view !== 'loading' && current.view !== newState.view) {
            historyRef.current.push(current);
            if (historyRef.current.length > 20) {
                historyRef.current.shift();
            }
        }
        appStateRef.current = newState;
        setAppState(newState);
    }, []);

    const goBack = useCallback(() => {
        const current = appStateRef.current;
        let nextState: AppState;
        if ('previousView' in current && current.previousView) {
            nextState = current.previousView;
            const last = historyRef.current[historyRef.current.length - 1];
            if (last && last.view === nextState.view) {
                historyRef.current.pop();
            }
        } else if (historyRef.current.length > 0) {
            nextState = historyRef.current.pop()!;
        } else {
            nextState = { view: "logged_in" };
        }
        appStateRef.current = nextState;
        setAppState(nextState);
    }, []);

    const checkProfile = useCallback(async () => {
        logger.info("NavigationContext: Initializing profile check...");
        try {
            // First, check if we are already logged in (session active)
            try {
                const profile = await profileService.getProfile();
                const displayName = profile.firstName || profile.id;
                logger.info(`Auto-login successful for profile: ${displayName}`);
                notifyLoginRef.current(displayName);
                historyRef.current = [];
                setAppState({ view: "logged_in" });
                return;
            } catch {
                logger.info("No active session found, checking available profiles.");
            }

            await authService.listProfiles();
            historyRef.current = [];
            setAppState({ view: "needs_login" });
        } catch (e) {
            error(`Failed to check if profile exists: ${stringifyError(e)}`);
            historyRef.current = [];
            setAppState({ view: "needs_login" });
        }
    }, []);

    useEffect(() => {
        getVersion().then(setAppVersion);
        checkProfile();
    }, [checkProfile]);

    return (
        <NavigationContext.Provider value={{ 
            appState, 
            navigate, 
            goBack, 
            isSidebarOpen, 
            setSidebarOpen, 
            appVersion,
            checkProfile
        }}>
            {children}
        </NavigationContext.Provider>
    );
}
