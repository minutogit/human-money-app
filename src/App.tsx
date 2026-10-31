import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import "./App.css";
import { ForkLockOverlay } from './components/ForkLockOverlay';
import { Button } from './components/ui/Button';
import { logger } from "./utils/log";

// Contexts
import { SessionProvider, useSession } from './context/SessionContext';
import { NavigationProvider, useNavigation } from './context/NavigationContext';

// Components
import { Sidebar } from './components/Sidebar';
import { Header } from './components/Header';
import { AppRouter } from "./components/AppRouter";
import { GlobalFileDropZone } from "./components/GlobalFileDropZone";

function AppContent() {
    const { t } = useTranslation();
    const { appState, navigate } = useNavigation();
    const { isForkLocked, isRecoveryRequired, clearLocks, isSessionActive, isAuthenticated } = useSession();
    const isLoggedIn = isAuthenticated ?? isSessionActive;

    useEffect(() => {
        if (isForkLocked || isRecoveryRequired) {
            logger.warn("Global lock detected, switching to recovery view state.");
        }
    }, [isForkLocked, isRecoveryRequired]);

    useEffect(() => {
        const PUBLIC_VIEWS = ['loading', 'needs_profile', 'needs_login', 'recreate_profile', 'restore_backup', 'needs_recovery', 'concept'];
        if (!isLoggedIn && !PUBLIC_VIEWS.includes(appState.view)) {
            logger.warn(`Session is inactive but view is "${appState.view}". Redirecting to "needs_login".`);
            navigate({ view: 'needs_login' });
        }
    }, [isLoggedIn, appState.view, navigate]);

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
                <Sidebar />

                <GlobalFileDropZone>
                    <div className="flex flex-1 flex-col overflow-y-auto">
                        <Header />

                        <main className="w-full flex-grow p-4 md:p-6 lg:p-8">
                            <AppRouter />
                        </main>
                    </div>
                </GlobalFileDropZone>

                {/* Fork detected overlay */}
                {isForkLocked && (
                    <ForkLockOverlay 
                        onStartRecovery={() => {
                            clearLocks();
                            navigate({ view: "needs_recovery" });
                        }} 
                    />
                )}
                
                {/* Fallback for general recovery required (no seal) */}
                {isRecoveryRequired && !isForkLocked && (
                    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
                         <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl p-8 text-center shadow-xl">
                            <h2 className="text-xl font-bold mb-4">{t('profile.recoveryRequiredTitle')}</h2>
                            <p className="text-theme-light mb-6">{t('profile.recoveryRequiredDescription')}</p>
                            <Button 
                                className="w-full"
                                onClick={() => {
                                    clearLocks();
                                    navigate({ view: "needs_recovery" });
                                }}
                            >
                                {t('profile.startRecovery')}
                            </Button>
                        </div>
                    </div>
                )}
            </div>
    );
}

function App() {
    return (
        <SessionProvider>
            <NavigationProvider>
                <AppContent />
            </NavigationProvider>
        </SessionProvider>
    );
}

export default App;