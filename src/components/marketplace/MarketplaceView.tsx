// src/components/marketplace/MarketplaceView.tsx
import { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Plus,
    Edit3,
    Users,
    User,
    LayoutGrid,
    SlidersHorizontal,
    ChevronUp,
    ChevronDown,
    RefreshCw,
} from 'lucide-react';
import {
    Contact,
    FrontendMarketSnapshot,
    FrontendMyMarketProfile,
    FrontendMyAd,
    FrontendAdInput,
    FrontendSaveMyMarketProfileRequest,
    FrontendPostInboxSummary,
    Ad,
} from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { marketService } from '../../services/marketService';
import { postService, POST_INBOX_UPDATED_EVENT } from '../../services/postService';
import { APP_EVENTS } from '../../utils/appEvents';
import { settingsService } from '../../services/settingsService';
import { contactService } from '../../services/contactService';
import ContactDialog from '../contacts/ContactDialog';
import { PageLayout } from '../ui/PageLayout';
import { Button } from '../ui/Button';
import { PageLoadingState } from '../ui/PageState';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { BoardTab } from './BoardTab';
import { PeopleTab } from './PeopleTab';
import { MineTab } from './MineTab';
import { MarketPersonView } from './MarketPersonView';
import { AdEditorModal } from './AdEditorModal';
import { RenewAdModal } from './RenewAdModal';
import { ProfileCardEditor } from './ProfileCardEditor';
import { ShareCardModal } from './ShareCardModal';
import { ImportCardModal } from './ImportCardModal';
import { MarketplaceOptionsPanel } from './MarketplaceOptionsPanel';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { isExactSameUserId, isSameRootIdentity } from '../../utils/userIdHelper';
import {
    SYNC_STATUS_UPDATED_EVENT,
    MY_PROFILE_UPDATED_EVENT,
    APP_NOTICES_UPDATED_EVENT,
    showTransientSyncToast,
} from '../../utils/syncEvents';
import { formatLastSyncLabel } from '../../utils/appNotices';
import { logger } from '../../utils/log';

export interface MarketplaceViewProps {
    onBack?: () => void;
    initialSelectedDid?: string;
    initialTab?: 'board' | 'people' | 'mine';
    initialOpenEditor?: boolean;
}

export type MarketplaceTab = 'board' | 'people' | 'mine';

export function MarketplaceView({
    onBack,
    initialSelectedDid,
    initialTab,
    initialOpenEditor = false,
}: MarketplaceViewProps) {
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
    const [addContactTarget, setAddContactTarget] = useState<{ did: string; suggestedName?: string | null } | null>(null);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [isSyncing, setIsSyncing] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);

    // Selected person DID for detail view
    const didFromNav = appState.view === 'marketplace' ? appState.selectedDid : undefined;
    const [selectedPersonDid, setSelectedPersonDid] = useState<string | null>(
        initialSelectedDid || didFromNav || null
    );

    // Modals
    const [isCreateAdOpen, setIsCreateAdOpen] = useState(false);
    const [editingAd, setEditingAd] = useState<Ad | FrontendMyAd | null>(null);
    const [inquiryTargetAd, setInquiryTargetAd] = useState<Ad | null>(null);
    const [renewingAd, setRenewingAd] = useState<FrontendMyAd | null>(null);
    const [isProfileEditorOpen, setIsProfileEditorOpen] = useState(initialOpenEditor);
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [isImportModalOpen, setIsImportModalOpen] = useState(false);
    const [isPublishPromptOpen, setIsPublishPromptOpen] = useState(false);
    const [isOptionsOpen, setIsOptionsOpen] = useState(false);
    const [maxDistanceKm, setMaxDistanceKm] = useState<number | null>(null);

    // Sync status & reader image settings
    const [connectedSyncCount, setConnectedSyncCount] = useState<number | null>(null);
    const [mediaAutoLoadMaxHops, setMediaAutoLoadMaxHops] = useState<number>(2);
    const [marketPollWifiOnly, setMarketPollWifiOnly] = useState<boolean>(false);

    // React to navigation state changes
    useEffect(() => {
        if (appState.view === 'marketplace') {
            if (appState.initialTab) {
                setActiveTab(appState.initialTab);
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
        try {
            const [config, overview] = await Promise.all([
                settingsService.getConfig(),
                typeof marketService?.getSyncOverviewStatus === 'function'
                    ? marketService.getSyncOverviewStatus().catch(() => null)
                    : Promise.resolve(null),
            ]);
            const count = overview
                ? overview.connectedCount
                : (config.securityPolicy.marketSelectedSyncs || []).length;
            setConnectedSyncCount(count);
            setMediaAutoLoadMaxHops(config.uiPreferences.mediaAutoLoadMaxHops ?? 2);
            setMarketPollWifiOnly(Boolean(config.uiPreferences.marketPollWifiOnly));
        } catch (err) {
            logger.debug('Failed to check sync settings in marketplace: ' + stringifyError(err));
            setConnectedSyncCount(0);
        }
    }, []);

    const refreshInbox = useCallback(async () => {
        try {
            const inbox = await postService.getPostInboxSummary();
            setInboxSummary(inbox);
            if (typeof contactService?.getContacts === 'function') {
                contactService.getContacts().then((list) => setContacts(list || [])).catch(() => {});
            }
            if (typeof marketService?.getAppNotices === 'function') {
                marketService.getAppNotices().then((res) => setLastSyncAt(res?.lastSyncAt ?? null)).catch(() => {});
            }
        } catch (err) {
            logger.debug('Failed to load post inbox in marketplace: ' + stringifyError(err));
        }
    }, []);

    const refreshData = useCallback(async (isInitial = false) => {
        if (isInitial) setIsLoading(true);
        setError(null);
        try {
            const [snap, profile] = await Promise.all([
                marketService.getMarketView(),
                marketService.getMyMarketProfile(),
            ]);
            setSnapshot(snap);
            setMyProfile(profile);
            await Promise.all([refreshInbox(), refreshSyncStatus()]);
        } catch (err) {
            logger.error('Failed to load market snapshot: ' + stringifyError(err));
            setError(translateError(err, t));
        } finally {
            if (isInitial) setIsLoading(false);
        }
    }, [refreshInbox, refreshSyncStatus, t]);

    useEffect(() => {
        refreshData(true);
    }, [refreshData]);

    // Listen to background updates
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
            if (typeof marketService?.getAppNotices === 'function') {
                marketService.getAppNotices().then((res) => setLastSyncAt(res?.lastSyncAt ?? null)).catch(() => {});
            }
        };
        const handleMarketCardUpdate = () => {
            refreshData(false);
        };

        window.addEventListener(POST_INBOX_UPDATED_EVENT, handleInboxUpdate);
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);
        window.addEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdate);
        window.addEventListener(APP_NOTICES_UPDATED_EVENT, handleNoticesUpdate);
        window.addEventListener(APP_EVENTS.MARKET_CARD_UPDATED, handleMarketCardUpdate);

        return () => {
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, handleInboxUpdate);
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);
            window.removeEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdate);
            window.removeEventListener(APP_NOTICES_UPDATED_EVENT, handleNoticesUpdate);
            window.removeEventListener(APP_EVENTS.MARKET_CARD_UPDATED, handleMarketCardUpdate);
        };
    }, [refreshInbox, refreshSyncStatus, refreshData]);

    // Handle "Inserat aufgeben" click
    const handleCreateAdClick = useCallback(() => {
        if (!myProfile) return;
        if (!myProfile.published) {
            setIsPublishPromptOpen(true);
        } else {
            setEditingAd(null);
            setIsCreateAdOpen(true);
        }
    }, [myProfile]);

    const handleConfirmPublishAndCreateAd = async () => {
        try {
            const updated = await protectAction(async (pwd) => {
                return await marketService.setCardPublished(true, false, pwd || undefined);
            });
            if (updated) {
                setMyProfile(updated);
                setIsPublishPromptOpen(false);
                setEditingAd(null);
                setIsCreateAdOpen(true);
            }
        } catch (err) {
            logger.error('Failed to publish business card: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handleTogglePublished = async (published: boolean) => {
        try {
            const updated = await protectAction(async (pwd) => {
                return await marketService.setCardPublished(published, false, pwd || undefined);
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

    const handleUpdateMyProfileOptions = async (patch: { maxHops?: number; validityYears?: number; allowInquiries?: boolean }) => {
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
        options: { text?: string; decision?: 'confirm' | 'decline' }
    ) => {
        try {
            await protectAction(async (pwd) => {
                await postService.sendMarketInquiryMessageViaPost(
                    inquiryId,
                    {
                        text: options.text,
                        decision: options.decision,
                    },
                    pwd || undefined
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
                            })
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
                (postRes?.newSignRequests ?? 0) > 0 ||
                (postRes?.importedSignatures ?? 0) > 0;

            await refreshData(false);
            if (syncErrors.length > 0) {
                setError(syncErrors.join(' · '));
            } else if (mktRes?.status === 'skippedWifiOnly') {
                showTransientSyncToast({
                    message: t('marketplace.syncSkippedWifi', 'Synchronisation übersprungen (nur im WLAN aktiv)'),
                    variant: 'info',
                });
            } else if (mktRes?.status === 'skippedCooldown') {
                showTransientSyncToast({
                    message: t('marketplace.syncSkippedCooldown', 'Synchronisation kürzlich durchgeführt – bitte später erneut prüfen.'),
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

    // If person detail view is active
    const selectedPerson = useMemo(() => {
        if (!selectedPersonDid || !snapshot) return null;
        return (
            snapshot.people.find((p) => isExactSameUserId(p.recipientDid || p.did, selectedPersonDid)) ||
            snapshot.people.find((p) => isExactSameUserId(p.did, selectedPersonDid)) ||
            (!selectedPersonDid.includes('@')
                ? snapshot.people.find((p) => isSameRootIdentity(p.did, selectedPersonDid))
                : undefined) ||
            null
        );
    }, [selectedPersonDid, snapshot]);

    if (isLoading) {
        return (
            <PageLayout
                title={t('marketplace.title', 'Marktplatz')}
                onBack={handleBack}
                description={t('marketplace.subtitle', 'Angebote & Gesuche im Netzwerk')}
            >
                <PageLoadingState message={t('common.loading', 'Laden...')} />
            </PageLayout>
        );
    }

    if (selectedPerson) {
        return (
            <MarketPersonView
                key={selectedPerson.recipientDid || selectedPerson.did}
                person={selectedPerson}
                initialInquiryAd={inquiryTargetAd}
                mediaAutoLoadMaxHops={mediaAutoLoadMaxHops}
                wifiOnly={marketPollWifiOnly}
                onBack={() => {
                    setSelectedPersonDid(null);
                    setInquiryTargetAd(null);
                }}
            />
        );
    }

    const defaultEmptyInbox: FrontendPostInboxSummary = {
        pendingSignRequests: [],
        pendingWotHandshakes: [],
        inquiries: [],
        unacknowledgedPayments: [],
        unsupportedMessages: [],
    };

    const defaultEmptyProfile: FrontendMyMarketProfile = {
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

    const currentProfile = myProfile || defaultEmptyProfile;

    const activeMyAdsCount = currentProfile.ads.filter((a) => a.status === 'active').length;
    const unreadInquiriesCount = (inboxSummary?.inquiries || []).filter((i) => i.unread).length;

    return (
        <PageLayout
            title={t('marketplace.title', 'Marktplatz')}
            onBack={handleBack}
            description={t('marketplace.subtitle', 'Angebote & Gesuche im Netzwerk')}
            optionsPanel={
                isOptionsOpen ? (
                    <MarketplaceOptionsPanel
                        myProfile={currentProfile}
                        onUpdateMyProfile={handleUpdateMyProfileOptions}
                        storedCardCount={snapshot?.storedCardCount}
                        onSettingsChanged={() => refreshData(false)}
                        onClose={() => setIsOptionsOpen(false)}
                        hasSyncService={(connectedSyncCount ?? 0) > 0}
                        onNavigateToSyncSettings={() => navigateToSettingsSection?.('sync')}
                    />
                ) : null
            }
            actions={
                <div className="flex items-center gap-2">
                    <Button
                        size="sm"
                        variant="outline"
                        onClick={handleManualSync}
                        disabled={isSyncing}
                        data-testid="refresh-marketplace-button"
                        title={t('marketplace.refresh')}
                        aria-label={t('marketplace.refresh')}
                        className="flex items-center gap-1.5"
                    >
                        <RefreshCw size={15} className={isSyncing ? 'animate-spin' : ''} />
                        <span className="hidden sm:inline text-xs font-bold">{t('marketplace.refresh')}</span>
                    </Button>
                    <Button
                        size="sm"
                        variant={isOptionsOpen ? 'primary' : 'outline'}
                        onClick={() => setIsOptionsOpen((prev) => !prev)}
                        className="flex items-center gap-1.5"
                        data-testid="toggle-marketplace-options-button"
                        title={t('marketplace.optionsTitle', 'Marktplatz-Optionen & Reichweite')}
                        aria-label={t('marketplace.optionsTitle', 'Marktplatz-Optionen & Reichweite')}
                    >
                        <SlidersHorizontal size={15} />
                        <span>{t('marketplace.optionsBtn', 'Optionen')}</span>
                        {isOptionsOpen ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </Button>
                    {activeTab === 'mine' && (
                        <Button
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                if (myProfile) setIsProfileEditorOpen(true);
                            }}
                            disabled={!myProfile}
                            className="flex items-center gap-1.5"
                        >
                            <Edit3 size={15} />
                            <span>{t('marketplace.editProfileCardBtn', 'Visitenkarte bearbeiten')}</span>
                        </Button>
                    )}
                    <Button
                        size="sm"
                        onClick={handleCreateAdClick}
                        className="flex items-center gap-1.5 shadow-sm"
                    >
                        <Plus size={15} />
                        <span>{t('marketplace.createAdBtn', 'Inserat aufgeben')}</span>
                    </Button>
                </div>
            }
        >
            <div className="space-y-6">
                {/* Error Banner */}
                {error && (
                    <div className="p-3.5 rounded-2xl bg-rose-50 border border-rose-200 text-rose-800 text-xs flex items-center justify-between gap-3">
                        <span>{error}</span>
                        <Button
                            size="xs"
                            variant="outline"
                            onClick={() => setError(null)}
                            className="text-rose-700 hover:text-rose-900 border-rose-300"
                        >
                            {t('common.dismiss', 'Ausblenden')}
                        </Button>
                    </div>
                )}

                {/* Segmented Control Tabs & Last Updated */}
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                    <div className="inline-flex p-1 bg-theme-subtle/40 rounded-2xl border border-theme-subtle/60 shadow-xs self-center sm:self-auto">
                        <button
                            type="button"
                            onClick={() => setActiveTab('board')}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                activeTab === 'board'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <LayoutGrid size={15} />
                            <span>{t('marketplace.tabBoard', 'Pinnwand')}</span>
                            {snapshot && snapshot.board.length > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        activeTab === 'board'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {snapshot.board.length}
                                </span>
                            )}
                        </button>

                        <button
                            type="button"
                            onClick={() => setActiveTab('people')}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                activeTab === 'people'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <Users size={15} />
                            <span>{t('marketplace.tabPeople', 'Menschen')}</span>
                            {snapshot && snapshot.people.length > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        activeTab === 'people'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {snapshot.people.length}
                                </span>
                            )}
                        </button>

                        <button
                            type="button"
                            onClick={() => setActiveTab('mine')}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                activeTab === 'mine'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <User size={15} />
                            <span>{t('marketplace.tabMine', 'Meins')}</span>
                            {unreadInquiriesCount > 0 && (
                                <span
                                    data-testid="marketplace-unread-inquiries-badge"
                                    className="px-1.5 py-0.2 rounded-full text-[10px] bg-rose-600 text-white font-bold"
                                >
                                    {unreadInquiriesCount}
                                </span>
                            )}
                            {unreadInquiriesCount === 0 && activeMyAdsCount > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        activeTab === 'mine'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {activeMyAdsCount}
                                </span>
                            )}
                        </button>
                    </div>
                    {lastSyncAt && (
                        <span className="text-[11px] text-theme-light self-center sm:self-auto" data-testid="marketplace-last-sync">
                            {t('common.lastUpdatedPrefix', 'Zuletzt aktualisiert:')} {formatLastSyncLabel(lastSyncAt, t)}
                        </span>
                    )}
                </div>

                {/* Tab content */}
                <div>
                    {activeTab === 'board' && (
                        <BoardTab
                            items={snapshot?.board || []}
                            people={snapshot?.people || []}
                            myCoordinates={currentProfile.profile.coordinates}
                            maxDistanceKm={maxDistanceKm}
                            onMaxDistanceChange={setMaxDistanceKm}
                            mediaAutoLoadMaxHops={mediaAutoLoadMaxHops}
                            wifiOnly={marketPollWifiOnly}
                            hasSyncService={connectedSyncCount !== null && connectedSyncCount > 0}
                            onCreateAd={handleCreateAdClick}
                            onImportCard={() => setIsImportModalOpen(true)}
                            onNavigateToSyncSettings={() => navigateToSettingsSection('sync')}
                            onInquire={handleBoardInquire}
                            onPay={handleBoardPay}
                            onOwnerClick={(ownerDid, isOwn) => {
                                if (isOwn || (currentProfile?.profile?.id && isExactSameUserId(ownerDid, currentProfile.profile.id))) {
                                    setActiveTab('mine');
                                } else {
                                    setSelectedPersonDid(ownerDid);
                                }
                            }}
                            onEditOwnAd={(ad) => {
                                const found = currentProfile?.ads.find((a) => a.id === ad.id);
                                setEditingAd(found || ad);
                                setIsCreateAdOpen(true);
                            }}
                        />
                    )}

                    {activeTab === 'people' && (
                        <PeopleTab
                            people={snapshot?.people || []}
                            myCoordinates={currentProfile.profile.coordinates}
                            maxDistanceKm={maxDistanceKm}
                            onMaxDistanceChange={setMaxDistanceKm}
                            onSelectPerson={(person) => setSelectedPersonDid(person.recipientDid || person.did)}
                        />
                    )}

                    {activeTab === 'mine' && (
                        <MineTab
                            myProfile={currentProfile}
                            inbox={inboxSummary || defaultEmptyInbox}
                            contacts={contacts}
                            onOpenProfileEditor={() => {
                                if (myProfile) setIsProfileEditorOpen(true);
                            }}
                            onOpenShareCard={() => setIsShareModalOpen(true)}
                            onTogglePublished={handleTogglePublished}
                            onCreateAd={handleCreateAdClick}
                            onEditAd={(ad) => {
                                setEditingAd(ad);
                                setIsCreateAdOpen(true);
                            }}
                            onRenewAd={(ad) => setRenewingAd(ad)}
                            onQuickRenewAd={(ad) => handleRenewAd(ad.id, 90)}
                            onDeleteAd={handleDeleteAd}
                            onSendInquiryMessage={handleSendInquiryMessage}
                            onMarkInquiryRead={handleMarkInquiryRead}
                            onPayInquiry={handlePayInquiry}
                            onDismissInquiry={handleDismissInquiry}
                            onAddContact={(did, suggestedName) => setAddContactTarget({ did, suggestedName })}
                            onViewCard={(did) => {
                                const match =
                                    snapshot?.people.find((p) => isExactSameUserId(p.recipientDid || p.did, did)) ||
                                    snapshot?.people.find((p) => isExactSameUserId(p.did, did));
                                if (match) {
                                    setSelectedPersonDid(match.recipientDid || match.did);
                                }
                            }}
                        />
                    )}
                </div>
            </div>

            {/* Modals */}

            {/* Ad Editor Modal */}
            <AdEditorModal
                isOpen={isCreateAdOpen || !!editingAd}
                onClose={() => {
                    setIsCreateAdOpen(false);
                    setEditingAd(null);
                }}
                onSave={handleSaveAd}
                editingAd={editingAd}
                activeAdsCount={activeMyAdsCount}
                allowInquiries={currentProfile.allowInquiries}
                ownerDid={currentProfile?.profile?.id}
                mediaMaxBytes={currentProfile?.mediaMaxBytes}
                mediaMaxCount={currentProfile?.mediaMaxCount}
                mediaUsedCount={currentProfile?.mediaUsedCount}
            />

            {/* Renew Ad Modal */}
            <RenewAdModal
                isOpen={!!renewingAd}
                onClose={() => setRenewingAd(null)}
                ad={renewingAd}
                onRenew={handleRenewAd}
            />

            {/* Profile & Business Card Editor */}
            {myProfile && (
                <ProfileCardEditor
                    isOpen={isProfileEditorOpen}
                    onClose={() => setIsProfileEditorOpen(false)}
                    myProfile={myProfile}
                    onSave={handleSaveProfile}
                />
            )}

            {/* Share Card Modal */}
            <ShareCardModal
                isOpen={isShareModalOpen}
                onClose={() => setIsShareModalOpen(false)}
                title={t('marketplace.shareOwnCardTitle', 'Meine Visitenkarte teilen')}
                subtitle={t('marketplace.shareOwnCardSubtitle', 'Sichere Datei oder QR-Code zum Austausch')}
                fileWire={currentProfile.fileWire}
                shareUri={currentProfile.shareUri}
                qrSvg={currentProfile.qrSvg}
                did={currentProfile.profile.id}
                qrIsPartial={currentProfile.qrIsPartial}
            />

            {/* Import Card Modal */}
            <ImportCardModal
                isOpen={isImportModalOpen}
                onClose={() => setIsImportModalOpen(false)}
                onImportSuccess={(did) => {
                    refreshData(false);
                    if (did) {
                        setSelectedPersonDid(did);
                    }
                }}
            />

            {/* Prompt to publish card before creating ad */}
            <ConfirmationModal
                isOpen={isPublishPromptOpen}
                onCancel={() => setIsPublishPromptOpen(false)}
                onConfirm={handleConfirmPublishAndCreateAd}
                title={t('marketplace.publishCardBeforeAdTitle', 'Visitenkarte veröffentlichen?')}
                description={t(
                    'marketplace.publishCardBeforeAdDesc',
                    'Um ein Inserat aufzugeben, muss deine Visitenkarte veröffentlicht sein, damit Interessenten dich erreichen können. Möchtest du sie jetzt veröffentlichen?'
                )}
                confirmText={t('marketplace.publishAndCreateAdBtn', 'Veröffentlichen & Inserat aufgeben')}
                cancelText={t('common.cancel', 'Abbrechen')}
            />

            {addContactTarget && (
                <ContactDialog
                    isOpen={!!addContactTarget}
                    onClose={() => setAddContactTarget(null)}
                    onSave={handleSaveContactFromInquiry}
                    initialDid={addContactTarget.did}
                    initialProfile={
                        addContactTarget.suggestedName
                            ? { firstName: addContactTarget.suggestedName }
                            : undefined
                    }
                />
            )}
        </PageLayout>
    );
}

export default MarketplaceView;
