// src/components/contacts/useAddressBook.ts
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { postService } from '../../services/postService';
import { recoveryService } from '../../services/recoveryService';
import { settingsService } from '../../services/settingsService';
import { wotService } from '../../services/wotService';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';
import { channelMembersSyncManager } from '../../sync/channelMembersSyncManager';
import {
    Contact,
    FrontendMarketSyncResult,
    FrontendPendingWotHandshake,
    FrontendPostSyncResult,
    FrontendSyncOverviewStatus,
    PublicProfile,
    WotProviderDto,
    WotSummaryDto,
} from '../../types';
import { APP_EVENTS, useAppEvent } from '../../utils/appEvents';
import { stringifyError, translateError } from '../../utils/errorHelper';
import { openFileAsBytes, saveBytesToFile } from '../../utils/fileIo';
import { parseContactCard } from '../../utils/humocoUri';
import { logger } from '../../utils/log';
import {
    notifySyncStatusUpdated,
    showTransientSyncToast,
    SYNC_STATUS_UPDATED_EVENT,
} from '../../utils/syncEvents';
import { isExactSameUserId, truncateUserId } from '../../utils/userIdHelper';

export interface AddressBookFeedback {
    type: 'success' | 'error' | 'warning' | 'info';
    message: string;
    action?: { label: string; onClick: () => void };
}

interface UseAddressBookParams {
    initialSearchQuery?: string;
    initialOpenAddModal?: boolean;
    initialContactPayload?: string;
}

function splitNameToProfile(name: string | undefined): Partial<PublicProfile> | undefined {
    if (!name) return undefined;
    const parts = name.trim().split(/\s+/);
    if (parts.length > 1) {
        return {
            firstName: parts.slice(0, -1).join(' '),
            lastName: parts[parts.length - 1],
        };
    }
    if (parts.length === 1 && parts[0]) {
        return { firstName: parts[0] };
    }
    return undefined;
}

export function useAddressBook({
    initialSearchQuery,
    initialOpenAddModal,
    initialContactPayload,
}: UseAddressBookParams) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const navigation = useOptionalNavigation();

    const [contacts, setContacts] = useState<Contact[]>([]);
    const [wotSummary, setWotSummary] = useState<WotSummaryDto | null>(null);
    const [providers, setProviders] = useState<WotProviderDto[]>([]);
    const [syncOverview, setSyncOverview] = useState<FrontendSyncOverviewStatus | null>(null);
    const [activeTab, setActiveTab] = useState<'contacts' | 'providers'>('contacts');
    const [isUpdatingHops, setIsUpdatingHops] = useState(false);
    const [searchQuery, setSearchQuery] = useState(initialSearchQuery || '');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);

    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [editingContact, setEditingContact] = useState<Contact | null>(null);
    const [initialAddDid, setInitialAddDid] = useState<string | undefined>(undefined);
    const [initialAddProfile, setInitialAddProfile] = useState<Partial<PublicProfile> | undefined>(undefined);
    const [initialAddSyncUrls, setInitialAddSyncUrls] = useState<string[] | undefined>(undefined);
    const [isAddContactModalOpen, setIsAddContactModalOpen] = useState(initialOpenAddModal || false);
    const [isQuickContactModalOpen, setIsQuickContactModalOpen] = useState(false);
    const [isNetworkOptionsOpen, setIsNetworkOptionsOpen] = useState(false);
    const [isTrustListModalOpen, setIsTrustListModalOpen] = useState(false);
    const [isLoading, setIsLoading] = useState(true);
    const [deleteRequest, setDeleteRequest] = useState<string | null>(null);

    const [isExportModalOpen, setIsExportModalOpen] = useState(false);
    const [exportMode, setExportMode] = useState<'contacts' | 'password'>('contacts');
    const [exportPassword, setExportPassword] = useState('');
    const [isExporting, setIsExporting] = useState(false);
    const [refreshingProviderDid, setRefreshingProviderDid] = useState<string | null>(null);
    const [deleteProviderRequest, setDeleteProviderRequest] = useState<WotProviderDto | null>(null);
    const [pendingWotImportBytes, setPendingWotImportBytes] = useState<number[] | null>(null);
    const [isWotPasswordModalOpen, setIsWotPasswordModalOpen] = useState(false);
    const [wotContainerPassword, setWotContainerPassword] = useState('');
    const [isImportingWot, setIsImportingWot] = useState(false);
    const [isSyncingServices, setIsSyncingServices] = useState(false);
    const [feedbackMsg, setFeedbackMsg] = useState<AddressBookFeedback | null>(null);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();

    const [pendingHandshakes, setPendingHandshakes] = useState<FrontendPendingWotHandshake[]>([]);
    const [isHandshakesExpanded, setIsHandshakesExpanded] = useState(false);
    const [handshakeAcceptedLevels, setHandshakeAcceptedLevels] = useState<Record<string, number>>({});
    const [handshakeReturnFlags, setHandshakeReturnFlags] = useState<Record<string, boolean>>({});
    const [isAcceptingHandshake, setIsAcceptingHandshake] = useState<string | null>(null);

    const [guardianDids, setGuardianDids] = useState<Set<string>>(new Set());
    const [guardianWardDids, setGuardianWardDids] = useState<Set<string>>(new Set());
    const [isGuardianSetupOpen, setIsGuardianSetupOpen] = useState(false);

    const hasSyncService = (syncOverview?.connectedCount ?? 0) > 0;

    const handleNavigateToSyncSettings = useCallback(() => {
        navigation?.navigateToSettingsSection?.('sync');
    }, [navigation]);

    const loadContacts = useCallback(async () => {
        setIsLoading(true);
        try {
            const [contactsResult, summaryResult, providersResult, postInbox, dropStatus, protectionStatus] =
                await Promise.all([
                    contactService.getContacts(),
                    wotService.getSummary().catch((e) => {
                        logger.warn('Failed to load WoT summary: ' + stringifyError(e));
                        return null;
                    }),
                    wotService.getProviders().catch((e) => {
                        logger.warn('Failed to load WoT providers: ' + stringifyError(e));
                        return [];
                    }),
                    postService.getPostInboxSummary().catch(() => null),
                    marketService.getSyncOverviewStatus().catch(() => null),
                    recoveryService.getRecoveryProtectionStatus().catch(() => null),
                ]);

            setContacts(contactsResult);
            setWotSummary(summaryResult ?? null);
            setProviders(providersResult ?? []);
            setSyncOverview(dropStatus ?? null);
            setGuardianDids(
                protectionStatus?.guardians
                    ? new Set(protectionStatus.guardians.map((g) => g.userId))
                    : new Set(),
            );
            setGuardianWardDids(
                Array.isArray(protectionStatus?.guardianWards)
                    ? new Set(protectionStatus.guardianWards.map((w) => w.ownerDid))
                    : new Set(),
            );
            if (postInbox) {
                const handshakes = postInbox.pendingWotHandshakes || [];
                setPendingHandshakes(handshakes);
                if (handshakes.length > 0) {
                    setIsHandshakesExpanded(true);
                }
            }
        } catch (err) {
            logger.error('Failed to load contacts: ' + stringifyError(err));
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadContacts();
        postService.syncPostChannels().catch(() => {});
        const onDropUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendSyncOverviewStatus | undefined>;
            if (custom.detail) {
                setSyncOverview(custom.detail);
            } else {
                marketService.getSyncOverviewStatus().then(setSyncOverview).catch(() => {});
            }
        };
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, onDropUpdated);
        return () => {
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onDropUpdated);
        };
    }, [loadContacts]);

    useAppEvent(APP_EVENTS.POST_INBOX_UPDATED, loadContacts);
    useAppEvent(APP_EVENTS.WOT_UPDATED, loadContacts);

    useEffect(() => {
        if (initialSearchQuery) {
            setSearchQuery(initialSearchQuery);
        }
    }, [initialSearchQuery]);

    useEffect(() => {
        if (initialOpenAddModal) {
            setIsAddContactModalOpen(true);
        }
    }, [initialOpenAddModal]);

    useEffect(() => {
        if (!initialContactPayload) return;
        const parsed = parseContactCard(initialContactPayload);
        if (parsed && parsed.did) {
            setEditingContact(null);
            setInitialAddDid(parsed.did);
            setInitialAddProfile(splitNameToProfile(parsed.name));
            setInitialAddSyncUrls(parsed.syncUrls);
            setIsDialogOpen(true);
        } else {
            setIsAddContactModalOpen(true);
        }
    }, [initialContactPayload]);

    const handleSaveContact = async (contact: Contact) => {
        try {
            const wasNew = !contacts.some((c) => isExactSameUserId(c.did, contact.did));
            await protectAction(async (pwd) => {
                await contactService.saveContact(contact, pwd || undefined);
            });
            channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
            await loadContacts();
            if (wasNew) {
                const displayName =
                    [contact.profile.firstName, contact.profile.lastName].filter(Boolean).join(' ') ||
                    contact.profile.organization ||
                    truncateUserId(contact.did);
                setFeedbackMsg({
                    type: 'success',
                    message: t('contacts.contactAddedReciprocalHint', { name: displayName }),
                    action: {
                        label: t('contacts.shareMyProfileBtn'),
                        onClick: () => setIsQuickContactModalOpen(true),
                    },
                });
            }
            logger.info('Contact saved successfully');
        } catch (err) {
            logger.error('Failed to save contact: ' + stringifyError(err));
            throw err;
        }
    };

    const handleDeleteContact = async () => {
        if (!deleteRequest) return;
        try {
            await protectAction(async (pwd) => {
                await contactService.deleteContact(deleteRequest, pwd || undefined);
            });
            channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
            await loadContacts();
            logger.info('Contact deleted');
        } catch (err) {
            logger.error('Failed to delete contact: ' + stringifyError(err));
        } finally {
            setDeleteRequest(null);
        }
    };

    const handleSetMaxHops = async (hops: number) => {
        const currentHops = wotSummary?.ownListMaxHops ?? 2;
        if (isUpdatingHops || hops === currentHops) return;
        setIsUpdatingHops(true);
        try {
            const updated = await protectAction(async (pwd) => wotService.setMaxHops(hops, pwd || null));
            if (updated) {
                setWotSummary(updated);
            }
            await loadContacts();
        } catch (err) {
            logger.error('Failed to update max hops: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsUpdatingHops(false);
        }
    };

    const saveWotPackageBytes = async (bytes: number[] | undefined, defaultFilename: string) => {
        if (!bytes || bytes.length === 0) return;
        const config = await settingsService.getConfig().catch(() => null);
        const saveResult = await saveBytesToFile(bytes, {
            defaultFilename,
            filters: [{ name: t('transfer.importFile.wotFilter'), extensions: ['wot'] }],
            uiPreferences: config?.uiPreferences,
            protectAction,
        });
        if (saveResult) {
            setFeedbackMsg({ type: 'success', message: t('contacts.exportWotSuccess') });
            await loadContacts();
        }
    };

    const handleExportOpenWot = async () => {
        setIsExporting(true);
        try {
            const bytes = await protectAction(async (walletPassword) => {
                if (exportMode === 'contacts') {
                    return wotService.exportPackage({
                        shareWithAllContacts: true,
                        password: walletPassword || null,
                    });
                }
                return wotService.exportPackage({
                    containerPassword: exportPassword.trim() || null,
                    password: walletPassword || null,
                });
            });
            setIsExportModalOpen(false);
            setExportPassword('');
            await saveWotPackageBytes(bytes ?? undefined, 'trust-network.wot');
        } catch (err) {
            logger.error('Failed to export WoT package: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsExporting(false);
        }
    };

    const handleShareContactWot = async (contact: Contact) => {
        try {
            const bytes = await protectAction(async (walletPassword) =>
                wotService.exportPackage({
                    recipientDid: contact.did,
                    password: walletPassword || null,
                }),
            );
            const safeName = (
                contact.profile.firstName || t('contacts.defaultContactName', 'contact')
            )
                .toLowerCase()
                .replace(/\s+/g, '-');
            await saveWotPackageBytes(bytes ?? undefined, `trust-${safeName}.wot`);
        } catch (err) {
            logger.error('Failed to export contact WoT package: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        }
    };

    const executeImportWot = async (fileBytes: number[], password?: string) => {
        setIsImportingWot(true);
        try {
            const result = await protectAction(async (walletPassword) =>
                wotService.importPackage({
                    containerBytes: fileBytes,
                    containerPassword: password || null,
                    password: walletPassword || null,
                }),
            );
            setIsWotPasswordModalOpen(false);
            setPendingWotImportBytes(null);
            setWotContainerPassword('');
            if (result) {
                channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
                if (result.isProviderFilter) {
                    setFeedbackMsg({
                        type: 'success',
                        message: t('contacts.providerImportSuccess', {
                            name: result.providerName || truncateUserId(result.senderDid),
                            count: result.providerMemberCount ?? 0,
                        }),
                    });
                    await loadContacts();
                    setActiveTab('providers');
                } else {
                    setFeedbackMsg({
                        type: 'success',
                        message: t('transfer.wotImport.stats', {
                            imported: result.imported,
                            updated: result.updated,
                        }),
                    });
                    await loadContacts();
                }
            }
        } catch (err) {
            const errorStr = stringifyError(err);
            const isPasswordRequired =
                errorStr.includes('Password required') ||
                errorStr.toLowerCase().includes('decrypt') ||
                errorStr.includes('SymmetricEncryption') ||
                errorStr.includes('MacError');
            if (isPasswordRequired) {
                setPendingWotImportBytes(fileBytes);
                setIsWotPasswordModalOpen(true);
                return;
            }
            logger.error('Failed to import WoT package: ' + errorStr);
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsImportingWot(false);
        }
    };

    const handleImportWot = async () => {
        try {
            const config = await settingsService.getConfig().catch(() => null);
            const fileResult = await openFileAsBytes({
                filters: [{ name: t('transfer.importFile.wotFilter'), extensions: ['wot'] }],
                uiPreferences: config?.uiPreferences,
                protectAction,
            });
            if (fileResult) {
                await executeImportWot(fileResult.bytes);
            }
        } catch (err) {
            logger.error('Failed to read WoT file: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleSetProviderTrust = async (issuerDid: string, level: number) => {
        try {
            await protectAction(async (pwd) => {
                await wotService.setProviderTrust(issuerDid, level, pwd || null);
            });
            channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
            await loadContacts();
        } catch (err) {
            logger.error('Failed to set provider trust: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleRefreshProvider = async (provider: WotProviderDto) => {
        setRefreshingProviderDid(provider.issuerDid);
        try {
            await protectAction(async (pwd) => {
                await wotService.refreshProviderUrl(provider.issuerDid, pwd || null);
            });
            setFeedbackMsg({
                type: 'success',
                message: t('contacts.providerRefreshSuccess', { name: provider.name }),
            });
            await loadContacts();
        } catch (err) {
            logger.error('Failed to refresh provider: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setRefreshingProviderDid(null);
        }
    };

    const handleDeleteProvider = async () => {
        if (!deleteProviderRequest) return;
        try {
            await protectAction(async (pwd) => {
                await wotService.removeProvider(deleteProviderRequest.issuerDid, pwd || null);
            });
            channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
            setFeedbackMsg({ type: 'success', message: t('contacts.providerRemovedSuccess') });
            await loadContacts();
        } catch (err) {
            logger.error('Failed to remove provider: ' + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setDeleteProviderRequest(null);
        }
    };

    const applySyncFeedback = (
        res: FrontendMarketSyncResult | undefined,
        postRes: FrontendPostSyncResult | null,
        updatedOverview: FrontendSyncOverviewStatus | null,
    ) => {
        const allErrors = [...(res?.errors ?? []), ...(postRes?.errors ?? [])];
        if (allErrors.length > 0) {
            const has429 = allErrors.some((e) => e.includes('429') || e.toLowerCase().includes('rate limit'));
            const errorMsg = has429
                ? `${t('contacts.syncServicesErrors', 'Fehler beim Synchronisieren:')} Rate-Limit erreicht (HTTP 429). Bitte später erneut versuchen.`
                : `${t('contacts.syncServicesErrors', 'Fehler beim Synchronisieren:')} ${allErrors.join(', ')}`;
            setFeedbackMsg({ type: 'error', message: errorMsg });
            return;
        }

        const totalConnected = updatedOverview?.connectedCount ?? syncOverview?.connectedCount ?? 0;
        const totalPolled = (res?.polledSyncs ?? 0) + (postRes?.polledSyncs ?? 0);
        const totalUploaded = (res?.uploadedTo ?? 0) + (postRes?.uploadedTo ?? 0);
        const hasActivity =
            totalConnected > 0 ||
            totalPolled > 0 ||
            totalUploaded > 0 ||
            (res?.importedWotLists ?? 0) > 0 ||
            (res?.importedCards ?? 0) > 0;

        if (!hasActivity) {
            setFeedbackMsg({
                type: 'warning',
                message: t(
                    'contacts.syncServicesNoneConfigured',
                    'Kein Sync-Dienst eingerichtet. Richte zuerst einen Sync-Dienst ein, um Vertrauenslisten und Angebote abzugleichen.',
                ),
                action: {
                    label: t('contacts.connectSyncServiceBtn', 'Sync-Dienst verbinden'),
                    onClick: handleNavigateToSyncSettings,
                },
            });
            return;
        }

        setFeedbackMsg({
            type: 'success',
            message: t(
                'contacts.syncServicesSuccess',
                'Sync-Dienste synchronisiert: {{wot}} Vertrauensliste(n), {{cards}} Angebot(e).',
                { wot: res?.importedWotLists ?? 0, cards: res?.importedCards ?? 0 },
            ),
        });
        const hasAnyNew =
            (res?.importedWotLists ?? 0) > 0 ||
            (res?.importedCards ?? 0) > 0 ||
            (postRes?.importedPayments ?? 0) > 0 ||
            (postRes?.newWotHandshakes ?? 0) > 0;
        if (!hasAnyNew) {
            showTransientSyncToast({
                message: t('common.syncUpToDateToast', 'Alles aktuell – keine neuen Einträge.'),
                variant: 'info',
            });
        }
    };

    const handleSyncServices = async () => {
        if (isCoolingDown) return;
        setIsSyncingServices(true);
        try {
            const [res, postRes] = await Promise.all([
                marketService.syncMarketChannels({ forceUpload: true }),
                postService.syncPostChannels().catch((err) => {
                    checkAndTriggerCooldown(err);
                    return null;
                }),
            ]);
            if (res) checkAndTriggerCooldown(res);
            if (postRes) checkAndTriggerCooldown(postRes);

            const updatedOverview = await marketService.getSyncOverviewStatus().catch(() => null);
            if (updatedOverview) {
                setSyncOverview(updatedOverview);
                notifySyncStatusUpdated(updatedOverview);
            } else {
                notifySyncStatusUpdated();
            }
            await loadContacts();
            applySyncFeedback(res, postRes, updatedOverview);
        } catch (err) {
            checkAndTriggerCooldown(err);
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsSyncingServices(false);
        }
    };

    const handleAcceptHandshake = async (handshake: FrontendPendingWotHandshake) => {
        const level = handshakeAcceptedLevels[handshake.msgId] ?? handshake.proposedLevel ?? 1;
        const returnHandshake = handshakeReturnFlags[handshake.msgId] ?? handshake.replyRequested ?? true;
        setIsAcceptingHandshake(handshake.msgId);
        try {
            const res = await protectAction(async (pwd) =>
                postService.acceptWotHandshake(handshake.msgId, level, returnHandshake, pwd || undefined),
            );
            if (res) {
                setFeedbackMsg({
                    type: 'success',
                    message: t('contacts.handshakeAcceptedSuccess', 'Vertrauensbestätigung akzeptiert'),
                });
                await loadContacts();
            }
        } catch (e) {
            setFeedbackMsg({ type: 'error', message: translateError(e, t) });
        } finally {
            setIsAcceptingHandshake(null);
        }
    };

    const handleDismissHandshake = async (msgId: string) => {
        try {
            await protectAction(async (pwd) => {
                await postService.dismissPendingPostItem('wot_handshake', msgId, pwd || undefined);
            });
            setPendingHandshakes((prev) => prev.filter((h) => h.msgId !== msgId));
        } catch (e) {
            setFeedbackMsg({ type: 'error', message: translateError(e, t) });
        }
    };

    const openEmptyAddModal = () => {
        setInitialAddDid(undefined);
        setInitialAddProfile(undefined);
        setInitialAddSyncUrls(undefined);
        setIsAddContactModalOpen(true);
    };

    const openEditContactDialog = (contact: Contact) => {
        setEditingContact(contact);
        setIsDialogOpen(true);
    };

    const closeContactDialog = () => {
        setIsDialogOpen(false);
        setInitialAddDid(undefined);
        setInitialAddProfile(undefined);
        setInitialAddSyncUrls(undefined);
    };

    const allTags = useMemo(
        () => Array.from(new Set(contacts.flatMap((c) => c.tags))),
        [contacts],
    );

    const filteredContacts = useMemo(() => {
        const q = searchQuery.toLowerCase();
        return contacts.filter((contact) => {
            const matchesSearch =
                (contact.profile.firstName || '').toLowerCase().includes(q) ||
                (contact.profile.lastName || '').toLowerCase().includes(q) ||
                (contact.profile.organization || '').toLowerCase().includes(q) ||
                contact.did.toLowerCase().includes(q);
            const matchesTag = !selectedTag || contact.tags.includes(selectedTag);
            return matchesSearch && matchesTag;
        });
    }, [contacts, searchQuery, selectedTag]);

    const currentMaxHops = wotSummary?.ownListMaxHops ?? 2;

    return {
        navigation,
        protectAction,
        contacts,
        wotSummary,
        providers,
        activeTab,
        setActiveTab,
        isUpdatingHops,
        searchQuery,
        setSearchQuery,
        selectedTag,
        setSelectedTag,
        isDialogOpen,
        editingContact,
        initialAddDid,
        initialAddProfile,
        initialAddSyncUrls,
        isAddContactModalOpen,
        setIsAddContactModalOpen,
        isQuickContactModalOpen,
        setIsQuickContactModalOpen,
        isNetworkOptionsOpen,
        setIsNetworkOptionsOpen,
        isTrustListModalOpen,
        setIsTrustListModalOpen,
        isLoading,
        deleteRequest,
        setDeleteRequest,
        isExportModalOpen,
        setIsExportModalOpen,
        exportMode,
        setExportMode,
        exportPassword,
        setExportPassword,
        isExporting,
        refreshingProviderDid,
        deleteProviderRequest,
        setDeleteProviderRequest,
        pendingWotImportBytes,
        setPendingWotImportBytes,
        isWotPasswordModalOpen,
        setIsWotPasswordModalOpen,
        wotContainerPassword,
        setWotContainerPassword,
        isImportingWot,
        isSyncingServices,
        feedbackMsg,
        setFeedbackMsg,
        cooldownSeconds,
        isCoolingDown,
        pendingHandshakes,
        isHandshakesExpanded,
        setIsHandshakesExpanded,
        handshakeAcceptedLevels,
        setHandshakeAcceptedLevels,
        handshakeReturnFlags,
        setHandshakeReturnFlags,
        isAcceptingHandshake,
        guardianDids,
        guardianWardDids,
        isGuardianSetupOpen,
        setIsGuardianSetupOpen,
        hasSyncService,
        allTags,
        filteredContacts,
        currentMaxHops,
        loadContacts,
        handleNavigateToSyncSettings,
        handleSaveContact,
        handleDeleteContact,
        handleSetMaxHops,
        handleExportOpenWot,
        handleShareContactWot,
        executeImportWot,
        handleImportWot,
        handleSetProviderTrust,
        handleRefreshProvider,
        handleDeleteProvider,
        handleSyncServices,
        handleAcceptHandshake,
        handleDismissHandshake,
        openEmptyAddModal,
        openEditContactDialog,
        closeContactDialog,
        setEditingContact,
        setInitialAddDid,
        setInitialAddProfile,
        setInitialAddSyncUrls,
        setIsDialogOpen,
    };
}
