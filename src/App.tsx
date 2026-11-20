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
import { classifyExtension, classifyHumocoFile, extractHumocoDeepLink, formatCardImportStatusFeedback } from "./utils/humocoFileClassifier";
import { marketService } from "./services/marketService";
import { recoveryService } from "./services/recoveryService";
import { backupService } from "./services/backupService";
import { translateError } from "./utils/errorHelper";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { isPublicView } from "./types";
import type { HelpTopicId } from "./help/helpTopics";

function isPreLoginDeepLink(uri: string): boolean {
    const lower = uri.toLowerCase();
    return lower.includes('humoco://kit') || lower.includes('humoco://recovery-reply');
}

type ClassifiedLaunchItem =
    | { kind: 'uri'; uri: string; preLogin: boolean }
    | { kind: 'backup'; bytes: number[]; preLogin: true }
    | {
          kind: 'file';
          fileName: string;
          bytes: number[];
          classification: ReturnType<typeof classifyHumocoFile>;
          preLogin: false;
      }
    | { kind: 'unknown'; preLogin: true };

function AppContent() {
    const { t } = useTranslation();
    const { appState, navigate, pendingDroppedFile, setPendingDroppedFile } = useNavigation();
    const { isForkLocked, isRecoveryRequired, clearLocks, isAuthenticated, protectAction } = useSession();
    const isLoggedIn = isAuthenticated;
    const isAuthReady = isLoggedIn && appState.view !== 'needs_profile';

    const appStateViewRef = useRef(appState.view);
    appStateViewRef.current = appState.view;
    const isAuthReadyRef = useRef(isAuthReady);
    isAuthReadyRef.current = isAuthReady;
    const hasCheckedStartupItemsRef = useRef(false);
    const isProcessingLaunchQueueRef = useRef(false);
    const classifiedLaunchCacheRef = useRef<Map<string, ClassifiedLaunchItem>>(new Map());
    const deferredLaunchItemsRef = useRef<Set<string>>(new Set());
    const reservedLaunchItemRef = useRef<string | null>(null);
    const skippedLaunchItemsRef = useRef<Set<string>>(new Set());

    const [reservedLaunchItem, setReservedLaunchItem] = useState<string | null>(null);
    const [releaseModalOpen, setReleaseModalOpen] = useState(false);
    const [releaseRequestUri, setReleaseRequestUri] = useState<string | undefined>(undefined);
    const [releaseInitialOwnerDid, setReleaseInitialOwnerDid] = useState<string | undefined>(undefined);
    const [launchQueue, setLaunchQueue] = useState<string[]>([]);
    const [launchQueueKick, setLaunchQueueKick] = useState(0);
    const [hasDeferredLink, setHasDeferredLink] = useState(false);
    const [hasDeferredFile, setHasDeferredFile] = useState(false);
    const [deepLinkFeedback, setDeepLinkFeedback] = useState<string | null>(null);

    const enqueueLaunchItems = useCallback((items: string[]) => {
        if (!Array.isArray(items) || items.length === 0) return;
        setLaunchQueue((prev) => {
            const next = [...prev];
            let changed = false;
            for (const raw of items) {
                const item = typeof raw === 'string' ? raw.trim() : '';
                if (item && !next.includes(item)) {
                    next.push(item);
                    changed = true;
                }
            }
            return changed ? next : prev;
        });
    }, []);

    const ackLaunchItem = useCallback(async (item: string) => {
        classifiedLaunchCacheRef.current.delete(item);
        deferredLaunchItemsRef.current.delete(item);
        setLaunchQueue((prev) => prev.filter((existing) => existing !== item));
        await Promise.resolve(invoke<boolean>("ack_startup_launch_item", { item })).catch(() => false);
    }, []);

    const finishLaunchItem = useCallback(async (item: string) => {
        if (!item) return;
        if (isAuthReadyRef.current) {
            await ackLaunchItem(item);
        }
        if (reservedLaunchItemRef.current === item) {
            reservedLaunchItemRef.current = null;
            setReservedLaunchItem(null);
        }
        setLaunchQueueKick((k) => k + 1);
    }, [ackLaunchItem]);

    useEffect(() => {
        if (!isAuthReady) {
            reservedLaunchItemRef.current = null;
            setReservedLaunchItem(null);
            skippedLaunchItemsRef.current.clear();
        }
    }, [isAuthReady]);

    type LaunchDispatchOutcome = 'ack' | 'reserved' | 'keep_unacked';

    const dispatchDeepLinkUri = useCallback(async (uri: string): Promise<LaunchDispatchOutcome> => {
        if (!uri) return 'ack';
        setDeepLinkFeedback(null);
        const normalizedUri = extractHumocoDeepLink(uri) || uri;
        const lowerUri = normalizedUri.toLowerCase();

        if (lowerUri.includes('humoco://card')) {
            const parsed = parseCardUri(normalizedUri);
            if (parsed) {
                try {
                    const res = await protectAction(async (pwd) => {
                        return await marketService.importContactCard(normalizedUri, pwd || undefined);
                    });
                    if (!res) {
                        return 'keep_unacked';
                    }
                    if (res.status === 'accepted' && res.did) {
                        navigate({ view: 'marketplace', selectedDid: res.did });
                    } else {
                        setDeepLinkFeedback(formatCardImportStatusFeedback(res.status, t));
                    }
                } catch (err) {
                    if (!isAuthReadyRef.current) {
                        return 'keep_unacked';
                    }
                    logger.error("Failed to import card from URI: " + String(err));
                    setDeepLinkFeedback(translateError(err, t));
                }
            }
            return 'ack';
        } else if (lowerUri.includes('humoco://pay')) {
            navigate({ view: 'send_vouchers', initialPaymentUri: normalizedUri });
        } else if (lowerUri.includes('humoco://contact')) {
            navigate({ view: 'address_book', initialContactPayload: normalizedUri });
        } else if (lowerUri.includes('humoco://recovery-reply')) {
            try {
                const session = await recoveryService.getActiveRecoverySession();
                if (session?.sessionMode === 'reset_local_password') {
                    navigate({ view: 'needs_recovery', initialPayload: normalizedUri });
                } else {
                    navigate({ view: 'restore_backup', initialMethod: 'social', initialPayload: normalizedUri });
                }
            } catch {
                navigate({ view: 'restore_backup', initialMethod: 'social', initialPayload: normalizedUri });
            }
        } else if (lowerUri.includes('humoco://kit')) {
            if (appStateViewRef.current === 'needs_recovery') {
                navigate({ view: 'needs_recovery', initialPayload: normalizedUri });
            } else {
                navigate({ view: 'restore_backup', initialMethod: 'words_or_sheet', initialPayload: normalizedUri });
            }
        } else if (lowerUri.includes('humoco://recovery-req')) {
            setReleaseRequestUri(normalizedUri);
            setReleaseModalOpen(true);
        }
        return 'ack';
    }, [navigate, protectAction, t]);

    const classifyLaunchItem = useCallback(async (item: string): Promise<ClassifiedLaunchItem> => {
        const cached = classifiedLaunchCacheRef.current.get(item);
        if (cached) return cached;

        let resolved: ClassifiedLaunchItem;
        if (item.toLowerCase().includes('humoco://')) {
            const uri = extractHumocoDeepLink(item) || item;
            resolved = isPreLoginDeepLink(uri)
                ? { kind: 'uri', uri, preLogin: true }
                : { kind: 'uri', uri, preLogin: false };
        } else {
            try {
                const rawBytes = await readFile(item);
                const bytes = Array.from(rawBytes);
                const fileName = item.split(/[/\\]/).pop() || 'launch.humoco';
                const classification = classifyHumocoFile({ fileName, bytes });
                if (classification.kind === 'deeplink' && classification.deepLink) {
                    const uri = classification.deepLink;
                    resolved = isPreLoginDeepLink(uri)
                        ? { kind: 'uri', uri, preLogin: true }
                        : { kind: 'uri', uri, preLogin: false };
                } else if (classification.kind === 'backup' || classifyExtension(fileName) === 'backup') {
                    resolved = { kind: 'backup', bytes, preLogin: true };
                } else if (classification.kind === 'unknown') {
                    resolved = { kind: 'unknown', preLogin: true };
                } else {
                    resolved = { kind: 'file', fileName, bytes, classification, preLogin: false };
                }
            } catch (err) {
                logger.warn("Failed to open launch item: " + String(err));
                resolved = { kind: 'unknown', preLogin: true };
            }
        }

        classifiedLaunchCacheRef.current.set(item, resolved);
        return resolved;
    }, []);

    const executeClassifiedItem = useCallback(async (classified: ClassifiedLaunchItem, launchItem: string): Promise<LaunchDispatchOutcome> => {
        if (classified.kind === 'uri') {
            return await dispatchDeepLinkUri(classified.uri);
        } else if (classified.kind === 'backup') {
            const stagedPath = await backupService.stageDroppedBackupBytes(classified.bytes);
            navigate({
                view: 'restore_backup',
                initialMethod: 'file_or_folder',
                initialBackupPath: stagedPath,
            });
            return 'ack';
        } else if (classified.kind === 'file') {
            if (classified.classification.kind === 'card') {
                const text = new TextDecoder().decode(new Uint8Array(classified.bytes));
                try {
                    const res = await protectAction(async (pwd) => {
                        return await marketService.importContactCard(text, pwd || undefined);
                    });
                    if (!res) {
                        return 'keep_unacked';
                    }
                    if (res.status === 'accepted' && res.did) {
                        navigate({ view: 'marketplace', selectedDid: res.did });
                    } else {
                        setDeepLinkFeedback(formatCardImportStatusFeedback(res.status, t));
                    }
                } catch (err) {
                    if (!isAuthReadyRef.current) {
                        return 'keep_unacked';
                    }
                    setDeepLinkFeedback(translateError(err, t));
                }
                return 'ack';
            } else {
                reservedLaunchItemRef.current = launchItem;
                setReservedLaunchItem(launchItem);
                navigate({
                    view: 'receive_bundle',
                    droppedFile: { name: classified.fileName, bytes: classified.bytes, launchItem },
                });
                return 'reserved';
            }
        }
        return 'ack';
    }, [dispatchDeepLinkUri, navigate, protectAction, t]);

    // Process FIFO launch queue:
    // - Pre-login items (kit, recovery-reply, backup) process & ack immediately.
    // - Auth-required items stay persisted & un-acked in backend until login & import finish.
    useEffect(() => {
        if (launchQueue.length === 0) {
            setHasDeferredLink(false);
            setHasDeferredFile(false);
            return;
        }
        if (isProcessingLaunchQueueRef.current) return;

        let cancelled = false;
        let didAdvance = false;
        const processNext = async () => {
            isProcessingLaunchQueueRef.current = true;
            try {
                if (!isAuthReady) {
                    let anyFile = false;
                    let anyLink = false;
                    for (const item of launchQueue) {
                        if (cancelled) break;
                        const classified = await classifyLaunchItem(item);
                        if (classified.preLogin) {
                            await executeClassifiedItem(classified, item);
                            await ackLaunchItem(item);
                            didAdvance = true;
                            break;
                        } else {
                            deferredLaunchItemsRef.current.add(item);
                            if (classified.kind === 'file') {
                                anyFile = true;
                            } else {
                                anyLink = true;
                            }
                        }
                    }
                    if (!cancelled) {
                        setHasDeferredFile(anyFile);
                        setHasDeferredLink(anyLink && !anyFile);
                    }
                } else {
                    deferredLaunchItemsRef.current.clear();
                    setHasDeferredFile(false);
                    setHasDeferredLink(false);
                    if (reservedLaunchItemRef.current) {
                        return;
                    }
                    const nextItem = launchQueue.find((item) => !skippedLaunchItemsRef.current.has(item));
                    if (nextItem) {
                        const classified = await classifyLaunchItem(nextItem);
                        const outcome = await executeClassifiedItem(classified, nextItem);
                        if (outcome === 'ack') {
                            if (classified.preLogin || isAuthReadyRef.current) {
                                await ackLaunchItem(nextItem);
                                didAdvance = true;
                            }
                        } else if (outcome === 'keep_unacked') {
                            if (isAuthReadyRef.current) {
                                skippedLaunchItemsRef.current.add(nextItem);
                                didAdvance = true;
                            }
                        }
                    }
                }
            } finally {
                isProcessingLaunchQueueRef.current = false;
                if (didAdvance && !cancelled) {
                    setLaunchQueueKick((k) => k + 1);
                }
            }
        };

        void processNext();
        return () => {
            cancelled = true;
        };
    }, [launchQueue, launchQueueKick, reservedLaunchItem, isAuthReady, classifyLaunchItem, executeClassifiedItem, ackLaunchItem]);

    useEffect(() => {
        if (!isLoggedIn || appState.view === 'needs_profile' || !pendingDroppedFile) return;
        const queued = pendingDroppedFile;
        setPendingDroppedFile(null);
        const classification = classifyHumocoFile({ fileName: queued.name, bytes: queued.bytes });
        if (classification.kind === 'card') {
            const text = new TextDecoder().decode(new Uint8Array(queued.bytes));
            protectAction(async (pwd) => {
                return await marketService.importContactCard(text, pwd || undefined);
            })
                .then((res) => {
                    if (res && res.status === 'accepted' && res.did) {
                        navigate({ view: 'marketplace', selectedDid: res.did });
                    } else if (res) {
                        setDeepLinkFeedback(formatCardImportStatusFeedback(res.status, t));
                    }
                })
                .catch((err) => setDeepLinkFeedback(translateError(err, t)));
        } else if (classification.kind !== 'unknown') {
            navigate({ view: 'receive_bundle', droppedFile: queued });
        }
    }, [isLoggedIn, appState.view, pendingDroppedFile, setPendingDroppedFile, protectAction, navigate, t]);

    useEffect(() => {
        if (typeof window !== 'undefined') {
            const currentUrl = window.location.href;
            if (currentUrl.toLowerCase().includes('humoco://')) {
                enqueueLaunchItems([currentUrl]);
            }

            if (!hasCheckedStartupItemsRef.current) {
                hasCheckedStartupItemsRef.current = true;
                Promise.resolve(invoke<string[]>("peek_startup_launch_items"))
                    .then((items) => enqueueLaunchItems(items))
                    .catch(() => {});
            }

            let cancelled = false;
            let unlistenLaunchItems: (() => void) | undefined;
            Promise.resolve(
                listen<string[]>("humoco://launch-items", (event) => {
                    enqueueLaunchItems(event.payload);
                })
            )
                .then((unlisten) => {
                    if (cancelled) {
                        unlisten();
                    } else {
                        unlistenLaunchItems = unlisten;
                    }
                })
                .catch(() => {});

            const onDeepLink = (e: Event) => {
                const customEv = e as CustomEvent<string>;
                if (customEv.detail) {
                    enqueueLaunchItems([customEv.detail]);
                }
            };
            window.addEventListener('humoco:deeplink', onDeepLink);
            return () => {
                cancelled = true;
                if (typeof unlistenLaunchItems === 'function') {
                    unlistenLaunchItems();
                }
                window.removeEventListener('humoco:deeplink', onDeepLink);
            };
        }
    }, [enqueueLaunchItems]);

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
            navigate({
                view: 'help_center',
                topic: customEvent.detail?.topic,
                previousView: appState.view === 'help_center' ? appState.previousView : appState,
            });
        };
        window.addEventListener('humoco:navigate-help-center', handleHelpCenterNav);
        return () => window.removeEventListener('humoco:navigate-help-center', handleHelpCenterNav);
    }, [navigate, appState]);

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
                <GlobalFileDropZone>
                    <div className="flex flex-1 min-h-0 w-full overflow-hidden">
                        <Sidebar />

                        <div className="flex flex-1 flex-col min-w-0 h-full overflow-hidden">
                            <Header />
                            <main className="w-full flex-1 min-h-0 overflow-y-auto px-3 sm:px-5 lg:px-8 pb-6">
                                {!isLoggedIn && (hasDeferredLink || hasDeferredFile || pendingDroppedFile || launchQueue.length > 0) && (
                                    <div className="max-w-xl mx-auto mt-3 mb-2" data-testid="pending-deeplink-banner">
                                        <AlertBanner
                                            variant="info"
                                            onDismiss={() => {
                                                const toDismiss = [...launchQueue];
                                                setPendingDroppedFile(null);
                                                setHasDeferredFile(false);
                                                setHasDeferredLink(false);
                                                for (const item of toDismiss) {
                                                    void ackLaunchItem(item);
                                                }
                                            }}
                                        >
                                            {(pendingDroppedFile || hasDeferredFile)
                                                ? t('auth.pendingFileNotice', 'Datei vorgemerkt – wird direkt nach dem Anmelden geöffnet.')
                                                : t('auth.pendingLinkNotice')}
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
                                <AppRouter onFinishLaunchItem={(item) => void finishLaunchItem(item)} />
                            </main>
                        </div>
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