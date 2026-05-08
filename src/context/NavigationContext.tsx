// src/context/NavigationContext.tsx
import { createContext, useContext, useState, ReactNode, useEffect, useCallback, useRef } from 'react';
import { AppState } from '../types';
import { getVersion } from "@tauri-apps/api/app";
import { profileService } from "../services/profileService";
import { authService } from "../services/authService";
import { logger } from "../utils/log";
import { useSession } from './SessionContext';
import { error } from "@tauri-apps/plugin-log";

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

    const navigate = useCallback((newState: AppState) => {
        setAppState(newState);
    }, []);

    const goBack = useCallback(() => {
        setAppState(current => {
            if ('previousView' in current && current.previousView) {
                return current.previousView;
            }
            return { view: "logged_in" };
        });
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
                setAppState({ view: "logged_in" });
                return;
            } catch {
                logger.info("No active session found, checking available profiles.");
            }

            const profiles = await authService.listProfiles();
            setAppState({ view: profiles.length > 0 ? "needs_login" : "needs_profile" });
        } catch (e) {
            error(`Failed to check if profile exists: ${e}`);
            setAppState({ view: "needs_profile" });
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
