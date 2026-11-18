// src/components/contacts/AddressBook.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Building2, QrCode, Search, UserPlus, Users, X } from 'lucide-react';
import { PageLayout } from '../ui/PageLayout';
import { Button } from '../ui/Button';
import { Advanced } from '../ui/Advanced';
import { ExpertTabsLayout } from '../ui/ExpertTabsLayout';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { AlertBanner } from '../ui/AlertBanner';
import { PageEmptyState, PageLoadingState } from '../ui/PageState';
import { SearchFilterBar } from '../ui/SearchFilterBar';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { GuardianSetupModal } from '../auth/GuardianSetupModal';
import { QuickContactModal } from '../profile/QuickContactModal';
import { WotNetworkOptionsPanel } from '../wot/WotNetworkOptionsPanel';
import ContactDialog from './ContactDialog';
import { AddContactModal } from './AddContactModal';
import { ContactListItem } from './ContactListItem';
import { PendingHandshakesList } from './PendingHandshakesList';
import { WotProvidersGrid } from './WotProvidersGrid';
import { WotSyncActionsBar, WotTrustPackageModals } from './WotSyncActionsBar';
import { useAddressBook } from './useAddressBook';

export interface AddressBookProps {
    onBack?: () => void;
    initialSearchQuery?: string;
    initialOpenAddModal?: boolean;
    initialContactPayload?: string;
}

const AddressBook: React.FC<AddressBookProps> = ({
    onBack,
    initialSearchQuery,
    initialOpenAddModal,
    initialContactPayload,
}) => {
    const { t } = useTranslation();
    const state = useAddressBook({
        initialSearchQuery,
        initialOpenAddModal,
        initialContactPayload,
    });

    const tagFiltersSlot = (
        <div className="flex items-center gap-2 overflow-x-auto pb-2 sm:pb-0 no-scrollbar scroll-smooth px-1 max-w-full">
            <button
                type="button"
                onClick={() => state.setSelectedTag(null)}
                className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                    !state.selectedTag
                        ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20'
                        : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                }`}
            >
                {t('contacts.filterAll')}
            </button>
            {state.allTags.map((tag) => (
                <button
                    key={tag}
                    type="button"
                    onClick={() => state.setSelectedTag(tag)}
                    className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                        state.selectedTag === tag
                            ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20'
                            : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                    }`}
                >
                    {tag}
                </button>
            ))}
        </div>
    );

    const contactsSection = (
        <>
            <SearchFilterBar
                value={state.searchQuery}
                onChange={state.setSearchQuery}
                placeholder={t('contacts.searchPlaceholder')}
                clearLabel={t('contacts.clearSearch')}
                inputClassName="bg-white/50 backdrop-blur-sm py-4 font-medium"
                rightSlot={tagFiltersSlot}
            />

            {state.isLoading ? (
                <PageLoadingState message={t('contacts.loading')} />
            ) : state.contacts.length === 0 ? (
                <PageEmptyState
                    icon={<Users size={32} />}
                    title={t('contacts.emptyNoContactsTitle', 'Noch keine Kontakte')}
                    description={t(
                        'contacts.emptyNoContactsDesc',
                        'Noch keine Kontakte. Füge die erste Person hinzu, der du vertraust.',
                    )}
                    action={
                        <Button onClick={state.openEmptyAddModal} className="gap-2">
                            <UserPlus size={18} />
                            <span>{t('contacts.addContact', 'Kontakt hinzufügen')}</span>
                        </Button>
                    }
                />
            ) : state.filteredContacts.length === 0 ? (
                <PageEmptyState
                    icon={<Search size={32} />}
                    title={t('contacts.emptyNoMatchesTitle', 'Keine Treffer')}
                    description={
                        state.searchQuery
                            ? t('contacts.emptyNoMatchesDesc', {
                                  query: state.searchQuery,
                                  defaultValue: `Keine Treffer für „${state.searchQuery}“.`,
                              })
                            : t('contacts.emptyNoMatchesFilterDesc', 'Keine Kontakte für diesen Filter gefunden.')
                    }
                    action={
                        <Button
                            variant="secondary"
                            onClick={() => {
                                state.setSearchQuery('');
                                state.setSelectedTag(null);
                            }}
                            className="gap-2"
                        >
                            <X size={18} />
                            <span>{t('contacts.resetFilterBtn', 'Filter zurücksetzen')}</span>
                        </Button>
                    }
                />
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    {state.filteredContacts.map((contact) => (
                        <ContactListItem
                            key={contact.did}
                            contact={contact}
                            isGuardian={state.guardianDids.has(contact.did)}
                            isGuardianWard={state.guardianWardDids.has(contact.did)}
                            onShareWot={state.handleShareContactWot}
                            onEdit={state.openEditContactDialog}
                            onDelete={state.setDeleteRequest}
                        />
                    ))}
                </div>
            )}
        </>
    );

    const providersSection = (
        <WotProvidersGrid
            providers={state.providers}
            refreshingProviderDid={state.refreshingProviderDid}
            onRefreshProvider={state.handleRefreshProvider}
            onDeleteProvider={state.setDeleteProviderRequest}
            onSetProviderTrust={state.handleSetProviderTrust}
        />
    );

    return (
        <PageLayout
            title={t('contacts.title')}
            onBack={onBack || state.navigation?.goBack}
            description={t('contacts.description')}
            maxWidth="6xl"
            optionsPanel={
                state.isNetworkOptionsOpen ? (
                    <WotNetworkOptionsPanel
                        wotSummary={state.wotSummary}
                        currentMaxHops={state.currentMaxHops}
                        isUpdatingHops={state.isUpdatingHops}
                        hasSyncService={state.hasSyncService}
                        protectAction={state.protectAction}
                        onSelectMaxHops={state.handleSetMaxHops}
                        onRefreshWot={() => {
                            void state.loadContacts();
                        }}
                        onOpenTrustListModal={() => state.setIsTrustListModalOpen(true)}
                        onNavigateToSyncSettings={state.handleNavigateToSyncSettings}
                        onClose={() => state.setIsNetworkOptionsOpen(false)}
                    />
                ) : null
            }
            actions={
                <>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => state.setIsQuickContactModalOpen(true)}
                        className="gap-2"
                        title={t('contacts.shareMyProfileBtn', 'Mein Profil teilen')}
                        data-testid="my-quick-contact-button"
                    >
                        <QrCode size={18} />
                        <span>{t('contacts.shareMyProfileBtn', 'Mein Profil teilen')}</span>
                    </Button>
                    <Button
                        size="sm"
                        onClick={state.openEmptyAddModal}
                        className="gap-2"
                        title={t('contacts.addContact')}
                    >
                        <UserPlus size={18} />
                        <span>{t('contacts.addContact')}</span>
                    </Button>
                </>
            }
            advancedActions={
                <WotSyncActionsBar
                    isSyncingServices={state.isSyncingServices}
                    isCoolingDown={state.isCoolingDown}
                    cooldownSeconds={state.cooldownSeconds}
                    isNetworkOptionsOpen={state.isNetworkOptionsOpen}
                    onSyncServices={state.handleSyncServices}
                    onToggleNetworkOptions={() => state.setIsNetworkOptionsOpen((prev) => !prev)}
                    onOpenGuardianSetup={() => state.setIsGuardianSetupOpen(true)}
                />
            }
        >
            <div className="space-y-8">
                {state.feedbackMsg && (
                    <AlertBanner
                        variant={state.feedbackMsg.type}
                        onDismiss={() => state.setFeedbackMsg(null)}
                        action={
                            state.feedbackMsg.action ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="secondary"
                                    onClick={state.feedbackMsg.action.onClick}
                                    className="shrink-0 bg-white/80 hover:bg-white text-theme-primary font-bold shadow-xs border-amber-300/60"
                                >
                                    {state.feedbackMsg.action.label}
                                </Button>
                            ) : undefined
                        }
                    >
                        {state.feedbackMsg.message}
                    </AlertBanner>
                )}

                <PendingHandshakesList
                    pendingHandshakes={state.pendingHandshakes}
                    isExpanded={state.isHandshakesExpanded}
                    onToggleExpanded={() => state.setIsHandshakesExpanded(!state.isHandshakesExpanded)}
                    acceptedLevels={state.handshakeAcceptedLevels}
                    onLevelChange={(msgId, lvl) =>
                        state.setHandshakeAcceptedLevels((prev) => ({ ...prev, [msgId]: lvl }))
                    }
                    returnFlags={state.handshakeReturnFlags}
                    onReturnFlagChange={(msgId, checked) =>
                        state.setHandshakeReturnFlags((prev) => ({ ...prev, [msgId]: checked }))
                    }
                    isAcceptingHandshake={state.isAcceptingHandshake}
                    onAcceptHandshake={state.handleAcceptHandshake}
                    onDismissHandshake={state.handleDismissHandshake}
                />

                {state.providers.length > 0 && (
                    <Advanced>
                        <div className="flex justify-start">
                            <SegmentedTabs
                                tabs={[
                                    {
                                        id: 'contacts',
                                        label: `${t('contacts.tabPersonalContacts')} (${state.contacts.length})`,
                                        icon: <Users size={16} />,
                                    },
                                    {
                                        id: 'providers',
                                        label: `${t('contacts.tabProviders')} (${state.providers.length})`,
                                        icon: <Building2 size={16} />,
                                    },
                                ]}
                                activeTab={state.activeTab}
                                onChange={(id) => state.setActiveTab(id as 'contacts' | 'providers')}
                            />
                        </div>
                    </Advanced>
                )}

                <ExpertTabsLayout
                    simpleContent={contactsSection}
                    expertContent={
                        state.activeTab === 'providers' && state.providers.length > 0
                            ? providersSection
                            : contactsSection
                    }
                />
            </div>

            <AddContactModal
                isOpen={state.isAddContactModalOpen}
                onClose={() => state.setIsAddContactModalOpen(false)}
                onOpenManualAdd={(initialDid, initialProfile, initialSyncUrls) => {
                    state.setEditingContact(null);
                    state.setInitialAddDid(initialDid);
                    state.setInitialAddProfile(initialProfile);
                    state.setInitialAddSyncUrls(initialSyncUrls);
                    state.setIsDialogOpen(true);
                }}
                onContactAdded={state.loadContacts}
                onSuccess={(msg) => state.setFeedbackMsg({ type: 'success', message: msg })}
                onError={(msg) => state.setFeedbackMsg({ type: 'error', message: msg })}
            />

            <QuickContactModal
                isOpen={state.isQuickContactModalOpen}
                onClose={() => state.setIsQuickContactModalOpen(false)}
            />

            <WotTrustPackageModals
                wotSummary={state.wotSummary}
                isTrustListModalOpen={state.isTrustListModalOpen}
                onCloseTrustListModal={() => state.setIsTrustListModalOpen(false)}
                onImportWot={state.handleImportWot}
                onOpenExportModal={() => {
                    state.setExportMode((state.wotSummary?.ratedContactsCount ?? 0) > 0 ? 'contacts' : 'password');
                    state.setExportPassword('');
                    state.setIsExportModalOpen(true);
                }}
                isExportModalOpen={state.isExportModalOpen}
                onCloseExportModal={() => {
                    state.setIsExportModalOpen(false);
                    state.setExportPassword('');
                }}
                exportMode={state.exportMode}
                onExportModeChange={state.setExportMode}
                exportPassword={state.exportPassword}
                onExportPasswordChange={state.setExportPassword}
                isExporting={state.isExporting}
                onConfirmExport={state.handleExportOpenWot}
                isWotPasswordModalOpen={state.isWotPasswordModalOpen}
                onCloseWotPasswordModal={() => {
                    state.setIsWotPasswordModalOpen(false);
                    state.setWotContainerPassword('');
                    state.setPendingWotImportBytes(null);
                }}
                wotContainerPassword={state.wotContainerPassword}
                onWotContainerPasswordChange={state.setWotContainerPassword}
                isImportingWot={state.isImportingWot}
                onConfirmEncryptedImport={() => {
                    if (state.pendingWotImportBytes) {
                        void state.executeImportWot(state.pendingWotImportBytes, state.wotContainerPassword);
                    }
                }}
            />

            <ContactDialog
                isOpen={state.isDialogOpen}
                onClose={state.closeContactDialog}
                onSave={state.handleSaveContact}
                existingContact={state.editingContact}
                initialDid={state.initialAddDid}
                initialProfile={state.initialAddProfile}
                initialSyncUrls={state.initialAddSyncUrls}
            />

            <ConfirmationModal
                isOpen={!!state.deleteRequest}
                title={t('contacts.deleteTitle')}
                description={t('contacts.deleteDescription')}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={state.handleDeleteContact}
                onCancel={() => state.setDeleteRequest(null)}
            />

            <ConfirmationModal
                isOpen={!!state.deleteProviderRequest}
                title={t('contacts.deleteProviderTitle')}
                description={t('contacts.deleteProviderDesc', { name: state.deleteProviderRequest?.name })}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={state.handleDeleteProvider}
                onCancel={() => state.setDeleteProviderRequest(null)}
            />

            <GuardianSetupModal
                isOpen={state.isGuardianSetupOpen}
                initialGuardianDids={Array.from(state.guardianDids)}
                onClose={() => {
                    state.setIsGuardianSetupOpen(false);
                    void state.loadContacts();
                }}
            />
        </PageLayout>
    );
};

export default AddressBook;
