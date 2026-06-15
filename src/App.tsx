import { useEffect } from "react";
import logo from './assets/logo.png';
import "./App.css";
import { ForkLockOverlay } from './components/ForkLockOverlay';
import { Button } from './components/ui/Button';
import { logger } from "./utils/log";

// Contexts
import { SessionProvider, useSession } from './context/SessionContext';
import { NavigationProvider, useNavigation } from './context/NavigationContext';

// Components
import { Sidebar, INTERNAL_VIEWS } from './components/Sidebar';
import { AppRouter } from "./components/AppRouter";

function AppContent() {
    const { appState, navigate, setSidebarOpen } = useNavigation();
    const { isForkLocked, isRecoveryRequired, clearLocks, isSessionActive } = useSession();

    useEffect(() => {
        if (isForkLocked || isRecoveryRequired) {
            logger.warn("Global lock detected, switching to recovery view state.");
        }
    }, [isForkLocked, isRecoveryRequired]);

    useEffect(() => {
        const PUBLIC_VIEWS = ['loading', 'needs_profile', 'needs_login', 'recreate_profile', 'needs_recovery', 'concept'];
        if (!isSessionActive && !PUBLIC_VIEWS.includes(appState.view)) {
            logger.warn(`Session is inactive but view is "${appState.view}". Redirecting to "needs_login".`);
            navigate({ view: 'needs_login' });
        }
    }, [isSessionActive, appState.view, navigate]);

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
                <Sidebar />

                <div className="flex flex-1 flex-col overflow-y-auto">
                    {INTERNAL_VIEWS.includes(appState.view) && isSessionActive && (
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

                    <main className={`w-full flex-grow ${INTERNAL_VIEWS.includes(appState.view) && isSessionActive ? 'p-4 md:p-6 lg:p-8' : ''}`}>
                        <AppRouter />
                    </main>
                </div>

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
                            <h2 className="text-xl font-bold mb-4">Recovery Required</h2>
                            <p className="text-theme-light mb-6">Your security seal has been lost. You must perform a recovery to continue using this wallet.</p>
                            <Button 
                                className="w-full"
                                onClick={() => {
                                    clearLocks();
                                    navigate({ view: "needs_recovery" });
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