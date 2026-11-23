// src/components/marketplace/MarketplaceView.tsx
import { useTranslation } from 'react-i18next';
import {
    ChevronDown,
    ChevronUp,
    Edit3,
    LayoutGrid,
    Plus,
    RefreshCw,
    SlidersHorizontal,
    User,
    Users,
} from 'lucide-react';
import { formatLastSyncLabel } from '../../utils/appNotices';
import { isExactSameUserId } from '../../utils/userIdHelper';
import { Button } from '../ui/Button';
import { PageLayout } from '../ui/PageLayout';
import { PageLoadingState } from '../ui/PageState';
import { BoardTab } from './BoardTab';
import { MarketplaceModals } from './MarketplaceModals';
import { MarketplaceOptionsPanel } from './MarketplaceOptionsPanel';
import { MarketPersonView } from './MarketPersonView';
import { MineTab } from './MineTab';
import { PeopleTab } from './PeopleTab';
import { useMarketplaceView, type MarketplaceTab } from './useMarketplaceView';

export type { MarketplaceTab } from './useMarketplaceView';

export interface MarketplaceViewProps {
    onBack?: () => void;
    initialSelectedDid?: string;
    initialTab?: MarketplaceTab;
    initialOpenEditor?: boolean;
}

export function MarketplaceView({
    onBack,
    initialSelectedDid,
    initialTab,
    initialOpenEditor = false,
}: MarketplaceViewProps) {
    const { t } = useTranslation();
    const view = useMarketplaceView({
        onBack,
        initialSelectedDid,
        initialTab,
        initialOpenEditor,
    });

    if (view.isLoading) {
        return (
            <PageLayout
                title={t('marketplace.title', 'Marktplatz')}
                onBack={view.handleBack}
                description={t('marketplace.subtitle', 'Angebote & Gesuche im Netzwerk')}
            >
                <PageLoadingState message={t('common.loading', 'Laden...')} />
            </PageLayout>
        );
    }

    if (view.selectedPerson) {
        return (
            <MarketPersonView
                key={view.selectedPerson.recipientDid || view.selectedPerson.did}
                person={view.selectedPerson}
                initialInquiryAd={view.inquiryTargetAd}
                mediaAutoLoadMaxHops={view.mediaAutoLoadMaxHops}
                wifiOnly={view.marketPollWifiOnly}
                onBack={() => {
                    view.setSelectedPersonDid(null);
                    view.setInquiryTargetAd(null);
                }}
            />
        );
    }

    return (
        <PageLayout
            title={t('marketplace.title', 'Marktplatz')}
            onBack={view.handleBack}
            description={t('marketplace.subtitle', 'Angebote & Gesuche im Netzwerk')}
            optionsPanel={
                view.isOptionsOpen ? (
                    <MarketplaceOptionsPanel
                        myProfile={view.currentProfile}
                        onUpdateMyProfile={view.handleUpdateMyProfileOptions}
                        storedCardCount={view.snapshot?.storedCardCount}
                        onSettingsChanged={() => view.refreshData(false)}
                        onClose={() => view.setIsOptionsOpen(false)}
                        hasSyncService={(view.connectedSyncCount ?? 0) > 0}
                        onNavigateToSyncSettings={() => view.navigateToSettingsSection?.('sync')}
                    />
                ) : null
            }
            actions={
                <div className="flex items-center gap-2">
                    <Button
                        size="sm"
                        variant="outline"
                        onClick={view.handleManualSync}
                        disabled={view.isSyncing}
                        data-testid="refresh-marketplace-button"
                        title={t('marketplace.refresh')}
                        aria-label={t('marketplace.refresh')}
                        className="flex items-center gap-1.5"
                    >
                        <RefreshCw size={15} className={view.isSyncing ? 'animate-spin' : ''} />
                        <span className="hidden sm:inline text-xs font-bold">
                            {t('marketplace.refresh')}
                        </span>
                    </Button>
                    <Button
                        size="sm"
                        variant={view.isOptionsOpen ? 'primary' : 'outline'}
                        onClick={() => view.setIsOptionsOpen((prev) => !prev)}
                        className="flex items-center gap-1.5"
                        data-testid="toggle-marketplace-options-button"
                        title={t('marketplace.optionsTitle', 'Marktplatz-Optionen & Reichweite')}
                        aria-label={t('marketplace.optionsTitle', 'Marktplatz-Optionen & Reichweite')}
                    >
                        <SlidersHorizontal size={15} />
                        <span>{t('marketplace.optionsBtn', 'Optionen')}</span>
                        {view.isOptionsOpen ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </Button>
                    {view.activeTab === 'mine' && (
                        <Button
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                if (view.myProfile) view.setIsProfileEditorOpen(true);
                            }}
                            disabled={!view.myProfile}
                            className="flex items-center gap-1.5"
                        >
                            <Edit3 size={15} />
                            <span>
                                {t('marketplace.editProfileCardBtn', 'Visitenkarte bearbeiten')}
                            </span>
                        </Button>
                    )}
                    <Button
                        size="sm"
                        onClick={view.handleCreateAdClick}
                        className="flex items-center gap-1.5 shadow-sm"
                    >
                        <Plus size={15} />
                        <span>{t('marketplace.createAdBtn', 'Inserat aufgeben')}</span>
                    </Button>
                </div>
            }
        >
            <div className="space-y-6">
                {view.error && (
                    <div className="p-3.5 rounded-2xl bg-rose-50 border border-rose-200 text-rose-800 text-xs flex items-center justify-between gap-3">
                        <span>{view.error}</span>
                        <Button
                            size="xs"
                            variant="outline"
                            onClick={() => view.setError(null)}
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
                            onClick={() => view.setActiveTab('board')}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                view.activeTab === 'board'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <LayoutGrid size={15} />
                            <span>{t('marketplace.tabBoard', 'Pinnwand')}</span>
                            {view.snapshot && view.snapshot.board.length > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        view.activeTab === 'board'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {view.snapshot.board.length}
                                </span>
                            )}
                        </button>

                        <button
                            type="button"
                            onClick={() => view.setActiveTab('people')}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                view.activeTab === 'people'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <Users size={15} />
                            <span>{t('marketplace.tabPeople', 'Menschen')}</span>
                            {view.snapshot && view.snapshot.people.length > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        view.activeTab === 'people'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {view.snapshot.people.length}
                                </span>
                            )}
                        </button>

                        <button
                            type="button"
                            onClick={() => {
                                view.setActiveTab('mine');
                                view.setSelectedPersonDid(null);
                                view.setInquiryTargetAd(null);
                            }}
                            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
                                view.activeTab === 'mine'
                                    ? 'bg-white text-theme-secondary shadow-xs font-black'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            <User size={15} />
                            <span>{t('marketplace.tabMine', 'Meins')}</span>
                            {view.unreadInquiriesCount > 0 && (
                                <span
                                    data-testid="marketplace-unread-inquiries-badge"
                                    className="px-1.5 py-0.2 rounded-full text-[10px] bg-rose-600 text-white font-bold"
                                >
                                    {view.unreadInquiriesCount}
                                </span>
                            )}
                            {view.unreadInquiriesCount === 0 && view.totalMineAdsCount > 0 && (
                                <span
                                    className={`px-1.5 py-0.2 rounded-full text-[10px] ${
                                        view.activeTab === 'mine'
                                            ? 'bg-theme-primary/10 text-theme-primary font-bold'
                                            : 'bg-theme-subtle/50 text-theme-light'
                                    }`}
                                >
                                    {view.totalMineAdsCount}
                                </span>
                            )}
                        </button>
                    </div>
                    {view.lastSyncAt && (
                        <span
                            className="text-[11px] text-theme-light self-center sm:self-auto"
                            data-testid="marketplace-last-sync"
                        >
                            {t('common.lastUpdatedPrefix', 'Zuletzt aktualisiert:')}{' '}
                            {formatLastSyncLabel(view.lastSyncAt, t)}
                        </span>
                    )}
                </div>

                {/* Tab content */}
                <div>
                    {view.activeTab === 'board' && (
                        <BoardTab
                            items={view.snapshot?.board || []}
                            people={view.snapshot?.people || []}
                            myCoordinates={view.currentProfile.profile.coordinates}
                            maxDistanceKm={view.maxDistanceKm}
                            onMaxDistanceChange={view.setMaxDistanceKm}
                            mediaAutoLoadMaxHops={view.mediaAutoLoadMaxHops}
                            wifiOnly={view.marketPollWifiOnly}
                            hasSyncService={
                                view.connectedSyncCount !== null && view.connectedSyncCount > 0
                            }
                            onCreateAd={view.handleCreateAdClick}
                            onImportCard={() => view.setIsImportModalOpen(true)}
                            onNavigateToSyncSettings={() =>
                                view.navigateToSettingsSection('sync')
                            }
                            onInquire={view.handleBoardInquire}
                            onPay={view.handleBoardPay}
                            onOwnerClick={(ownerDid, isOwn) => {
                                if (
                                    isOwn ||
                                    (view.currentProfile?.profile?.id &&
                                        isExactSameUserId(ownerDid, view.currentProfile.profile.id))
                                ) {
                                    view.setActiveTab('mine');
                                    view.setSelectedPersonDid(null);
                                    view.setInquiryTargetAd(null);
                                } else {
                                    view.setSelectedPersonDid(ownerDid);
                                }
                            }}
                            onEditOwnAd={(ad) => {
                                const found = view.currentProfile?.ads.find((a) => a.id === ad.id);
                                view.setEditingAd(found || ad);
                                view.setIsCreateAdOpen(true);
                            }}
                        />
                    )}

                    {view.activeTab === 'people' && (
                        <PeopleTab
                            people={view.snapshot?.people || []}
                            myCoordinates={view.currentProfile.profile.coordinates}
                            maxDistanceKm={view.maxDistanceKm}
                            onMaxDistanceChange={view.setMaxDistanceKm}
                            onSelectPerson={(person) =>
                                view.setSelectedPersonDid(person.recipientDid || person.did)
                            }
                        />
                    )}

                    {view.activeTab === 'mine' && (
                        <MineTab
                            myProfile={view.currentProfile}
                            inbox={view.inboxSummary || view.defaultEmptyInbox}
                            contacts={view.contacts}
                            onOpenProfileEditor={() => {
                                if (view.myProfile) view.setIsProfileEditorOpen(true);
                            }}
                            onOpenShareCard={() => view.setIsShareModalOpen(true)}
                            onTogglePublished={view.handleTogglePublished}
                            onCreateAd={view.handleCreateAdClick}
                            onEditAd={(ad) => {
                                view.setEditingAd(ad);
                                view.setIsCreateAdOpen(true);
                            }}
                            onToggleAdPublished={view.handleToggleAdPublished}
                            onRenewAd={(ad) => view.setRenewingAd(ad)}
                            onQuickRenewAd={(ad) => view.handleRenewAd(ad.id, 90)}
                            onDeleteAd={view.handleDeleteAd}
                            onSendInquiryMessage={view.handleSendInquiryMessage}
                            onMarkInquiryRead={view.handleMarkInquiryRead}
                            onPayInquiry={view.handlePayInquiry}
                            onDismissInquiry={view.handleDismissInquiry}
                            onAddContact={(did, suggestedName) =>
                                view.setAddContactTarget({ did, suggestedName })
                            }
                            onViewCard={(did) => {
                                const match =
                                    view.snapshot?.people.find((p) =>
                                        isExactSameUserId(p.recipientDid || p.did, did),
                                    ) ||
                                    view.snapshot?.people.find((p) =>
                                        isExactSameUserId(p.did, did),
                                    );
                                if (match) {
                                    view.setSelectedPersonDid(match.recipientDid || match.did);
                                }
                            }}
                        />
                    )}
                </div>
            </div>

            <MarketplaceModals
                myProfile={view.myProfile}
                currentProfile={view.currentProfile}
                activePublishedMyAdsCount={view.activePublishedMyAdsCount}
                isCreateAdOpen={view.isCreateAdOpen}
                editingAd={view.editingAd}
                renewingAd={view.renewingAd}
                isProfileEditorOpen={view.isProfileEditorOpen}
                isShareModalOpen={view.isShareModalOpen}
                isImportModalOpen={view.isImportModalOpen}
                isPublishPromptOpen={view.isPublishPromptOpen}
                addContactTarget={view.addContactTarget}
                onCloseAdEditor={() => {
                    view.setIsCreateAdOpen(false);
                    view.setEditingAd(null);
                }}
                onSaveAd={view.handleSaveAd}
                onCloseRenewModal={() => view.setRenewingAd(null)}
                onRenewAd={view.handleRenewAd}
                onCloseProfileEditor={() => view.setIsProfileEditorOpen(false)}
                onSaveProfile={view.handleSaveProfile}
                onCloseShareModal={() => view.setIsShareModalOpen(false)}
                onCloseImportModal={() => view.setIsImportModalOpen(false)}
                onImportSuccess={(did) => {
                    view.refreshData(false);
                    if (did) {
                        view.setSelectedPersonDid(did);
                    }
                }}
                onCancelPublishPrompt={() => {
                    view.setIsPublishPromptOpen(false);
                    view.setPendingPublishAdId(null);
                }}
                onConfirmPublishAndAd={view.handleConfirmPublishAndAd}
                onCloseContactDialog={() => view.setAddContactTarget(null)}
                onSaveContactFromInquiry={view.handleSaveContactFromInquiry}
            />
        </PageLayout>
    );
}

export default MarketplaceView;
