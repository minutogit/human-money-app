import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import "./App.css";
import { ForkLockOverlay } from './components/auth/ForkLockOverlay';
import { Button } from './components/ui/Button';
import { AlertBanner } from './components/ui/AlertBanner';
import { logger } from "./utils/log";

// Contexts
import { SessionProvider, useSession } from './context/SessionContext';
import { UiModeProvider } from './context/UiModeContext';
import { NavigationProvider, useNavigation } from './context/NavigationContext';

// Components
import { Sidebar } from './components/shell/Sidebar';
import { Header } from './components/shell/Header';
import { AppRouter } from "./components/shell/AppRouter";
import { GlobalFileDropZone } from "./components/shell/GlobalFileDropZone";
import { GuardianReleaseModal } from "./components/auth/GuardianReleaseModal";
import { GlobalProfileEditorModal } from "./components/profile/GlobalProfileEditorModal";
import { EventToastStack } from "./components/shell/EventToastStack";
import { parseCardUri } from "./utils/humocoUri";
import { marketService } from "./services/marketService";
import { recoveryService } from "./services/recoveryService";
import { isPublicView } from "./types";
import type { HelpTopicId } from "./help/helpTopics";

function AppContent() {
    const { t } = useTranslation();
    const { appState, navigate } = useNavigation();
    const { isForkLocked, isRecoveryRequired, clearLocks, isSessionActive, isAuthenticated, protectAction } = useSession();
    const isLoggedIn = isAuthenticated ?? isSessionActive;

    const appStateViewRef = useRef(appState.view);
    appStateViewRef.current = appState.view;

    const [releaseModalOpen, setReleaseModalOpen] = useState(false);
    const [releaseRequestUri, setReleaseRequestUri] = useState<string | undefined>(undefined);
    const [releaseInitialOwnerDid, setReleaseInitialOwnerDid] = useState<string | undefined>(undefined);
    const [pendingDeepLink, setPendingDeepLink] = useState<string | null>(null);
    const [deepLinkFeedback, setDeepLinkFeedback] = useState<string | null>(null);

    const handleDeepLink = useCallback((uri: string) => {
        if (!uri) return;
        setDeepLinkFeedback(null);

        const handleUriImport = async (targetUri: string) => {
            const parsed = parseCardUri(targetUri);
            if (parsed) {
                try {
                    const res = await protectAction(async (pwd) => {
                        return await marketService.importContactCard(targetUri, pwd || undefined);
                    });
                    if (res && res.status === 'accepted' && res.did) {
                        navigate({ view: 'marketplace', selectedDid: res.did });
                    } else if (res) {
                        const statusMap: Record<string, string> = {
                            ignored_stale: t('marketplace.importIgnoredDesc'),
                            rejected_expired: t('marketplace.importExpiredDesc'),
                            rejected_hops: t('marketplace.importHopsDesc'),
                            rejected_offender: t('marketplace.importOffenderDesc'),
                        };
                        setDeepLinkFeedback(statusMap[res.status] || res.status);
                    }
                } catch (err) {
                    logger.error("Failed to import card from URI: " + String(err));
                    setDeepLinkFeedback(String(err));
                }
            }
        };

        const lowerUri = uri.toLowerCase();
        if (lowerUri.includes('humoco://card') || lowerUri.includes('?env=')) {
            if (isLoggedIn) {
                handleUriImport(uri);
            } else {
                setPendingDeepLink(uri);
            }
        } else if (lowerUri.includes('humoco://pay')) {
            if (isLoggedIn) {
                navigate({ view: 'send_vouchers', initialPaymentUri: uri });
            } else {
                setPendingDeepLink(uri);
            }
        } else if (lowerUri.includes('humoco://contact')) {
            if (isLoggedIn) {
                navigate({ view: 'address_book', initialContactPayload: uri });
            } else {
                setPendingDeepLink(uri);
            }
        } else if (lowerUri.includes('humoco://recovery-reply')) {
            recoveryService
                .getActiveRecoverySession()
                .then((session) => {
                    if (session?.sessionMode === 'reset_local_password') {
                        navigate({ view: 'needs_recovery', initialPayload: uri });
                    } else {
                        navigate({ view: 'restore_backup', initialMethod: 'social', initialPayload: uri });
                    }
                })
                .catch(() => {
                    navigate({ view: 'restore_backup', initialMethod: 'social', initialPayload: uri });
                });
        } else if (lowerUri.includes('humoco://kit')) {
            if (appStateViewRef.current === 'needs_recovery') {
                navigate({ view: 'needs_recovery', initialPayload: uri });
            } else {
                navigate({ view: 'restore_backup', initialMethod: 'words_or_sheet', initialPayload: uri });
            }
        } else if (lowerUri.includes('humoco://recovery-req')) {
            if (isLoggedIn) {
                setReleaseRequestUri(uri);
                setReleaseModalOpen(true);
            } else {
                setPendingDeepLink(uri);
            }
        }
    }, [isLoggedIn, navigate, protectAction]);

    useEffect(() => {
        if (isLoggedIn && pendingDeepLink) {
            const linkToRun = pendingDeepLink;
            setPendingDeepLink(null);
            handleDeepLink(linkToRun);
        }
    }, [isLoggedIn, pendingDeepLink, handleDeepLink]);

    useEffect(() => {
        if (typeof window !== 'undefined') {
            const currentUrl = window.location.href;
            if (currentUrl.toLowerCase().includes('humoco://') || currentUrl.toLowerCase().includes('?env=')) {
                handleDeepLink(currentUrl);
            }

            const onDeepLink = (e: Event) => {
                const customEv = e as CustomEvent<string>;
                if (customEv.detail) {
                    handleDeepLink(customEv.detail);
                }
            };
            window.addEventListener('humoco:deeplink', onDeepLink);
            return () => {
                window.removeEventListener('humoco:deeplink', onDeepLink);
            };
        }
    }, [handleDeepLink]);

    useEffect(() => {
        const onOpenRelease = (e: Event) => {
            const customEv = e as CustomEvent<{ requestUri?: string; ownerDid?: string } | undefined>;
            setReleaseRequestUri(customEv.detail?.requestUri);
            setReleaseInitialOwnerDid(customEv.detail?.ownerDid);
            setReleaseModalOpen(true);
        };
        window.addEventListener('humoco:open-guardian-release', onOpenRelease);
        return () => window.removeEventListener('humoco:open-guardian-release', onOpenRelease);
    }, []);

    useEffect(() => {
        if (isForkLocked || isRecoveryRequired) {
            logger.warn("Global lock detected, switching to recovery view state.");
        }
    }, [isForkLocked, isRecoveryRequired]);

    useEffect(() => {
        if (!isLoggedIn && !isPublicView(appState.view)) {
            logger.warn(`Session is inactive but view is "${appState.view}". Redirecting to "needs_login".`);
            navigate({ view: 'needs_login' });
        }
    }, [isLoggedIn, appState.view, navigate]);

    useEffect(() => {
        const handleHelpCenterNav = (e: Event) => {
            const customEvent = e as CustomEvent<{ topic?: HelpTopicId }>;
            navigate({ view: 'help_center', topic: customEvent.detail?.topic, previousView: appState });
        };
        window.addEventListener('humoco:navigate-help-center', handleHelpCenterNav);
        return () => window.removeEventListener('humoco:navigate-help-center', handleHelpCenterNav);
    }, [navigate, appState]);

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
                <Sidebar />

                <GlobalFileDropZone>
                    <div className="flex flex-1 flex-col min-w-0 h-full overflow-hidden">
                        <Header />
                        <main className="w-full flex-1 min-h-0 overflow-y-auto px-3 sm:px-5 lg:px-8 pb-6">
                            {!isLoggedIn && pendingDeepLink && (
                                <div className="max-w-xl mx-auto mt-3 mb-2" data-testid="pending-deeplink-banner">
                                    <AlertBanner variant="info" onDismiss={() => setPendingDeepLink(null)}>
                                        {t('auth.pendingLinkNotice')}
                                    </AlertBanner>
                                </div>
                            )}
                            {deepLinkFeedback && (
                                <div className="max-w-xl mx-auto mt-3 mb-2" data-testid="deeplink-feedback-banner">
                                    <AlertBanner variant="warning" onDismiss={() => setDeepLinkFeedback(null)}>
                                        {deepLinkFeedback}
                                    </AlertBanner>
                                </div>
                            )}
                            <AppRouter />
                        </main>
                    </div>
                </GlobalFileDropZone>

                {/* Fork detected overlay */}
                {isForkLocked && appState.view !== 'help_center' && (
                    <ForkLockOverlay 
                        onStartRecovery={() => {
                            clearLocks();
                            navigate({ view: "needs_recovery" });
                        }} 
                    />
                )}
                
                {/* Fallback for general recovery required (no seal) */}
                {isRecoveryRequired && !isForkLocked && appState.view !== 'help_center' && (
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
                <GuardianReleaseModal
                    isOpen={releaseModalOpen}
                    onClose={() => {
                        setReleaseModalOpen(false);
                        setReleaseRequestUri(undefined);
                        setReleaseInitialOwnerDid(undefined);
                    }}
                    initialRequestUri={releaseRequestUri}
                    initialOwnerDid={releaseInitialOwnerDid}
                />
                <GlobalProfileEditorModal />
                <EventToastStack />
            </div>
    );
}

function App() {
    return (
        <SessionProvider>
            <UiModeProvider>
                <NavigationProvider>
                    <AppContent />
                </NavigationProvider>
            </UiModeProvider>
        </SessionProvider>
    );
}

export default App;