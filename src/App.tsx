// src/App.tsx
import { useState, useEffect } from "react";
import { profileService } from "./services/profileService";
import { authService } from "./services/authService";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { error } from "@tauri-apps/plugin-log";
import { logger } from "./utils/log";
import logo from './assets/logo.png';
import "./App.css";
import { ForkLockOverlay } from './components/ForkLockOverlay';
import { Button } from './components/ui/Button';
import { AppState } from './types';

// WICHTIG: Der Import für den Provider
import { SessionProvider, useSession } from './context/SessionContext';
import { Sidebar, INTERNAL_VIEWS } from './components/Sidebar';
import { AppRouter } from "./components/AppRouter";

function AppContent() {
    const [appState, setAppState] = useState<AppState>({ view: "loading" });
    const [profileName, setProfileName] = useState<string>("");
    const [isSidebarOpen, setSidebarOpen] = useState(false);
    const [appVersion, setAppVersion] = useState<string>("");
    
    // Zugriff auf SessionContext
    const { notifyLogin, notifyLogout, isForkLocked, isRecoveryRequired, clearLocks } = useSession();

    useEffect(() => {
        if (isForkLocked || isRecoveryRequired) {
            logger.warn("Global lock detected, switching to recovery view state.");
        }
    }, [isForkLocked, isRecoveryRequired]);

    useEffect(() => {
        getVersion().then(setAppVersion);
    }, []);

    useEffect(() => {
        // Log that the frontend application is starting
        logger.info("Frontend application starting, initializing profile check...");

        async function checkProfile() {
            try {
                // First, check if we are already logged in (session active)
                try {
                    const profile = await profileService.getProfile();
                    const displayName = profile.firstName || profile.id;
                    logger.info(`Auto-login successful for profile: ${displayName}`);
                    setProfileName(displayName);
                    notifyLogin();
                    setAppState({ view: "logged_in" });
                    return; // Exit early if auto-login worked
                } catch {
                    // Not logged in or session locked, proceed with normal flow
                    logger.info("No active session found, checking available profiles.");
                }

                const profiles = await authService.listProfiles();
                setAppState({ view: profiles.length > 0 ? "needs_login" : "needs_profile" });
            } catch (e) {
                error(`Failed to check if profile exists: ${e}`);
                // Fallback to creation if there's an error, as it's the safest default
                setAppState({ view: "needs_profile" });
            }
        }
        checkProfile();
    }, [notifyLogin]);

    useEffect(() => {
        async function updateTitle() {
            const win = getCurrentWindow();
            if (profileName) {
                await win.setTitle(`Human Money App - ${profileName}`);
            } else {
                await win.setTitle('Human Money App');
            }
        }

        updateTitle();
    }, [profileName]);

    function handleLogout() {
        profileService.logout().catch(e => error(`Logout failed: ${e}`));
        setSidebarOpen(false);
        // Reset the profile name on logout
        setProfileName("");
        setAppState({ view: "needs_login" });
        notifyLogout();
    }

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
                <Sidebar 
                    appState={appState}
                    setAppState={setAppState}
                    profileName={profileName}
                    appVersion={appVersion}
                    onLogout={handleLogout}
                    isOpen={isSidebarOpen}
                    setIsOpen={setSidebarOpen}
                />

                <div className="flex flex-1 flex-col overflow-y-auto">
                    {INTERNAL_VIEWS.includes(appState.view) && (
                        <header className="sticky top-0 z-20 flex h-16 items-center justify-between border-b border-theme-subtle bg-card px-4 shadow-sm md:hidden">
                            <button
                                onClick={() => setSidebarOpen(true)}
                                className="rounded p-2 text-theme-secondary hover:bg-bg-app focus:outline-none focus:ring-2 focus:ring-theme-accent"
                                aria-label="Open navigation"
                            >
                                <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16"></path>
                                </svg>
                            </button>
                            <div className="flex items-center gap-2">
                                <img src={logo} alt="Logo" className="w-8 h-8 object-contain" />
                                <h1 className="text-lg font-bold text-theme-primary">Human Money App</h1>
                            </div>
                            <div className="w-10"></div> {/* Spacer for symmetry */}
                        </header>
                    )}

                    <main className={`w-full flex-grow ${INTERNAL_VIEWS.includes(appState.view) ? 'p-4 md:p-6 lg:p-8' : ''}`}>
                        <AppRouter 
                            appState={appState} 
                            setAppState={setAppState} 
                            profileName={profileName} 
                            setProfileName={setProfileName} 
                        />
                    </main>
                </div>

                {/* Fork detected overlay */}
                {isForkLocked && (
                    <ForkLockOverlay 
                        onStartRecovery={() => {
                            clearLocks();
                            setAppState({ view: "needs_recovery" });
                        }} 
                    />
                )}
                
                {/* Fallback for general recovery required (no seal) */}
                {isRecoveryRequired && !isForkLocked && (
                    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
                         <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl p-8 text-center shadow-xl">
                            <h2 className="text-xl font-bold mb-4">Recovery Required</h2>
                            <p className="text-theme-light mb-6">Your security seal has been lost. You must perform a recovery to continue using this wallet.</p>
                            <Button 
                                className="w-full"
                                onClick={() => {
                                    clearLocks();
                                    setAppState({ view: "needs_recovery" });
                                }}
                            >
                                Start Recovery
                            </Button>
                        </div>
                    </div>
                )}
            </div>
    );
}

// Wir müssen AppContent in eine Wrapper-Komponente auslagern,
// da useSession nur INNERHALB des SessionProviders funktioniert.
function App() {
    return (
        <SessionProvider>
            <AppContent />
        </SessionProvider>
    );
}

export default App;