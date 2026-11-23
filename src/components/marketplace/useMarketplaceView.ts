import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { postService } from '../../services/postService';
import { settingsService } from '../../services/settingsService';
import type {
    Ad,
    Contact,
    FrontendAdInput,
    FrontendMarketSnapshot,
    FrontendMyAd,
    FrontendMyMarketProfile,
    FrontendPostInboxSummary,
    FrontendSaveMyMarketProfileRequest,
} from '../../types';
import { APP_EVENTS } from '../../utils/appEvents';
import { stringifyError, translateError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import {
    APP_NOTICES_UPDATED_EVENT,
    MY_PROFILE_UPDATED_EVENT,
    showTransientSyncToast,
    SYNC_STATUS_UPDATED_EVENT,
} from '../../utils/syncEvents';
import { isExactSameUserId, isSameRootIdentity } from '../../utils/userIdHelper';

export type MarketplaceTab = 'board' | 'people' | 'mine';

const DEFAULT_EMPTY_INBOX: FrontendPostInboxSummary = {
    pendingSignRequests: [],
    pendingWotHandshakes: [],
    inquiries: [],
    unacknowledgedPayments: [],
    unsupportedMessages: [],
};

const DEFAULT_EMPTY_PROFILE: FrontendMyMarketProfile = {
    profile: { id: '' },
    visibility: {
        name: 'everyone',
        organization: 'everyone',
        community: 'everyone',
        city: 'everyone',
        url: 'everyone',
        picture: 'everyone',
        street: 'contacts',
        email: 'contacts',
        phone: 'contacts',
        coordinates: 'everyone',
        gender: 'nobody',
    },
    skills: [],
    ads: [],
    maxHops: 2,
    allowInquiries: true,
    published: false,
    qrIsPartial: false,
};

interface UseMarketplaceViewParams {
    onBack?: () => void;
    initialSelectedDid?: string;
    initialTab?: MarketplaceTab;
    initialOpenEditor?: boolean;
}

export function useMarketplaceView({
    onBack,
    initialSelectedDid,
    initialTab,
    initialOpenEditor = false,
}: UseMarketplaceViewParams) {
    const { t } = useTranslation();
    const { navigate, goBack, appState, navigateToSettingsSection } = useNavigation();
    const { protectAction } = useSession();
    const handleBack = onBack || goBack;

    const [activeTab, setActiveTab] = useState<MarketplaceTab>(initialTab || 'board');
    const [snapshot, setSnapshot] = useState<FrontendMarketSnapshot | null>(null);
    const [myProfile, setMyProfile] = useState<FrontendMyMarketProfile | null>(null);
    const [inboxSummary, setInboxSummary] = useState<FrontendPostInboxSummary | null>(null);
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [lastSyncAt, setLastSyncAt] = useState<number | null>(null);
    const [addContactTarget, setAddContactTarget] = useState<{
        did: string;
        suggestedName?: string | null;
    } | null>(null);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [isSyncing, setIsSyncing] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);

    const didFromNav = appState.view === 'marketplace' ? appState.selectedDid : undefined;
    const [selectedPersonDid, setSelectedPersonDid] = useState<string | null>(
        initialSelectedDid || didFromNav || null,
    );

    const [isCreateAdOpen, setIsCreateAdOpen] = useState(false);
    const [editingAd, setEditingAd] = useState<Ad | FrontendMyAd | null>(null);
    const [inquiryTargetAd, setInquiryTargetAd] = useState<Ad | null>(null);
    const [renewingAd, setRenewingAd] = useState<FrontendMyAd | null>(null);
    const [isProfileEditorOpen, setIsProfileEditorOpen] = useState(initialOpenEditor);
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [isImportModalOpen, setIsImportModalOpen] = useState(false);
    const [isPublishPromptOpen, setIsPublishPromptOpen] = useState(false);
    const [pendingPublishAdId, setPendingPublishAdId] = useState<string | null>(null);
    const [isOptionsOpen, setIsOptionsOpen] = useState(false);
    const [maxDistanceKm, setMaxDistanceKm] = useState<number | null>(null);

    const [connectedSyncCount, setConnectedSyncCount] = useState<number | null>(null);
    const [mediaAutoLoadMaxHops, setMediaAutoLoadMaxHops] = useState<number>(2);
    const [marketPollWifiOnly, setMarketPollWifiOnly] = useState<boolean>(false);

    const mountedRef = useRef(true);
    const syncStatusSeqRef = useRef(0);
    const inboxSeqRef = useRef(0);
    const refreshSeqRef = useRef(0);
    const noticesSeqRef = useRef(0);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);

    useEffect(() => {
        if (appState.view === 'marketplace') {
            if (appState.initialTab) {
                setActiveTab(appState.initialTab);
                if (appState.initialTab === 'mine' && !appState.selectedDid) {
                    setSelectedPersonDid(null);
                    setInquiryTargetAd(null);
                }
            }
            if (appState.selectedDid) {
                setSelectedPersonDid(appState.selectedDid);
            }
            if (appState.initialOpenEditor) {
                setIsProfileEditorOpen(true);
            }
        }
    }, [appState]);

    const refreshSyncStatus = useCallback(async () => {
        const seq = ++syncStatusSeqRef.current;
        try {
            const [config, overview] = await Promise.all([
                settingsService.getConfig(),
                typeof marketService?.getSyncOverviewStatus === 'function'
                    ? marketService.getSyncOverviewStatus().catch(() => null)
                    : Promise.resolve(null),
            ]);
            if (!mountedRef.current || seq !== syncStatusSeqRef.current) return;
            const count = overview
                ? overview.connectedCount
                : (config.securityPolicy.marketSelectedSyncs || []).length;
            setConnectedSyncCount(count);
            setMediaAutoLoadMaxHops(config.uiPreferences.mediaAutoLoadMaxHops ?? 2);
            setMarketPollWifiOnly(Boolean(config.uiPreferences.marketPollWifiOnly));
        } catch (err) {
            if (!mountedRef.current || seq !== syncStatusSeqRef.current) return;
            logger.debug('Failed to check sync settings in marketplace: ' + stringifyError(err));
            setConnectedSyncCount(0);
        }
    }, []);

    const refreshInbox = useCallback(async () => {
        const seq = ++inboxSeqRef.current;
        try {
            const inbox = await postService.getPostInboxSummary();
            if (!mountedRef.current || seq !== inboxSeqRef.current) return;
            setInboxSummary(inbox);
            if (typeof contactService?.getContacts === 'function') {
                contactService
                    .getContacts()
                    .then((list) => {
                        if (mountedRef.current && seq === inboxSeqRef.current) {
                            setContacts(list || []);
                        }
                    })
                    .catch(() => {});
            }
            if (typeof marketService?.getAppNotices === 'function') {
                marketService
                    .getAppNotices()
                    .then((res) => {
                        if (mountedRef.current && seq === inboxSeqRef.current) {
                            setLastSyncAt(res?.lastSyncAt ?? null);
                        }
                    })
                    .catch(() => {});
            }
        } catch (err) {
            logger.debug('Failed to load post inbox in marketplace: ' + stringifyError(err));
        }
    }, []);

    const refreshData = useCallback(
        async (isInitial = false) => {
            const seq = ++refreshSeqRef.current;
            if (isInitial) setIsLoading(true);
            setError(null);
            try {
                const [snap, profile] = await Promise.all([
                    marketService.getMarketView(),
                    marketService.getMyMarketProfile(),
                ]);
                if (!mountedRef.current || seq !== refreshSeqRef.current) return;
                setSnapshot(snap);
                setMyProfile(profile);
                await Promise.all([refreshInbox(), refreshSyncStatus()]);
            } catch (err) {
                if (!mountedRef.current || seq !== refreshSeqRef.current) return;
                logger.error('Failed to load market snapshot: ' + stringifyError(err));
                setError(translateError(err, t));
            } finally {
                if (isInitial && mountedRef.current && seq === refreshSeqRef.current) {
                    setIsLoading(false);
                }
            }
        },
        [refreshInbox, refreshSyncStatus, t],
    );

    useEffect(() => {
        refreshData(true);
    }, [refreshData]);

    useEffect(() => {
        const handleInboxUpdate = () => {
            refreshInbox();
        };
        const handleSyncUpdate = () => {
            refreshSyncStatus();
            refreshData(false);
        };
        const handleProfileUpdate = (e: Event) => {
            const detail = (e as CustomEvent<FrontendMyMarketProfile | undefined>).detail;
            if (detail?.profile) {
                setMyProfile(detail);
            } else {
                refreshData(false);
            }
        };
        const handleNoticesUpdate = () => {
            const seq = ++noticesSeqRef.current;
            if (typeof marketService?.getAppNotices === 'function') {
                marketService
                    .getAppNotices()
                    .then((res) => {
                        if (mountedRef.current && seq === noticesSeqRef.current) {
                            setLastSyncAt(res?.lastSyncAt ?? null);
                        }
                    })
                    .catch(() => {});
            }
        };
        const handleMarketCardUpdate = () => {
            refreshData(false);
        };

        window.addEventListener(APP_EVENTS.POST_INBOX_UPDATED, handleInboxUpdate);
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);
        window.addEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdate);
        window.addEventListener(APP_NOTICES_UPDATED_EVENT, handleNoticesUpdate);
        window.addEventListener(APP_EVENTS.MARKET_CARD_UPDATED, handleMarketCardUpdate);

        return () => {
            window.removeEventListener(APP_EVENTS.POST_INBOX_UPDATED, handleInboxUpdate);
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);
            window.removeEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdate);
            window.removeEventListener(APP_NOTICES_UPDATED_EVENT, handleNoticesUpdate);
            window.removeEventListener(APP_EVENTS.MARKET_CARD_UPDATED, handleMarketCardUpdate);
        };
    }, [refreshInbox, refreshSyncStatus, refreshData]);

    const handleCreateAdClick = useCallback(() => {
        if (!myProfile) return;
        setEditingAd(null);
        setIsCreateAdOpen(true);
    }, [myProfile]);

    const handleToggleAdPublished = async (ad: FrontendMyAd, published: boolean) => {
        if (published && !myProfile?.published) {
            setPendingPublishAdId(ad.id);
            setIsPublishPromptOpen(true);
            return;
        }
        try {
            await protectAction(async (pwd) => {
                await marketService.setAdPublished(ad.id, published, pwd || undefined);
            });
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to toggle ad published status: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleConfirmPublishAndAd = async () => {
        if (!pendingPublishAdId) return;
        try {
            await protectAction(async (pwd) => {
                await marketService.setAdPublished(pendingPublishAdId, true, pwd || undefined, true);
            });
            setIsPublishPromptOpen(false);
            setPendingPublishAdId(null);
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to publish business card and ad: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleTogglePublished = async (published: boolean) => {
        try {
            const updated = await protectAction(async (pwd) => {
                return await marketService.setCardPublished(published, pwd || undefined);
            });
            if (updated) {
                setMyProfile(updated);
            }
        } catch (err) {
            logger.error('Failed to toggle published status: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleSaveAd = async (input: FrontendAdInput) => {
        try {
            await protectAction(async (pwd) => {
                await marketService.upsertAd(input, pwd || undefined);
            });
            setIsCreateAdOpen(false);
            setEditingAd(null);
            if (!input.id && !input.published) {
                setActiveTab('mine');
            }
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to save ad: ' + stringifyError(err));
            setError(translateError(err, t));
            throw err;
        }
    };

    const handleRenewAd = async (id: string, durationDays: number) => {
        try {
            await protectAction(async (pwd) => {
                await marketService.renewAd(id, durationDays, pwd || undefined);
            });
            setRenewingAd(null);
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to renew ad: ' + stringifyError(err));
            setError(translateError(err, t));
            throw err;
        }
    };

    const handleDeleteAd = async (id: string) => {
        try {
            await protectAction(async (pwd) => {
                await marketService.deleteAd(id, pwd || undefined);
            });
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to delete ad: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleSaveProfile = async (req: FrontendSaveMyMarketProfileRequest) => {
        if (!myProfile) return;
        try {
            const updated = await protectAction(
                async (pwd) => {
                    return await marketService.saveMyMarketProfile(req, pwd || undefined);
                },
                { requireFullAccess: true },
            );
            if (updated) {
                setMyProfile(updated);
            }
            setIsProfileEditorOpen(false);
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to save profile: ' + stringifyError(err));
            setError(translateError(err, t));
            throw err;
        }
    };

    const handleUpdateMyProfileOptions = async (patch: {
        maxHops?: number;
        validityYears?: number;
        allowInquiries?: boolean;
    }) => {
        if (!myProfile) return;
        try {
            const req: FrontendSaveMyMarketProfileRequest = {
                profile: myProfile.profile,
                visibility: myProfile.visibility,
                skills: myProfile.skills,
                maxHops: patch.maxHops ?? myProfile.maxHops,
                validityYears: patch.validityYears ?? myProfile.validityYears ?? 2,
                allowInquiries: patch.allowInquiries ?? myProfile.allowInquiries,
                published: myProfile.published,
            };
            const updated = await protectAction(
                async (pwd) => {
                    return await marketService.saveMyMarketProfile(req, pwd || undefined);
                },
                { requireFullAccess: true },
            );
            if (updated) {
                setMyProfile(updated);
            }
            await refreshData(false);
        } catch (err) {
            logger.error('Failed to update profile options: ' + stringifyError(err));
            setError(translateError(err, t));
            throw err;
        }
    };

    const handleSendInquiryMessage = async (
        inquiryId: string,
        options: { text?: string; decision?: 'confirm' | 'decline' },
    ) => {
        try {
            await protectAction(async (pwd) => {
                await postService.sendMarketInquiryMessageViaPost(
                    inquiryId,
                    {
                        text: options.text,
                        decision: options.decision,
                    },
                    pwd || undefined,
                );
            });
            await refreshInbox();
        } catch (err) {
            logger.error('Failed to send inquiry message: ' + stringifyError(err));
            setError(translateError(err, t));
            throw err;
        }
    };

    const handleMarkInquiryRead = async (inquiryId: string) => {
        const target = inboxSummary?.inquiries.find((i) => i.inquiryId === inquiryId);
        if (target && !target.unread) {
            return;
        }
        try {
            if (typeof postService?.markInquiryRead === 'function') {
                await protectAction(async (pwd) => {
                    await postService.markInquiryRead(inquiryId, pwd || undefined);
                });
                await refreshInbox();
            }
        } catch (err) {
            logger.debug('Failed to mark inquiry read: ' + stringifyError(err));
        }
    };

    const handleDismissInquiry = async (inquiryId: string) => {
        try {
            await protectAction(async (pwd) => {
                await postService.dismissPendingPostItem('inquiry', inquiryId, pwd || undefined);
            });
            await refreshInbox();
        } catch (err) {
            logger.error('Failed to dismiss inquiry: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleSaveContactFromInquiry = async (contact: Contact) => {
        await protectAction(async (pwd) => {
            await contactService.saveContact(contact, pwd || undefined);
        });
        setAddContactTarget(null);
        await refreshInbox();
    };

    const handlePayInquiry = (counterpartyDid: string, paymentUri?: string | null) => {
        navigate({
            view: 'send_vouchers',
            initialRecipientId: counterpartyDid,
            ...(paymentUri ? { initialPaymentUri: paymentUri } : {}),
        });
    };

    const handleManualSync = async () => {
        if (isSyncing) return;
        setIsSyncing(true);
        setError(null);
        try {
            const results = await Promise.allSettled([
                marketService.syncMarketChannels(),
                postService.syncPostChannels(),
            ]);
            const syncErrors: string[] = [];
            for (const res of results) {
                if (res.status === 'rejected') {
                    syncErrors.push(translateError(res.reason, t));
                } else if (res.value) {
                    if (res.value.retryAfterSecs && res.value.retryAfterSecs > 0) {
                        syncErrors.push(
                            t('marketplace.syncRateLimited', {
                                seconds: res.value.retryAfterSecs,
                            }),
                        );
                    }
                    if (Array.isArray(res.value.errors) && res.value.errors.length > 0) {
                        syncErrors.push(...res.value.errors);
                    }
                }
            }
            const mktRes = results[0].status === 'fulfilled' ? results[0].value : null;
            const postRes = results[1].status === 'fulfilled' ? results[1].value : null;
            const hasNewItems =
                (mktRes?.importedCards ?? 0) > 0 ||
                (mktRes?.newAds ?? 0) > 0 ||
                (postRes?.importedPayments ?? 0) > 0 ||
                (postRes?.newInquiries ?? 0) > 0 ||
                (postRes?.newInquiryMessages ?? 0) > 0 ||
                (postRes?.newWotHandshakes ?? 0) > 0 ||
                (postRes?.importedWotLists ?? 0) > 0 ||
                (postRes?.newSignRequests ?? 0) > 0 ||
                (postRes?.importedSignatures ?? 0) > 0;

            await refreshData(false);
            if (syncErrors.length > 0) {
                setError(syncErrors.join(' · '));
            } else if (mktRes?.status === 'skippedWifiOnly') {
                showTransientSyncToast({
                    message: t(
                        'marketplace.syncSkippedWifi',
                        'Synchronisation übersprungen (nur im WLAN aktiv)',
                    ),
                    variant: 'info',
                });
            } else if (mktRes?.status === 'skippedCooldown') {
                showTransientSyncToast({
                    message: t(
                        'marketplace.syncSkippedCooldown',
                        'Synchronisation kürzlich durchgeführt – bitte später erneut prüfen.',
                    ),
                    variant: 'info',
                });
            } else if (!hasNewItems && mktRes?.status !== 'sessionSuperseded') {
                showTransientSyncToast({
                    message: t('common.syncUpToDateToast', 'Alles aktuell – keine neuen Einträge.'),
                    variant: 'info',
                });
            }
        } catch (err) {
            logger.error('Failed to sync marketplace manually: ' + stringifyError(err));
            setError(translateError(err, t));
        } finally {
            setIsSyncing(false);
        }
    };

    const handleBoardInquire = (ad: Ad, ownerDid: string, _ownerName: string) => {
        setInquiryTargetAd(ad);
        setSelectedPersonDid(ownerDid);
    };

    const handleBoardPay = (ownerDid: string) => {
        navigate({ view: 'send_vouchers', initialRecipientId: ownerDid });
    };

    const selectedPerson = useMemo(() => {
        if (!selectedPersonDid || !snapshot) return null;
        return (
            snapshot.people.find((p) =>
                isExactSameUserId(p.recipientDid || p.did, selectedPersonDid),
            ) ||
            snapshot.people.find((p) => isExactSameUserId(p.did, selectedPersonDid)) ||
            (!selectedPersonDid.includes('@')
                ? snapshot.people.find((p) => isSameRootIdentity(p.did, selectedPersonDid))
                : undefined) ||
            null
        );
    }, [selectedPersonDid, snapshot]);

    const currentProfile = myProfile || DEFAULT_EMPTY_PROFILE;
    const activePublishedMyAdsCount = currentProfile.ads.filter(
        (a) => a.published && a.status === 'active',
    ).length;
    const totalMineAdsCount = currentProfile.ads.filter(
        (a) => !a.published || a.status === 'active',
    ).length;
    const unreadInquiriesCount = (inboxSummary?.inquiries || []).filter((i) => i.unread).length;

    return {
        handleBack,
        navigateToSettingsSection,
        activeTab,
        setActiveTab,
        snapshot,
        myProfile,
        currentProfile,
        defaultEmptyInbox: DEFAULT_EMPTY_INBOX,
        inboxSummary,
        contacts,
        lastSyncAt,
        addContactTarget,
        setAddContactTarget,
        isLoading,
        isSyncing,
        error,
        setError,
        selectedPerson,
        setSelectedPersonDid,
        inquiryTargetAd,
        setInquiryTargetAd,
        isCreateAdOpen,
        setIsCreateAdOpen,
        editingAd,
        setEditingAd,
        renewingAd,
        setRenewingAd,
        isProfileEditorOpen,
        setIsProfileEditorOpen,
        isShareModalOpen,
        setIsShareModalOpen,
        isImportModalOpen,
        setIsImportModalOpen,
        isPublishPromptOpen,
        setIsPublishPromptOpen,
        setPendingPublishAdId,
        isOptionsOpen,
        setIsOptionsOpen,
        maxDistanceKm,
        setMaxDistanceKm,
        connectedSyncCount,
        mediaAutoLoadMaxHops,
        marketPollWifiOnly,
        activePublishedMyAdsCount,
        totalMineAdsCount,
        unreadInquiriesCount,
        refreshData,
        handleCreateAdClick,
        handleToggleAdPublished,
        handleConfirmPublishAndAd,
        handleTogglePublished,
        handleSaveAd,
        handleRenewAd,
        handleDeleteAd,
        handleSaveProfile,
        handleUpdateMyProfileOptions,
        handleSendInquiryMessage,
        handleMarkInquiryRead,
        handleDismissInquiry,
        handleSaveContactFromInquiry,
        handlePayInquiry,
        handleManualSync,
        handleBoardInquire,
        handleBoardPay,
    };
}
