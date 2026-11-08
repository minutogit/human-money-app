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
import { parseCardUri } from "./utils/humocoUri";
import { marketService } from "./services/marketService";

function AppContent() {
    const { t } = useTranslation();
    const { appState, navigate } = useNavigation();
    const { isForkLocked, isRecoveryRequired, clearLocks, isSessionActive, isAuthenticated, protectAction } = useSession();
    const isLoggedIn = isAuthenticated ?? isSessionActive;

    useEffect(() => {
        if (!isLoggedIn) return;

        const handleUriImport = async (uri: string) => {
            const parsed = parseCardUri(uri);
            if (parsed) {
                try {
                    const res = await protectAction(async (pwd) => {
                        return await marketService.importContactCard(uri, pwd || undefined);
                    });
                    if (res) {
                        navigate({ view: 'marketplace', selectedDid: res.did ?? undefined });
                    }
                } catch (err) {
                    logger.error("Failed to import card from URI: " + String(err));
                }
            }
        };

        if (typeof window !== 'undefined') {
            const currentUrl = window.location.href;
            if (currentUrl.includes('humoco://card') || currentUrl.includes('?env=')) {
                handleUriImport(currentUrl);
            }

            const onDeepLink = (e: Event) => {
                const customEv = e as CustomEvent<string>;
                if (customEv.detail && (customEv.detail.includes('humoco://card') || customEv.detail.includes('?env='))) {
                    handleUriImport(customEv.detail);
                }
            };
            window.addEventListener('humoco:deeplink', onDeepLink);
            return () => {
                window.removeEventListener('humoco:deeplink', onDeepLink);
            };
        }
    }, [isLoggedIn, navigate, protectAction]);

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
                    <div className="flex flex-1 flex-col min-w-0 h-full overflow-hidden">
                        <Header />
                        <main className="w-full flex-1 min-h-0 overflow-y-auto px-3 sm:px-5 lg:px-8 pb-6">
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