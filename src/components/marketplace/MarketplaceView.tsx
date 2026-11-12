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
} from 'lucide-react';
import {
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
import { settingsService } from '../../services/settingsService';
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
import { SYNC_STATUS_UPDATED_EVENT } from '../../utils/syncEvents';
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
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    // Selected person DID for detail view
    const didFromNav = appState.view === 'marketplace' ? appState.selectedDid : undefined;
    const [selectedPersonDid, setSelectedPersonDid] = useState<string | null>(
        initialSelectedDid || didFromNav || null
    );

    // Modals
    const [isCreateAdOpen, setIsCreateAdOpen] = useState(false);
    const [editingAd, setEditingAd] = useState<FrontendMyAd | null>(null);
    const [renewingAd, setRenewingAd] = useState<FrontendMyAd | null>(null);
    const [isProfileEditorOpen, setIsProfileEditorOpen] = useState(initialOpenEditor);
    const [isShareModalOpen, setIsShareModalOpen] = useState(false);
    const [isImportModalOpen, setIsImportModalOpen] = useState(false);
    const [isPublishPromptOpen, setIsPublishPromptOpen] = useState(false);
    const [isOptionsOpen, setIsOptionsOpen] = useState(false);
    const [maxDistanceKm, setMaxDistanceKm] = useState<number | null>(null);

    // Sync status count
    const [connectedSyncCount, setConnectedSyncCount] = useState<number | null>(null);

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
            const settings = await settingsService.getSettings();
            const count = (settings.marketSelectedSyncs || []).length;
            setConnectedSyncCount(count);
        } catch (err) {
            logger.debug('Failed to check sync settings in marketplace: ' + stringifyError(err));
            setConnectedSyncCount(0);
        }
    }, []);

    const refreshInbox = useCallback(async () => {
        try {
            const inbox = await postService.getPostInboxSummary();
            setInboxSummary(inbox);
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

        window.addEventListener(POST_INBOX_UPDATED_EVENT, handleInboxUpdate);
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);

        return () => {
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, handleInboxUpdate);
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdate);
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
        try {
            const updated = await protectAction(async (pwd) => {
                return await marketService.saveMyMarketProfile(req, pwd || undefined);
            });
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
        try {
            const req: FrontendSaveMyMarketProfileRequest = {
                profile: currentProfile.profile,
                visibility: currentProfile.visibility,
                skills: currentProfile.skills,
                maxHops: patch.maxHops ?? currentProfile.maxHops,
                validityYears: patch.validityYears ?? currentProfile.validityYears ?? 2,
                allowInquiries: patch.allowInquiries ?? currentProfile.allowInquiries,
            };
            const updated = await protectAction(async (pwd) => {
                return await marketService.saveMyMarketProfile(req, pwd || undefined);
            });
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

    const handleReplyInquiry = async (
        inquiryId: string,
        decision: 'confirm' | 'decline',
        note?: string,
        paymentUri?: string
    ) => {
        try {
            await protectAction(async (pwd) => {
                await postService.replyMarketInquiryViaPost(
                    inquiryId,
                    decision,
                    note,
                    paymentUri,
                    undefined,
                    undefined,
                    pwd || undefined
                );
            });
            await refreshInbox();
        } catch (err) {
            logger.error('Failed to reply to inquiry: ' + stringifyError(err));
            setError(translateError(err, t));
        }
    };

    const handlePayInquiry = (_paymentUri: string) => {
        navigate({ view: 'send_vouchers' });
    };

    const handleBoardInquire = (_ad: Ad, ownerDid: string, _ownerName: string) => {
        setSelectedPersonDid(ownerDid);
    };

    const handleBoardPay = (ownerDid: string) => {
        navigate({ view: 'send_vouchers', initialRecipientId: ownerDid });
    };

    // If person detail view is active
    const selectedPerson = useMemo(() => {
        if (!selectedPersonDid || !snapshot) return null;
        return snapshot.people.find((p) => p.did === selectedPersonDid) || null;
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
                person={selectedPerson}
                onBack={() => setSelectedPersonDid(null)}
            />
        );
    }

    const defaultEmptyInbox: FrontendPostInboxSummary = {
        pendingSignRequests: [],
        pendingWotHandshakes: [],
        sellerInquiries: [],
        buyerInquiries: [],
        unacknowledgedPayments: [],
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
            street: 'everyone',
            email: 'everyone',
            phone: 'everyone',
            coordinates: 'everyone',
            gender: 'everyone',
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
                            onClick={() => setIsProfileEditorOpen(true)}
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

                {/* Segmented Control Tabs */}
                <div className="flex items-center justify-center sm:justify-start">
                    <div className="inline-flex p-1 bg-theme-subtle/40 rounded-2xl border border-theme-subtle/60 shadow-xs">
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
                            {activeMyAdsCount > 0 && (
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
                            hasSyncService={connectedSyncCount !== null && connectedSyncCount > 0}
                            onCreateAd={handleCreateAdClick}
                            onImportCard={() => setIsImportModalOpen(true)}
                            onNavigateToSyncSettings={() => navigateToSettingsSection('sync')}
                            onInquire={handleBoardInquire}
                            onPay={handleBoardPay}
                            onOwnerClick={(ownerDid, isOwn) => {
                                if (isOwn || (currentProfile?.profile?.id && ownerDid === currentProfile.profile.id)) {
                                    setActiveTab('mine');
                                } else {
                                    setSelectedPersonDid(ownerDid);
                                }
                            }}
                            onEditOwnAd={(ad) => {
                                const found = currentProfile?.ads.find((a) => a.id === ad.id);
                                if (found) {
                                    setEditingAd(found);
                                } else {
                                    setEditingAd({
                                        id: ad.id,
                                        kind: ad.kind,
                                        title: ad.title,
                                        text: ad.text,
                                        pricing: ad.pricing,
                                        publishedAt: ad.publishedAt,
                                        renewedAt: ad.renewedAt,
                                        expiresAt: ad.expiresAt,
                                        allowInquiries: ad.allowInquiries,
                                        isRemote: ad.isRemote,
                                        status: 'active',
                                        daysLeft: Math.max(0, Math.ceil((ad.expiresAt - Math.floor(Date.now() / 1000)) / 86400)),
                                        imageToken: ad.imageToken,
                                    });
                                }
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
                            onSelectPerson={(person) => setSelectedPersonDid(person.did)}
                        />
                    )}

                    {activeTab === 'mine' && (
                        <MineTab
                            myProfile={currentProfile}
                            inbox={inboxSummary || defaultEmptyInbox}
                            onOpenProfileEditor={() => setIsProfileEditorOpen(true)}
                            onOpenShareCard={() => setIsShareModalOpen(true)}
                            onTogglePublished={handleTogglePublished}
                            onCreateAd={handleCreateAdClick}
                            onEditAd={(ad) => {
                                setEditingAd(ad);
                                setIsCreateAdOpen(true);
                            }}
                            onRenewAd={(ad) => setRenewingAd(ad)}
                            onDeleteAd={handleDeleteAd}
                            onReplyInquiry={handleReplyInquiry}
                            onPayInquiry={handlePayInquiry}
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
            <ProfileCardEditor
                isOpen={isProfileEditorOpen}
                onClose={() => setIsProfileEditorOpen(false)}
                myProfile={currentProfile}
                onSave={handleSaveProfile}
            />

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
        </PageLayout>
    );
}

export default MarketplaceView;
