import { Dispatch, MutableRefObject, SetStateAction, useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { marketService } from '../../services/marketService';
import { rateLimitManager } from '../../sync/rateLimitManager';
import { dispatchBackupUpdated, updateNetworkStatus } from '../../sync/syncBridge';
import {
    FrontendAutoBackupStatusDto,
    FrontendBackupSyncResult,
    FrontendMarketSyncResult,
    FrontendMyMarketProfile,
    FrontendPostSyncResult,
} from '../../types';
import { APP_EVENTS, emitAppEvent } from '../../utils/appEvents';
import { detectIsWifiConnection } from '../../utils/networkStatus';
import {
    MY_PROFILE_UPDATED_EVENT,
    notifyAppNoticesUpdated,
    notifySyncStatusUpdated,
} from '../../utils/syncEvents';
import { tourBridge } from '../../utils/tourBridge';
import type { SecurityLockdownKind } from './types';

interface UseSessionEventBridgeParams {
    isAuthenticated: boolean;
    isAuthenticatedRef: MutableRefObject<boolean>;
    hasCachedSessionRef: MutableRefObject<boolean>;
    sessionGenRef: MutableRefObject<number>;
    setIsAuthenticated: Dispatch<SetStateAction<boolean>>;
    setHasCachedSession: Dispatch<SetStateAction<boolean>>;
    setIsForkLocked: Dispatch<SetStateAction<boolean>>;
    setLockdownKind: Dispatch<SetStateAction<SecurityLockdownKind>>;
    setIsRecoveryRequired: Dispatch<SetStateAction<boolean>>;
    setProfileName: Dispatch<SetStateAction<string>>;
    setDisplayProfileName: Dispatch<SetStateAction<string>>;
    setUserAvatarUrl: Dispatch<SetStateAction<string | null>>;
}

export function useSessionEventBridge({
    isAuthenticated,
    isAuthenticatedRef,
    hasCachedSessionRef,
    sessionGenRef,
    setIsAuthenticated,
    setHasCachedSession,
    setIsForkLocked,
    setLockdownKind,
    setIsRecoveryRequired,
    setProfileName,
    setDisplayProfileName,
    setUserAvatarUrl,
}: UseSessionEventBridgeParams) {
    const syncNetworkStatusNow = useCallback(() => {
        if (typeof navigator === 'undefined') return;
        const isOnline = navigator.onLine !== false;
        const isWifi = detectIsWifiConnection();
        const isVisible =
            typeof document === 'undefined' ? true : document.visibilityState !== 'hidden';
        void updateNetworkStatus(isOnline, isWifi, isVisible);
    }, []);

    // Lightweight network & visibility bridge feeding the backend BackgroundSyncCoordinator.
    useEffect(() => {
        if (typeof window === 'undefined') return;
        syncNetworkStatusNow();
        const handleStateChange = () => syncNetworkStatusNow();
        window.addEventListener('online', handleStateChange);
        window.addEventListener('offline', handleStateChange);
        if (typeof document !== 'undefined') {
            document.addEventListener('visibilitychange', handleStateChange);
        }
        const navConn = (navigator as Navigator & {
            connection?: {
                addEventListener?: (t: string, cb: () => void) => void;
                removeEventListener?: (t: string, cb: () => void) => void;
            };
        }).connection;
        navConn?.addEventListener?.('change', handleStateChange);

        return () => {
            window.removeEventListener('online', handleStateChange);
            window.removeEventListener('offline', handleStateChange);
            if (typeof document !== 'undefined') {
                document.removeEventListener('visibilitychange', handleStateChange);
            }
            navConn?.removeEventListener?.('change', handleStateChange);
        };
    }, [syncNetworkStatusNow]);

    // Subscribe to backend coordinator events so UI components refresh immediately.
    useEffect(() => {
        if (typeof window === 'undefined') return;
        let cancelled = false;
        const unlisteners: Array<() => void> = [];

        const setup = async () => {
            const register = async <T,>(
                event: string,
                handler: (payload: T) => void,
            ): Promise<void> => {
                try {
                    const unlisten = await Promise.resolve(
                        listen<T>(event, (ev) => {
                            if (!cancelled) {
                                handler(ev.payload);
                            }
                        }),
                    );
                    if (typeof unlisten === 'function') {
                        if (cancelled) {
                            unlisten();
                        } else {
                            unlisteners.push(unlisten);
                        }
                    }
                } catch {
                    // Ignore in non-Tauri / unit-test environments without event bridge
                }
            };

            await Promise.all([
                register<FrontendAutoBackupStatusDto | null>(
                    'humoco://backup-updated',
                    (status) => {
                        dispatchBackupUpdated(status ?? undefined);
                    },
                ),
                register<FrontendBackupSyncResult | null>(
                    'humoco://sync-status-updated',
                    (res) => {
                        notifySyncStatusUpdated();
                        if (res) {
                            const retrySecs = rateLimitManager.extractRetryAfter(res);
                            if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        }
                    },
                ),
                register<FrontendPostSyncResult | null>(
                    'humoco://post-synced',
                    (res) => {
                        if (!res) return;
                        const retrySecs = rateLimitManager.extractRetryAfter(res);
                        if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        const hasInboxChanges =
                            res.uploadedTo > 0 ||
                            res.importedPayments > 0 ||
                            res.importedReceipts > 0 ||
                            res.importedSignatures > 0 ||
                            res.newSignRequests > 0 ||
                            res.newWotHandshakes > 0 ||
                            (res.importedWotLists ?? 0) > 0 ||
                            res.newInquiries > 0 ||
                            res.newInquiryMessages > 0 ||
                            (res.newTolerancePayments ?? 0) > 0 ||
                            res.prunedBoxes > 0;
                        if (hasInboxChanges) {
                            emitAppEvent(APP_EVENTS.POST_INBOX_UPDATED);
                            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'post' });
                        }
                        if (res.newWotHandshakes > 0 || (res.importedWotLists ?? 0) > 0) {
                            emitAppEvent(APP_EVENTS.WOT_UPDATED);
                        }
                        if (hasInboxChanges || res.polledSyncs > 0) {
                            notifyAppNoticesUpdated();
                        }
                    },
                ),
                register<FrontendMarketSyncResult | null>(
                    'humoco://market-synced',
                    (res) => {
                        notifyAppNoticesUpdated();
                        if (!res) return;
                        const retrySecs = rateLimitManager.extractRetryAfter(res);
                        if (retrySecs) rateLimitManager.setCooldown(retrySecs);
                        const hasRevokedOwnSyncs = (res.revokedOwnSyncs?.length ?? 0) > 0;
                        if (hasRevokedOwnSyncs) {
                            notifySyncStatusUpdated();
                        }
                        if (res.importedCards > 0 || (res.newAds ?? 0) > 0) {
                            emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
                        }
                        if (res.importedCards > 0) {
                            emitAppEvent(APP_EVENTS.CONTACTS_UPDATED);
                        }
                        if (res.importedWotLists > 0) {
                            emitAppEvent(APP_EVENTS.WOT_UPDATED);
                        }
                        if (
                            res.uploadedTo > 0 ||
                            res.polledSyncs > 0 ||
                            res.importedCards > 0 ||
                            res.importedWotLists > 0 ||
                            hasRevokedOwnSyncs
                        ) {
                            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'market' });
                        }
                    },
                ),
                register<unknown>('humoco://conflicts-updated', () => {
                    emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
                }),
            ]);
        };

        void setup();
        return () => {
            cancelled = true;
            for (const unlisten of unlisteners) {
                unlisten();
            }
        };
    }, []);

    useEffect(() => {
        let active = true;
        const expectedGen = sessionGenRef.current;

        const applySummary = (mp: FrontendMyMarketProfile | undefined | null) => {
            if (!mp?.profile) return;
            const fullName = [mp.profile.firstName, mp.profile.lastName]
                .map((s) => s?.trim())
                .filter(Boolean)
                .join(' ');
            setDisplayProfileName(fullName || mp.profile.organization?.trim() || '');
            setUserAvatarUrl(mp.profile.pictureUrl?.trim() || null);
        };

        const loadSummary = () => {
            if (!isAuthenticated) return;
            if (typeof marketService?.getMyMarketProfile !== 'function') return;
            Promise.resolve(marketService.getMyMarketProfile())
                .then((mp) => {
                    if (!active || sessionGenRef.current !== expectedGen || !isAuthenticatedRef.current) return;
                    applySummary(mp);
                })
                .catch(() => {});
        };

        loadSummary();

        const onProfileUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendMyMarketProfile | undefined>;
            if (custom.detail?.profile) {
                applySummary(custom.detail);
            } else {
                loadSummary();
            }
        };

        window.addEventListener(MY_PROFILE_UPDATED_EVENT, onProfileUpdated);
        return () => {
            active = false;
            window.removeEventListener(MY_PROFILE_UPDATED_EVENT, onProfileUpdated);
        };
    }, [isAuthenticated, isAuthenticatedRef, sessionGenRef, setDisplayProfileName, setUserAvatarUrl]);

    useEffect(() => {
        return tourBridge.subscribe((partial) => {
            if (partial.isAuthenticated !== undefined) {
                setIsAuthenticated(partial.isAuthenticated);
                isAuthenticatedRef.current = partial.isAuthenticated;
            }
            if (partial.hasCachedSession !== undefined) {
                setHasCachedSession(partial.hasCachedSession);
                hasCachedSessionRef.current = partial.hasCachedSession;
            }
            if (partial.isForkLocked !== undefined) {
                setIsForkLocked(partial.isForkLocked);
                setLockdownKind(partial.isForkLocked ? 'fork' : null);
            }
            if (partial.isRecoveryRequired !== undefined) {
                setIsRecoveryRequired(partial.isRecoveryRequired);
            }
            if (partial.profileName !== undefined) {
                setProfileName(partial.profileName);
            }
        });
    }, [
        hasCachedSessionRef,
        isAuthenticatedRef,
        setHasCachedSession,
        setIsAuthenticated,
        setIsForkLocked,
        setIsRecoveryRequired,
        setLockdownKind,
        setProfileName,
    ]);

    return { syncNetworkStatusNow };
}
