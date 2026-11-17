// src/components/AddressBook.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { PageLayout } from './ui/PageLayout';
import { Button } from './ui/Button';
import { Advanced } from './ui/Advanced';
import { ExpertTabsLayout } from './ui/ExpertTabsLayout';
import { TechId } from './ui/TechId';
import { contactService } from '../services/contactService';
import { wotService } from '../services/wotService';
import { postService, POST_INBOX_UPDATED_EVENT } from '../services/postService';
import { settingsService } from '../services/settingsService';
import { marketService, triggerDebouncedChannelMembersSync } from '../services/marketService';
import { useRateLimitCooldown } from '../hooks/useRateLimitCooldown';
import { notifySyncStatusUpdated, SYNC_STATUS_UPDATED_EVENT, showTransientSyncToast } from '../utils/syncEvents';
import { save, open } from '@tauri-apps/plugin-dialog';
import { writeFile, readFile } from '@tauri-apps/plugin-fs';
import { InfoHint } from './ui/InfoHint';
import { 
    Contact, 
    WotSummaryDto, 
    WotProviderDto, 
    FrontendPendingWotHandshake, 
    FrontendSyncOverviewStatus,
    PublicProfile 
} from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { UserAvatar } from './ui/UserAvatar';
import ContactDialog from './ContactDialog';
import { AddContactModal } from './AddContactModal';
import { TrustLevelRadioCards } from './TrustLevelRadioCards';
import { logger } from '../utils/log';
import { stringifyError, translateError } from '../utils/errorHelper';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { useSession } from '../context/SessionContext';
import { useOptionalNavigation } from '../context/NavigationContext';
import { AlertBanner } from './ui/AlertBanner';
import { ModalShell } from './ui/ModalShell';
import { PageLoadingState, PageEmptyState } from './ui/PageState';
import { truncateUserId } from '../utils/userIdHelper';
import { parseContactCard } from '../utils/humocoUri';
import { SegmentedTabs } from './ui/SegmentedTabs';
import { 
    UserPlus, 
    Search, 
    Trash2, 
    Edit3, 
    Tag, 
    Building2, 
    Users,
    X,
    Share2,
    Upload,
    RefreshCw,
    Server,
    Handshake,
    ChevronDown,
    ChevronUp,
    Check,
    QrCode,
    SlidersHorizontal,
    ShieldCheck,
    UserCheck,
} from 'lucide-react';
import { recoveryService } from '../services/recoveryService';
import { GuardianSetupModal } from './GuardianSetupModal';
import { QuickContactModal } from './profile/QuickContactModal';
import { WotNetworkOptionsPanel } from './wot/WotNetworkOptionsPanel';

interface AddressBookProps {
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
    const [feedbackMsg, setFeedbackMsg] = useState<{
        type: 'success' | 'error' | 'warning' | 'info';
        message: string;
        action?: { label: string; onClick: () => void };
    } | null>(null);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();

    // Handshakes
    const [pendingHandshakes, setPendingHandshakes] = useState<FrontendPendingWotHandshake[]>([]);
    const [isHandshakesExpanded, setIsHandshakesExpanded] = useState(false);
    const [handshakeAcceptedLevels, setHandshakeAcceptedLevels] = useState<{ [msgId: string]: number }>({});
    const [handshakeReturnFlags, setHandshakeReturnFlags] = useState<{ [msgId: string]: boolean }>({});
    const [isAcceptingHandshake, setIsAcceptingHandshake] = useState<string | null>(null);

    const [guardianDids, setGuardianDids] = useState<Set<string>>(new Set());
    const [guardianWardDids, setGuardianWardDids] = useState<Set<string>>(new Set());
    const [isGuardianSetupOpen, setIsGuardianSetupOpen] = useState(false);

    const hasSyncService = (syncOverview?.connectedCount ?? 0) > 0;

    const handleNavigateToSyncSettings = () => {
        if (navigation?.navigateToSettingsSection) {
            navigation.navigateToSettingsSection('sync');
        }
    };

    const loadContacts = useCallback(async () => {
        setIsLoading(true);
        try {
            const [contactsResult, summaryResult, providersResult, postInbox, dropStatus, protectionStatus] = await Promise.all([
                contactService.getContacts(),
                wotService.getSummary().catch(e => {
                    logger.warn("Failed to load WoT summary: " + stringifyError(e));
                    return null;
                }),
                wotService.getProviders().catch(e => {
                    logger.warn("Failed to load WoT providers: " + stringifyError(e));
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
            if (protectionStatus?.guardians) {
                setGuardianDids(new Set(protectionStatus.guardians.map(g => g.userId)));
            } else {
                setGuardianDids(new Set());
            }
            if (Array.isArray(protectionStatus?.guardianWards)) {
                setGuardianWardDids(new Set(protectionStatus.guardianWards.map(w => w.ownerDid)));
            } else {
                setGuardianWardDids(new Set());
            }
            if (postInbox) {
                const handshakes = postInbox.pendingWotHandshakes || [];
                setPendingHandshakes(handshakes);
                if (handshakes.length > 0) {
                    setIsHandshakesExpanded(true);
                }
            }
        } catch (err) {
            logger.error("Failed to load contacts: " + stringifyError(err));
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadContacts();
        postService.syncPostChannels().catch(() => {});
        window.addEventListener(POST_INBOX_UPDATED_EVENT, loadContacts);
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
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, loadContacts);
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onDropUpdated);
        };
    }, [loadContacts]);

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
            let initialProfile: Partial<PublicProfile> | undefined;
            if (parsed.name) {
                const parts = parsed.name.trim().split(/\s+/);
                if (parts.length > 1) {
                    initialProfile = {
                        firstName: parts.slice(0, -1).join(' '),
                        lastName: parts[parts.length - 1],
                    };
                } else if (parts.length === 1 && parts[0]) {
                    initialProfile = { firstName: parts[0] };
                }
            }
            setEditingContact(null);
            setInitialAddDid(parsed.did);
            setInitialAddProfile(initialProfile);
            setInitialAddSyncUrls(parsed.syncUrls);
            setIsDialogOpen(true);
        } else {
            setIsAddContactModalOpen(true);
        }
    }, [initialContactPayload]);

    const handleSaveContact = async (contact: Contact) => {
        try {
            await protectAction(async (pwd) => {
                await contactService.saveContact(contact, pwd || undefined);
            });
            triggerDebouncedChannelMembersSync(1500);
            await loadContacts();
            logger.info("Contact saved successfully");
        } catch (err) {
            logger.error("Failed to save contact: " + stringifyError(err));
            throw err;
        }
    };

    const handleDeleteContact = async () => {
        if (!deleteRequest) return;
        
        try {
            await protectAction(async (pwd) => {
                await contactService.deleteContact(deleteRequest, pwd || undefined);
            });
            triggerDebouncedChannelMembersSync(1500);
            await loadContacts();
            logger.info("Contact deleted");
        } catch (err) {
            logger.error("Failed to delete contact: " + stringifyError(err));
        } finally {
            setDeleteRequest(null);
        }
    };

    const handleSetMaxHops = async (hops: number) => {
        const currentHops = wotSummary?.ownListMaxHops ?? 2;
        if (isUpdatingHops || hops === currentHops) return;
        setIsUpdatingHops(true);
        try {
            const updated = await protectAction(async (pwd) => {
                return await wotService.setMaxHops(hops, pwd || null);
            });
            if (updated) {
                setWotSummary(updated);
            }
            await loadContacts();
        } catch (err) {
            logger.error("Failed to update max hops: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsUpdatingHops(false);
        }
    };

    const handleExportOpenWot = async () => {
        setIsExporting(true);
        try {
            const bytes = await protectAction(async (walletPassword) => {
                if (exportMode === 'contacts') {
                    return await wotService.exportPackage({
                        shareWithAllContacts: true,
                        password: walletPassword || null,
                    });
                } else {
                    return await wotService.exportPackage({
                        containerPassword: exportPassword.trim() || null,
                        password: walletPassword || null,
                    });
                }
            });
            setIsExportModalOpen(false);
            setExportPassword('');
            if (bytes && bytes.length > 0) {
                const settings = await settingsService.getSettings().catch(() => null);
                const filePath = await save({
                    defaultPath: settings?.lastUsedDirectory
                        ? `${settings.lastUsedDirectory}/trust-network.wot`
                        : 'trust-network.wot',
                    filters: [{ name: t('transfer.importFile.wotFilter'), extensions: ['wot'] }]
                });
                if (filePath) {
                    await writeFile(filePath, new Uint8Array(bytes));
                    if (settings) {
                        await updateLastUsedDirectory(filePath, settings, protectAction).catch(() => {});
                    }
                    setFeedbackMsg({ type: 'success', message: t('contacts.exportWotSuccess') });
                    await loadContacts();
                }
            }
        } catch (err) {
            logger.error("Failed to export WoT package: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsExporting(false);
        }
    };

    const handleShareContactWot = async (contact: Contact) => {
        try {
            const bytes = await protectAction(async (walletPassword) => {
                return await wotService.exportPackage({
                    recipientDid: contact.did,
                    password: walletPassword || null,
                });
            });
            if (bytes && bytes.length > 0) {
                const settings = await settingsService.getSettings().catch(() => null);
                const safeName = (contact.profile.firstName || t('contacts.defaultContactName', 'contact')).toLowerCase().replace(/\s+/g, '-');
                const filePath = await save({
                    defaultPath: settings?.lastUsedDirectory
                        ? `${settings.lastUsedDirectory}/trust-${safeName}.wot`
                        : `trust-${safeName}.wot`,
                    filters: [{ name: t('transfer.importFile.wotFilter'), extensions: ['wot'] }]
                });
                if (filePath) {
                    await writeFile(filePath, new Uint8Array(bytes));
                    if (settings) {
                        await updateLastUsedDirectory(filePath, settings, protectAction).catch(() => {});
                    }
                    setFeedbackMsg({ type: 'success', message: t('contacts.exportWotSuccess') });
                    await loadContacts();
                }
            }
        } catch (err) {
            logger.error("Failed to export contact WoT package: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        }
    };

    const executeImportWot = async (fileBytes: number[], password?: string) => {
        setIsImportingWot(true);
        try {
            const result = await protectAction(async (walletPassword) => {
                return await wotService.importPackage({
                    containerBytes: fileBytes,
                    containerPassword: password || null,
                    password: walletPassword || null,
                });
            });
            setIsWotPasswordModalOpen(false);
            setPendingWotImportBytes(null);
            setWotContainerPassword('');
            if (result) {
                triggerDebouncedChannelMembersSync(1500);
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
            const isPasswordRequired = errorStr.includes("Password required") || errorStr.toLowerCase().includes("decrypt") || errorStr.includes("SymmetricEncryption") || errorStr.includes("MacError");
            if (isPasswordRequired) {
                setPendingWotImportBytes(fileBytes);
                setIsWotPasswordModalOpen(true);
                return;
            }
            logger.error("Failed to import WoT package: " + errorStr);
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsImportingWot(false);
        }
    };

    const handleImportWot = async () => {
        try {
            const settings = await settingsService.getSettings().catch(() => null);
            const selectedPath = await open({
                multiple: false,
                defaultPath: settings?.lastUsedDirectory,
                filters: [{ name: t('transfer.importFile.wotFilter'), extensions: ['wot'] }]
            });
            if (typeof selectedPath === 'string') {
                if (settings) {
                    await updateLastUsedDirectory(selectedPath, settings, protectAction).catch(() => {});
                }
                const fileUint8Array = await readFile(selectedPath);
                const fileBytes = Array.from(fileUint8Array);
                await executeImportWot(fileBytes);
            }
        } catch (err) {
            logger.error("Failed to read WoT file: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleSetProviderTrust = async (issuerDid: string, level: number) => {
        try {
            await protectAction(async (pwd) => {
                await wotService.setProviderTrust(issuerDid, level, pwd || null);
            });
            triggerDebouncedChannelMembersSync(1500);
            await loadContacts();
        } catch (err) {
            logger.error("Failed to set provider trust: " + stringifyError(err));
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
            logger.error("Failed to refresh provider: " + stringifyError(err));
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
            triggerDebouncedChannelMembersSync(1500);
            setFeedbackMsg({ type: 'success', message: t('contacts.providerRemovedSuccess') });
            await loadContacts();
        } catch (err) {
            logger.error("Failed to remove provider: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setDeleteProviderRequest(null);
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
                })
            ]);
            if (res) {
                checkAndTriggerCooldown(res);
            }
            if (postRes) {
                checkAndTriggerCooldown(postRes);
            }
            const updatedOverview = await marketService.getSyncOverviewStatus().catch(() => null);
            if (updatedOverview) {
                setSyncOverview(updatedOverview);
                notifySyncStatusUpdated(updatedOverview);
            } else {
                notifySyncStatusUpdated();
            }
            await loadContacts();

            const allErrors: string[] = [];
            if (res?.errors && res.errors.length > 0) {
                allErrors.push(...res.errors);
            }
            if (postRes?.errors && postRes.errors.length > 0) {
                allErrors.push(...postRes.errors);
            }

            if (allErrors.length > 0) {
                const has429 = allErrors.some(e => e.includes('429') || e.toLowerCase().includes('rate limit'));
                const errorDetail = allErrors.join(', ');
                const errorMsg = has429
                    ? `${t('contacts.syncServicesErrors', 'Fehler beim Synchronisieren:')} Rate-Limit erreicht (HTTP 429). Bitte später erneut versuchen.`
                    : `${t('contacts.syncServicesErrors', 'Fehler beim Synchronisieren:')} ${errorDetail}`;
                setFeedbackMsg({ type: 'error', message: errorMsg });
            } else {
                const totalConnected = updatedOverview?.connectedCount ?? syncOverview?.connectedCount ?? 0;
                const totalPolled = (res?.polledSyncs ?? 0) + (postRes?.polledSyncs ?? 0);
                const totalUploaded = (res?.uploadedTo ?? 0) + (postRes?.uploadedTo ?? 0);
                const hasActivity = totalConnected > 0 || totalPolled > 0 || totalUploaded > 0 || (res?.importedWotLists ?? 0) > 0 || (res?.importedCards ?? 0) > 0;

                if (!hasActivity) {
                    setFeedbackMsg({
                        type: 'warning',
                        message: t(
                            'contacts.syncServicesNoneConfigured',
                            'Kein Sync-Dienst eingerichtet. Richte zuerst einen Sync-Dienst ein, um Vertrauenslisten und Angebote abzugleichen.'
                        ),
                        action: {
                            label: t('contacts.connectSyncServiceBtn', 'Sync-Dienst verbinden'),
                            onClick: handleNavigateToSyncSettings,
                        },
                    });
                } else {
                    const msg = t('contacts.syncServicesSuccess', 'Sync-Dienste synchronisiert: {{wot}} Vertrauensliste(n), {{cards}} Angebot(e).', {
                        wot: res?.importedWotLists ?? 0,
                        cards: res?.importedCards ?? 0,
                    });
                    setFeedbackMsg({ type: 'success', message: msg });
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
                }
            }
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
            const res = await protectAction(async (pwd) => {
                return await postService.acceptWotHandshake(handshake.msgId, level, returnHandshake, pwd || undefined);
            });
            if (res) {
                setFeedbackMsg({
                    type: 'success',
                    message: t('contacts.handshakeAcceptedSuccess', 'Vertrauensbestätigung akzeptiert')
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
            setPendingHandshakes(prev => prev.filter(h => h.msgId !== msgId));
        } catch (e) {
            setFeedbackMsg({ type: 'error', message: translateError(e, t) });
        }
    };

    const allTags = Array.from(new Set(contacts.flatMap(c => c.tags)));

    const filteredContacts = contacts.filter(contact => {
        const matchesSearch = 
            (contact.profile.firstName || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.lastName || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            (contact.profile.organization || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
            contact.did.toLowerCase().includes(searchQuery.toLowerCase());
        
        const matchesTag = !selectedTag || contact.tags.includes(selectedTag);
        
        return matchesSearch && matchesTag;
    });

    const currentMaxHops = wotSummary?.ownListMaxHops ?? 2;

    return (
        <PageLayout 
            title={t('contacts.title')} 
            onBack={onBack || navigation?.goBack}
            description={t('contacts.description')} 
            maxWidth="6xl"
            optionsPanel={
                isNetworkOptionsOpen ? (
                    <WotNetworkOptionsPanel
                        wotSummary={wotSummary}
                        currentMaxHops={currentMaxHops}
                        isUpdatingHops={isUpdatingHops}
                        hasSyncService={hasSyncService}
                        protectAction={protectAction}
                        onSelectMaxHops={handleSetMaxHops}
                        onRefreshWot={() => { void loadContacts(); }}
                        onOpenTrustListModal={() => setIsTrustListModalOpen(true)}
                        onNavigateToSyncSettings={handleNavigateToSyncSettings}
                        onClose={() => setIsNetworkOptionsOpen(false)}
                    />
                ) : null
            }
            actions={
                <>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => setIsQuickContactModalOpen(true)}
                        className="gap-2"
                        title={t('contacts.shareMyProfileBtn', 'Mein Profil teilen')}
                        data-testid="my-quick-contact-button"
                    >
                        <QrCode size={18} />
                        <span>
                            {t('contacts.shareMyProfileBtn', 'Mein Profil teilen')}
                        </span>
                    </Button>
                    <Button
                        size="sm"
                        onClick={() => {
                            setInitialAddDid(undefined);
                            setInitialAddProfile(undefined);
                            setInitialAddSyncUrls(undefined);
                            setIsAddContactModalOpen(true);
                        }}
                        className="gap-2"
                        title={t('contacts.addContact')}
                    >
                        <UserPlus size={18} />
                        <span>{t('contacts.addContact')}</span>
                    </Button>
                </>
            }
            advancedActions={
                <>
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={handleSyncServices}
                        disabled={isSyncingServices || isCoolingDown}
                        className="gap-1.5"
                        title={isCoolingDown
                            ? t('contacts.syncRateLimitWait', { seconds: cooldownSeconds, defaultValue: `Bitte noch ${cooldownSeconds}s warten...` })
                            : isSyncingServices
                            ? t('settings.syncing', 'Synchronisiere...')
                            : t('contacts.syncServicesButton', 'Über Sync-Dienste synchronisieren')}
                        aria-label={isCoolingDown
                            ? t('contacts.syncRateLimitWait', { seconds: cooldownSeconds, defaultValue: `Bitte noch ${cooldownSeconds}s warten...` })
                            : isSyncingServices
                            ? t('settings.syncing', 'Synchronisiere...')
                            : t('contacts.syncServicesButton', 'Über Sync-Dienste synchronisieren')}
                        data-testid="sync-services-button"
                    >
                        <RefreshCw size={16} className={isSyncingServices ? 'animate-spin' : ''} />
                        <span>
                            {isCoolingDown
                                ? `${cooldownSeconds}s`
                                : isSyncingServices
                                    ? t('settings.syncing', 'Synchronisiere...')
                                    : t('contacts.refreshSyncBtn', 'Aktualisieren')}
                        </span>
                    </Button>
                    <Button
                        type="button"
                        variant={isNetworkOptionsOpen ? 'primary' : 'secondary'}
                        size="sm"
                        onClick={() => setIsNetworkOptionsOpen(prev => !prev)}
                        className="gap-1.5"
                        title={t('contacts.wotCustomizeOptions', 'Netzwerk-Optionen anpassen')}
                        aria-label={t('contacts.wotCustomizeOptions', 'Netzwerk-Optionen anpassen')}
                        data-testid="toggle-network-options-button"
                    >
                        <SlidersHorizontal size={16} />
                        <span>{t('contacts.wotNetworkAndReachBtn', 'Netzwerk & Reichweite')}</span>
                        {isNetworkOptionsOpen ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </Button>
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => setIsGuardianSetupOpen(true)}
                        className="gap-2"
                        title={t('backup.manageGuardiansButton')}
                        data-testid="manage-guardians-btn"
                    >
                        <ShieldCheck size={18} />
                        <span>{t('backup.socialRecoveryStatusLabel')}</span>
                    </Button>
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => window.dispatchEvent(new CustomEvent('humoco:open-guardian-release'))}
                        className="gap-2"
                        title={t('backup.helpContactRecoveryBtn')}
                        data-testid="open-guardian-release-btn"
                    >
                        <UserCheck size={18} />
                        <span>{t('backup.helpContactRecoveryBtn')}</span>
                    </Button>
                </>
            }
        >
            <div className="space-y-8">
                {feedbackMsg && (
                    <AlertBanner
                        variant={feedbackMsg.type}
                        onDismiss={() => setFeedbackMsg(null)}
                        action={feedbackMsg.action ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="secondary"
                                onClick={feedbackMsg.action.onClick}
                                className="shrink-0 bg-white/80 hover:bg-white text-theme-primary font-bold shadow-xs border-amber-300/60"
                            >
                                {feedbackMsg.action.label}
                            </Button>
                        ) : undefined}
                    >
                        {feedbackMsg.message}
                    </AlertBanner>
                )}

                {/* Incoming Pending Handshakes */}
                {pendingHandshakes.length > 0 && (
                    <div className="p-4 sm:p-5 bg-sky-50/80 border border-sky-200 rounded-3xl space-y-4" data-testid="pending-handshakes-section">
                        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                            <div className="flex items-center gap-3">
                                <div className="w-10 h-10 rounded-2xl bg-sky-100 border border-sky-200 text-sky-700 flex items-center justify-center shrink-0">
                                    <Handshake size={20} />
                                </div>
                                <div>
                                    <div className="flex items-center gap-2">
                                        <h4 className="text-sm font-extrabold text-sky-950">
                                            {t('contacts.incomingHandshakesTitle', 'Eingehende Vertrauensbestätigungen')}
                                        </h4>
                                        <span className="px-2 py-0.5 rounded-full text-xs font-black bg-sky-600 text-white">
                                            {pendingHandshakes.length}
                                        </span>
                                    </div>
                                    <p className="text-xs text-sky-900/80 mt-0.5">
                                        {pendingHandshakes.length === 1
                                            ? t('contacts.incomingHandshakesBanner', '1 Kontakt möchte dich als vertraut eintragen.')
                                            : t('contacts.incomingHandshakesBannerPlural', '{{count}} Kontakte möchten dich als vertraut eintragen.', { count: pendingHandshakes.length })}
                                    </p>
                                </div>
                            </div>
                            <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                onClick={() => setIsHandshakesExpanded(!isHandshakesExpanded)}
                                className="self-start sm:self-auto bg-white border-sky-200 text-sky-900 font-bold"
                                data-testid="toggle-handshakes-button"
                            >
                                <span>{isHandshakesExpanded ? t('contacts.hideHandshakesBtn', 'Ausblenden') : t('contacts.viewHandshakesBtn', 'Ansehen')}</span>
                                {isHandshakesExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                            </Button>
                        </div>

                        {isHandshakesExpanded && (
                            <div className="space-y-4 pt-2 border-t border-sky-200/60 animate-in fade-in duration-200">
                                {pendingHandshakes.map((h) => {
                                    const name = h.senderName || truncateUserId(h.senderDid);
                                    const currentLevel = handshakeAcceptedLevels[h.msgId] ?? (h.proposedLevel <= 3 ? h.proposedLevel : 1);
                                    const returnChecked = handshakeReturnFlags[h.msgId] ?? h.replyRequested ?? true;
                                    const isAccepting = isAcceptingHandshake === h.msgId;

                                    return (
                                        <div
                                            key={h.msgId}
                                            className="p-5 bg-white rounded-2xl border border-sky-100 shadow-xs space-y-4"
                                            data-testid={`handshake-item-${h.msgId}`}
                                        >
                                            <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-3">
                                                <div className="space-y-1">
                                                    <p className="font-extrabold text-sm text-theme-primary">
                                                        {t('contacts.handshakeProposalPhrase', '{{name}} möchte euch gegenseitig als vertraut eintragen.', { name })}
                                                    </p>
                                                    <TechId value={h.senderDid} kind="did" />
                                                </div>
                                            </div>

                                            <div className="space-y-2">
                                                <label className="block text-xs font-bold text-theme-secondary">
                                                    {t('contacts.acceptAsLevel', 'Wie möchtest du diesen Kontakt einstufen?')}
                                                </label>
                                                <TrustLevelRadioCards
                                                    value={currentLevel}
                                                    onChange={(lvl) => setHandshakeAcceptedLevels((prev) => ({ ...prev, [h.msgId]: lvl ?? 1 }))}
                                                    allowUnrated={false}
                                                    idPrefix={`handshake-${h.msgId}`}
                                                />
                                            </div>

                                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pt-2 border-t border-slate-100">
                                                <label className="flex items-center gap-2 text-xs font-medium text-theme-secondary cursor-pointer">
                                                    <input
                                                        type="checkbox"
                                                        checked={returnChecked}
                                                        onChange={(e) => setHandshakeReturnFlags((prev) => ({ ...prev, [h.msgId]: e.target.checked }))}
                                                        className="rounded accent-sky-600"
                                                    />
                                                    <span>{t('contacts.sendReturnHandshakeLabel', 'Gegenseitige Vertrauensbestätigung zurücksenden')}</span>
                                                </label>

                                                <div className="flex items-center gap-2 self-end sm:self-auto">
                                                    <Button
                                                        size="xs"
                                                        variant="outline"
                                                        onClick={() => handleDismissHandshake(h.msgId)}
                                                        data-testid="dismiss-handshake-button"
                                                        className="text-theme-light hover:text-rose-600"
                                                    >
                                                        {t('common.dismiss', 'Verwerfen')}
                                                    </Button>
                                                    <Button
                                                        size="xs"
                                                        variant="primary"
                                                        disabled={isAccepting}
                                                        onClick={() => handleAcceptHandshake(h)}
                                                        data-testid="accept-handshake-button"
                                                        className="bg-sky-600 hover:bg-sky-700 text-white gap-1"
                                                    >
                                                        {isAccepting ? <RefreshCw size={12} className="animate-spin" /> : <Check size={12} />}
                                                        <span>{t('common.confirm', 'Bestätigen')}</span>
                                                    </Button>
                                                </div>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                )}

                {/* Progressive Disclosure Tab Bar */}
                {providers.length > 0 && (
                    <Advanced>
                        <div className="flex justify-start">
                            <SegmentedTabs
                                tabs={[
                                    {
                                        id: 'contacts',
                                        label: `${t('contacts.tabPersonalContacts')} (${contacts.length})`,
                                        icon: <Users size={16} />
                                    },
                                    {
                                        id: 'providers',
                                        label: `${t('contacts.tabProviders')} (${providers.length})`,
                                        icon: <Building2 size={16} />
                                    }
                                ]}
                                activeTab={activeTab}
                                onChange={(id) => setActiveTab(id as 'contacts' | 'providers')}
                            />
                        </div>
                    </Advanced>
                )}

                {(() => {
                    const providersSection = (
                        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                        {providers.map((provider) => (
                            <div key={provider.issuerDid} className="group h-full">
                                <div className="h-full bg-white border border-theme-subtle rounded-[32px] p-6 flex flex-col group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                                    <div className="flex items-start justify-between mb-4">
                                        <div className="w-14 h-14 rounded-[20px] bg-theme-primary/5 border-2 border-theme-subtle flex items-center justify-center text-theme-primary shrink-0 shadow-inner">
                                            <Building2 size={28} />
                                        </div>
                                        <div className="flex gap-1">
                                            {provider.sourceUrl && (
                                                <button
                                                    onClick={() => handleRefreshProvider(provider)}
                                                    disabled={refreshingProviderDid === provider.issuerDid}
                                                    className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer disabled:opacity-50"
                                                    title={t('contacts.providerRefreshButton')}
                                                >
                                                    <RefreshCw
                                                        size={18}
                                                        className={refreshingProviderDid === provider.issuerDid ? 'animate-spin text-theme-primary' : ''}
                                                    />
                                                </button>
                                            )}
                                            <button
                                                onClick={() => setDeleteProviderRequest(provider)}
                                                className="p-2 hover:bg-rose-50 rounded-xl text-theme-light hover:text-rose-500 transition-all cursor-pointer"
                                                title={t('common.remove')}
                                            >
                                                <Trash2 size={18} />
                                            </button>
                                        </div>
                                    </div>

                                    <div className="flex-1 flex flex-col">
                                        <h3 className="text-lg font-black text-theme-primary tracking-tight leading-tight group-hover:text-theme-accent transition-colors">
                                            {provider.name}
                                        </h3>

                                        <Advanced>
                                            <div className="flex items-center gap-1.5 mt-2 mb-3">
                                                <TechId value={provider.issuerDid} kind="did" />
                                            </div>
                                        </Advanced>

                                        <div className="flex items-center gap-1.5 flex-wrap mb-4">
                                            <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-slate-100 text-slate-700 border border-slate-200">
                                                {t('contacts.providerMembers', { count: provider.memberCount })}
                                            </span>
                                            <span className="text-[10px] font-medium text-theme-light/80">
                                                {new Date(provider.issuedAt * 1000).toLocaleDateString()}
                                            </span>
                                            {provider.isStale && (
                                                <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-amber-50 text-amber-800 border border-amber-200">
                                                    {t('contacts.providerStaleBadge')}
                                                </span>
                                            )}
                                        </div>

                                        {/* Trust level selector */}
                                        <div className="space-y-1.5 pt-3 border-t border-theme-subtle/40 mt-auto">
                                            <label className="block text-[10px] font-black uppercase tracking-widest text-theme-light">
                                                {t('contacts.trustSectionLabel')}
                                            </label>
                                            <div className="grid grid-cols-4 gap-1">
                                                {[
                                                    { level: 0, labelKey: 'contacts.providerTrustInactive' },
                                                    { level: 1, labelKey: 'contacts.trustLevel1' },
                                                    { level: 2, labelKey: 'contacts.trustLevel2' },
                                                    { level: 3, labelKey: 'contacts.trustLevel3' },
                                                ].map((opt) => {
                                                    const isSelected = provider.trustLevel === opt.level;
                                                    return (
                                                        <button
                                                            key={opt.level}
                                                            type="button"
                                                            onClick={() => handleSetProviderTrust(provider.issuerDid, opt.level)}
                                                            className={`py-1.5 px-1 rounded-lg text-[10px] font-black uppercase tracking-wider border text-center transition-all cursor-pointer ${
                                                                isSelected
                                                                    ? 'bg-theme-secondary border-theme-secondary text-white shadow-sm'
                                                                    : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                                            }`}
                                                        >
                                                            {t(opt.labelKey)}
                                                        </button>
                                                    );
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        ))}
                        </div>
                    );
                    const contactsSection = (
                        <>
                            {/* Search and Filters */}
                        <div className="flex flex-col md:flex-row gap-6">
                            <div className="relative flex-1 group">
                                <div className="absolute left-4 top-1/2 -translate-y-1/2 text-theme-light group-focus-within:text-theme-primary transition-colors">
                                    <Search size={18} />
                                </div>
                                <input
                                    type="text"
                                    placeholder={t('contacts.searchPlaceholder')}
                                    value={searchQuery}
                                    onChange={(e) => setSearchQuery(e.target.value)}
                                    className="w-full bg-white/50 backdrop-blur-sm border border-theme-subtle rounded-2xl pl-12 pr-10 py-4 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/10 transition-all font-medium shadow-inner-soft"
                                />
                                {searchQuery && (
                                    <button
                                        onClick={() => setSearchQuery('')}
                                        className="absolute right-3 top-1/2 -translate-y-1/2 text-theme-light hover:text-rose-500 hover:bg-rose-50 rounded-full p-1 transition-colors"
                                        title={t('contacts.clearSearch')}
                                    >
                                        <X size={16} />
                                    </button>
                                )}
                            </div>

                            <div className="flex items-center gap-2 overflow-x-auto pb-2 no-scrollbar scroll-smooth px-1">
                                <button
                                    onClick={() => setSelectedTag(null)}
                                    className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                                        !selectedTag
                                            ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20'
                                            : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                    }`}
                                >
                                    {t('contacts.filterAll')}
                                </button>
                                {allTags.map(tag => (
                                    <button
                                        key={tag}
                                        onClick={() => setSelectedTag(tag)}
                                        className={`px-4 py-2.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all whitespace-nowrap border ${
                                            selectedTag === tag
                                                ? 'bg-theme-secondary border-theme-secondary text-white shadow-lg shadow-theme-secondary/20'
                                                : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                        }`}
                                    >
                                        {tag}
                                    </button>
                                ))}
                            </div>
                        </div>

                        {/* Contact List */}
                        {isLoading ? (
                            <PageLoadingState message={t('contacts.loading')} />
                        ) : contacts.length === 0 ? (
                            /* Empty State 1: No contacts at all */
                            <PageEmptyState
                                icon={<Users size={32} />}
                                title={t('contacts.emptyNoContactsTitle', 'Noch keine Kontakte')}
                                description={t('contacts.emptyNoContactsDesc', 'Noch keine Kontakte. Füge die erste Person hinzu, der du vertraust.')}
                                action={
                                    <Button
                                        onClick={() => {
                                            setInitialAddDid(undefined);
                                            setInitialAddProfile(undefined);
                                            setIsAddContactModalOpen(true);
                                        }}
                                        className="gap-2"
                                    >
                                        <UserPlus size={18} />
                                        <span>{t('contacts.addContact', 'Kontakt hinzufügen')}</span>
                                    </Button>
                                }
                            />
                        ) : filteredContacts.length === 0 ? (
                            /* Empty State 2: Filter/search without match */
                            <PageEmptyState
                                icon={<Search size={32} />}
                                title={t('contacts.emptyNoMatchesTitle', 'Keine Treffer')}
                                description={
                                    searchQuery
                                        ? t('contacts.emptyNoMatchesDesc', { query: searchQuery, defaultValue: `Keine Treffer für „${searchQuery}“.` })
                                        : t('contacts.emptyNoMatchesFilterDesc', 'Keine Kontakte für diesen Filter gefunden.')
                                }
                                action={
                                    <Button
                                        variant="secondary"
                                        onClick={() => {
                                            setSearchQuery('');
                                            setSelectedTag(null);
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
                                {filteredContacts.map(contact => (
                                    <div key={contact.did} className="group h-full">
                                        <div className="h-full bg-white border border-theme-subtle rounded-[32px] p-6 flex flex-col group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                                            <div className="flex items-start justify-between mb-4">
                                                <div className="relative">
                                                    <div className="w-16 h-16 rounded-[24px] overflow-hidden border-2 border-theme-subtle group-hover:border-theme-primary/30 transition-all p-1 bg-white shadow-inner flex items-center justify-center">
                                                        <UserAvatar
                                                            size={56}
                                                            rounded="2xl"
                                                            pictureUrl={contact.profile.pictureUrl}
                                                            name={[contact.profile.firstName, contact.profile.lastName].filter(Boolean).join(' ') || contact.profile.organization || contact.did}
                                                        />
                                                    </div>
                                                    <div className="absolute -bottom-1 -right-1 bg-white p-1 rounded-full shadow-sm border border-theme-subtle">
                                                        <div className="bg-emerald-500 w-3 h-3 rounded-full border-2 border-white"></div>
                                                    </div>
                                                </div>
                                                <div className="flex gap-1 opacity-100 md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100 transition-opacity">
                                                    <Advanced>
                                                        <button
                                                            onClick={() => handleShareContactWot(contact)}
                                                            className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                                                            title={t('contacts.shareWotForContact')}
                                                        >
                                                            <Share2 size={18} />
                                                        </button>
                                                    </Advanced>
                                                    <button
                                                        onClick={() => {
                                                             setEditingContact(contact);
                                                             setIsDialogOpen(true);
                                                         }}
                                                         className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                                                         title={t('contacts.editProfile')}
                                                    >
                                                         <Edit3 size={18} />
                                                    </button>
                                                    <button
                                                        onClick={() => setDeleteRequest(contact.did)}
                                                        className="p-2 hover:bg-rose-50 rounded-xl text-theme-light hover:text-rose-500 transition-all cursor-pointer"
                                                        title={t('contacts.removeContact')}
                                                    >
                                                         <Trash2 size={18} />
                                                    </button>
                                                </div>
                                            </div>

                                            <div className="flex-1">
                                                <h3 className="text-lg font-black text-theme-primary tracking-tight leading-tight group-hover:text-theme-accent transition-colors">
                                                    {contact.profile.firstName || contact.profile.lastName
                                                        ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim()
                                                        : contact.profile.organization || t('contacts.unnamedIdentity')}
                                                </h3>

                                                <Advanced>
                                                    <div className="flex items-center gap-1.5 mt-2 mb-4">
                                                        <TechId value={contact.did} kind="did" />
                                                    </div>
                                                </Advanced>

                                                {(contact.profile.organization || contact.profile.community) && (
                                                    <div className="flex items-center gap-2 mb-4 p-2.5 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                                        <Building2 size={14} className="text-theme-light" />
                                                        <span className="text-xs font-bold text-theme-secondary truncate">
                                                            {contact.profile.organization || contact.profile.community}
                                                        </span>
                                                    </div>
                                                )}

                                                {/* Trust Rating & Synced Peer List Pills */}
                                                <div className="flex items-center gap-1.5 flex-wrap mb-3">
                                                    {guardianDids.has(contact.did) && (
                                                        <span
                                                            title={t('backup.addressBookGuardianBadge')}
                                                            className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm bg-purple-50 text-purple-800 border-purple-200 flex items-center gap-1"
                                                        >
                                                            <ShieldCheck size={10} />
                                                            {t('backup.socialRecoveryStatusLabel')}
                                                        </span>
                                                    )}
                                                    {guardianWardDids.has(contact.did) && (
                                                        <button
                                                            type="button"
                                                            data-testid={`open-ward-release-${contact.did}`}
                                                            onClick={() =>
                                                                window.dispatchEvent(
                                                                    new CustomEvent('humoco:open-guardian-release', {
                                                                        detail: { ownerDid: contact.did },
                                                                    })
                                                                )
                                                            }
                                                            title={t('backup.guardianWardBadgeHint')}
                                                            className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm bg-sky-50 hover:bg-sky-100 text-sky-800 border-sky-200 flex items-center gap-1 cursor-pointer transition-colors"
                                                        >
                                                            <UserCheck size={10} />
                                                            {t('backup.addressBookOpenRelease')}
                                                        </button>
                                                    )}
                                                    <span
                                                        title={t('contacts.trustLevelHint')}
                                                        className={`px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm ${
                                                        contact.trustLevel === 3
                                                            ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                                            : contact.trustLevel === 2
                                                                ? 'bg-teal-50 text-teal-800 border-teal-200'
                                                                : contact.trustLevel === 1
                                                                    ? 'bg-amber-50 text-amber-800 border-amber-200'
                                                                    : contact.trustLevel === 0
                                                                        ? 'bg-sky-50 text-sky-800 border-sky-200'
                                                                        : 'bg-slate-50 text-slate-500 border-slate-200'
                                                    }`}>
                                                        {contact.trustLevel === 3
                                                            ? t('contacts.trustLevel3')
                                                            : contact.trustLevel === 2
                                                                ? t('contacts.trustLevel2')
                                                                : contact.trustLevel === 1
                                                                    ? t('contacts.trustLevel1')
                                                                    : contact.trustLevel === 0
                                                                        ? t('contacts.trustLevel0')
                                                                        : t('contacts.trustLevelUnrated')}
                                                    </span>
                                                    <Advanced>
                                                        {contact.hasPeerList && (
                                                            contact.peerListUpdatedAt && (Date.now() / 1000 - contact.peerListUpdatedAt) > 31_536_000 ? (
                                                                <span
                                                                    title={t('contacts.peerListStaleHint')}
                                                                    className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-amber-50 text-amber-700 border border-amber-200 shadow-sm"
                                                                >
                                                                    {t('contacts.peerListStale')}
                                                                </span>
                                                            ) : (
                                                                <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-indigo-50 text-indigo-700 border border-indigo-200 shadow-sm">
                                                                    {t('contacts.peerListSynced')}
                                                                </span>
                                                            )
                                                        )}
                                                        {contact.syncs && contact.syncs.length > 0 && (
                                                            <div
                                                                data-testid="contact-has-sync-badge"
                                                                className="flex flex-wrap items-center gap-1.5"
                                                            >
                                                                <span
                                                                    className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-emerald-50 text-emerald-700 border border-emerald-200 shadow-sm flex items-center gap-1"
                                                                    title={t('contacts.hasSyncTooltip')}
                                                                >
                                                                    <Server size={9} />
                                                                    {t('contacts.hasSync')}
                                                                </span>
                                                                {contact.syncs.map((syncEp, idx) => {
                                                                    const host = (() => {
                                                                        try {
                                                                            return new URL(syncEp.writeUrl || syncEp.readUrl).hostname;
                                                                        } catch {
                                                                            return syncEp.writeUrl || syncEp.readUrl;
                                                                        }
                                                                    })();
                                                                    const isPendingAuth = syncEp.status === 'pending_auth';
                                                                    const isFailing = syncEp.status === 'unreachable' || (!isPendingAuth && (syncEp.failCount ?? 0) > 0);
                                                                    const isActive = syncEp.status === 'active';

                                                                    let badgeColor = 'bg-slate-50 text-slate-600 border-slate-200';
                                                                    let dotColor = 'bg-slate-400';
                                                                    let statusLabel = t('contacts.syncUnverified', 'Ungeprüft');
                                                                    let tooltip = `${host} (${statusLabel})`;

                                                                    if (isFailing) {
                                                                        const isHardUnreachable = syncEp.status === 'unreachable' || (syncEp.failCount && syncEp.failCount >= 3);
                                                                        badgeColor = isHardUnreachable
                                                                            ? 'bg-rose-50 text-rose-700 border-rose-200'
                                                                            : 'bg-amber-50 text-amber-700 border-amber-200';
                                                                        dotColor = isHardUnreachable ? 'bg-rose-500' : 'bg-amber-500';
                                                                        statusLabel = isHardUnreachable ? t('contacts.syncUnreachable') : t('contacts.syncWarning');
                                                                        tooltip = `${host} - ${statusLabel} (${t('contacts.syncFailCount')} ${syncEp.failCount ?? 0})`;
                                                                    } else if (isPendingAuth) {
                                                                        badgeColor = 'bg-amber-50 text-amber-700 border-amber-200';
                                                                        dotColor = 'bg-amber-500';
                                                                        statusLabel = t('contacts.syncPendingAuth', 'Erreichbar, nicht freigeschaltet');
                                                                        tooltip = `${host} - ${statusLabel}`;
                                                                    } else if (isActive) {
                                                                        badgeColor = 'bg-emerald-50 text-emerald-700 border-emerald-200';
                                                                        dotColor = 'bg-emerald-500';
                                                                        statusLabel = t('contacts.syncAvailable', 'Abgleich bereit');
                                                                        const dateStr = syncEp.lastSeenAt ? new Date(syncEp.lastSeenAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
                                                                        tooltip = `${host} - ${statusLabel}${dateStr ? ` (${t('contacts.syncLastSeen')} ${dateStr})` : ''}`;
                                                                    } else {
                                                                        badgeColor = 'bg-slate-50 text-slate-600 border-slate-200';
                                                                        dotColor = 'bg-slate-400';
                                                                        statusLabel = t('contacts.syncUnverified', 'Ungeprüft');
                                                                        tooltip = `${host} (${statusLabel})`;
                                                                    }

                                                                    return (
                                                                        <span
                                                                            key={`${syncEp.writeUrl || syncEp.readUrl}-${idx}`}
                                                                            className={`px-1.5 py-0.5 rounded-md text-[9px] font-semibold border shadow-sm flex items-center gap-1 ${badgeColor}`}
                                                                            title={tooltip}
                                                                        >
                                                                            <span className={`w-1.5 h-1.5 rounded-full ${dotColor}`} />
                                                                            <span className="font-mono">{host}</span>
                                                                            <span className="opacity-75 font-normal">({statusLabel})</span>
                                                                        </span>
                                                                    );
                                                                })}
                                                            </div>
                                                        )}
                                                    </Advanced>
                                                </div>

                                                <div className="flex flex-wrap gap-1.5 mt-auto">
                                                    {contact.tags.slice(0, 1).map(tag => (
                                                        <span key={tag} className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm">
                                                            <Tag size={8} />
                                                            {tag}
                                                        </span>
                                                    ))}
                                                    <Advanced>
                                                        {contact.tags.slice(1).map(tag => (
                                                            <span key={tag} className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm">
                                                                <Tag size={8} />
                                                                {tag}
                                                            </span>
                                                        ))}
                                                        {contact.tags.length === 0 && (
                                                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light/40 italic">{t('contacts.noTagsAssigned')}</span>
                                                        )}
                                                    </Advanced>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                        </>
                    );
                    return (
                        <ExpertTabsLayout
                            simpleContent={contactsSection}
                            expertContent={activeTab === 'providers' && providers.length > 0 ? providersSection : contactsSection}
                        />
                    );
                })()}

            </div>

            {/* Add Contact Modal with 4 tiles */}
            <AddContactModal
                isOpen={isAddContactModalOpen}
                onClose={() => setIsAddContactModalOpen(false)}
                onOpenManualAdd={(initialDid, initialProfile, initialSyncUrls) => {
                    setEditingContact(null);
                    setInitialAddDid(initialDid);
                    setInitialAddProfile(initialProfile);
                    setInitialAddSyncUrls(initialSyncUrls);
                    setIsDialogOpen(true);
                }}
                onContactAdded={loadContacts}
                onSuccess={(msg) => setFeedbackMsg({ type: 'success', message: msg })}
                onError={(msg) => setFeedbackMsg({ type: 'error', message: msg })}
            />

            {/* Unified Profile Share Modal */}
            <QuickContactModal
                isOpen={isQuickContactModalOpen}
                onClose={() => setIsQuickContactModalOpen(false)}
            />

            {/* Trust List Menu Modal ("⋯") */}
            <ModalShell
                isOpen={isTrustListModalOpen}
                onClose={() => setIsTrustListModalOpen(false)}
                maxWidth="md"
                title={t('contacts.trustListMenuBtn', 'Vertrauensliste')}
                description={t('contacts.trustListMenuDesc', 'Vertrauenslisten offline teilen oder einlesen')}
            >
                <div className="space-y-4 py-2">
                    <InfoHint topic="visibilityReach" variant="banner" />

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                        <button
                            type="button"
                            onClick={() => {
                                setIsTrustListModalOpen(false);
                                handleImportWot();
                            }}
                            className="p-4 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-2 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                        >
                            <div className="w-10 h-10 rounded-xl bg-theme-primary/5 text-theme-primary flex items-center justify-center shrink-0">
                                <Upload size={20} />
                            </div>
                            <div>
                                <h4 className="font-extrabold text-sm text-theme-primary">
                                    {t('contacts.importWotButton', 'Vertrauensliste einlesen')}
                                </h4>
                                <p className="text-xs text-theme-light mt-0.5 leading-relaxed">
                                    {t('contacts.trustListImportDesc', 'Eine empfangene Vertrauensliste importieren')}
                                </p>
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => {
                                setIsTrustListModalOpen(false);
                                setExportMode((wotSummary?.ratedContactsCount ?? 0) > 0 ? 'contacts' : 'password');
                                setExportPassword('');
                                setIsExportModalOpen(true);
                            }}
                            className="p-4 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-2 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                        >
                            <div className="w-10 h-10 rounded-xl bg-theme-primary/5 text-theme-primary flex items-center justify-center shrink-0">
                                <Share2 size={20} />
                            </div>
                            <div>
                                <h4 className="font-extrabold text-sm text-theme-primary">
                                    {t('contacts.exportWotButton', 'Vertrauensliste teilen')}
                                </h4>
                                <p className="text-xs text-theme-light mt-0.5 leading-relaxed">
                                    {t('contacts.trustListExportDesc', 'Deine Vertrauenseinstufungen als Datei exportieren')}
                                </p>
                            </div>
                        </button>
                    </div>
                </div>
            </ModalShell>

            {/* Manual Add / Edit Contact Dialog */}
            <ContactDialog
                isOpen={isDialogOpen}
                onClose={() => {
                    setIsDialogOpen(false);
                    setInitialAddDid(undefined);
                    setInitialAddProfile(undefined);
                    setInitialAddSyncUrls(undefined);
                }}
                onSave={handleSaveContact}
                existingContact={editingContact}
                initialDid={initialAddDid}
                initialProfile={initialAddProfile}
                initialSyncUrls={initialAddSyncUrls}
            />

            {/* Confirmation: Delete Contact */}
            <ConfirmationModal
                isOpen={!!deleteRequest}
                title={t('contacts.deleteTitle')}
                description={t('contacts.deleteDescription')}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={handleDeleteContact}
                onCancel={() => setDeleteRequest(null)}
            />

            {/* Confirmation: Delete Provider */}
            <ConfirmationModal
                isOpen={!!deleteProviderRequest}
                title={t('contacts.deleteProviderTitle')}
                description={t('contacts.deleteProviderDesc', { name: deleteProviderRequest?.name })}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={handleDeleteProvider}
                onCancel={() => setDeleteProviderRequest(null)}
            />

            {/* Export Trust List Package Modal */}
            <ModalShell
                isOpen={isExportModalOpen}
                onClose={() => {
                    setIsExportModalOpen(false);
                    setExportPassword('');
                }}
                title={t('contacts.exportWotModalTitle')}
                description={t('contacts.exportWotModalDesc')}
                footer={
                    <div className="flex justify-end gap-3">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                setIsExportModalOpen(false);
                                setExportPassword('');
                            }}
                            disabled={isExporting}
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={handleExportOpenWot}
                            disabled={
                                isExporting ||
                                (exportMode === 'password' && !exportPassword.trim())
                            }
                        >
                            {isExporting ? t('common.loading') : t('contacts.exportWotButton')}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-4">
                    <div className="grid grid-cols-1 gap-2.5">
                        <button
                            type="button"
                            disabled={(wotSummary?.ratedContactsCount ?? 0) === 0}
                            onClick={() => setExportMode('contacts')}
                            className={`w-full text-left p-3 rounded-xl border transition-all flex flex-col gap-1 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed ${
                                exportMode === 'contacts'
                                    ? 'bg-emerald-50/70 border-emerald-500 text-emerald-950 ring-2 ring-emerald-400/20 shadow-sm'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                        >
                            <span className="font-bold text-xs flex items-center justify-between">
                                <span>{t('contacts.exportModeAllContacts', { count: wotSummary?.ratedContactsCount ?? 0 })}</span>
                                {exportMode === 'contacts' && <span className="text-[10px] font-black uppercase tracking-wider text-emerald-600">✓</span>}
                            </span>
                            <span className="text-[11px] text-theme-light leading-tight">
                                {t('contacts.exportModeAllContactsDesc')}
                            </span>
                        </button>

                        <button
                            type="button"
                            onClick={() => setExportMode('password')}
                            className={`w-full text-left p-3 rounded-xl border transition-all flex flex-col gap-1 cursor-pointer ${
                                exportMode === 'password'
                                    ? 'bg-theme-secondary/5 border-theme-secondary text-theme-secondary ring-2 ring-theme-secondary/20 shadow-sm'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                        >
                            <span className="font-bold text-xs flex items-center justify-between">
                                <span>{t('contacts.exportModePassword')}</span>
                                {exportMode === 'password' && <span className="text-[10px] font-black uppercase tracking-wider text-theme-secondary">✓</span>}
                            </span>
                            <span className="text-[11px] text-theme-light leading-tight">
                                {t('contacts.exportModePasswordDesc')}
                            </span>
                        </button>
                    </div>

                    {exportMode === 'password' && (
                        <div className="pt-2 animate-in fade-in duration-200">
                            <input
                                type="password"
                                value={exportPassword}
                                onChange={(e) => setExportPassword(e.target.value)}
                                placeholder={t('contacts.exportWotPasswordPlaceholder')}
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-sm shadow-sm"
                                autoFocus
                            />
                        </div>
                    )}
                </div>
            </ModalShell>

            {/* Password Modal for Encrypted Trust Package */}
            <ModalShell
                isOpen={isWotPasswordModalOpen}
                onClose={() => {
                    setIsWotPasswordModalOpen(false);
                    setWotContainerPassword('');
                    setPendingWotImportBytes(null);
                }}
                title={t('transfer.wotImport.title')}
                description={t('transfer.importFile.passwordPlaceholder')}
                footer={
                    <div className="flex justify-end gap-3">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                setIsWotPasswordModalOpen(false);
                                setWotContainerPassword('');
                                setPendingWotImportBytes(null);
                            }}
                            disabled={isImportingWot}
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => {
                                if (pendingWotImportBytes) {
                                    executeImportWot(pendingWotImportBytes, wotContainerPassword);
                                }
                            }}
                            disabled={isImportingWot}
                        >
                            {isImportingWot ? t('common.loading') : t('transfer.importFile.confirmImport')}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-4">
                    <input
                        type="password"
                        value={wotContainerPassword}
                        onChange={(e) => setWotContainerPassword(e.target.value)}
                        placeholder={t('transfer.importFile.passwordPlaceholder')}
                        className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-sm shadow-sm"
                        autoFocus
                    />
                </div>
            </ModalShell>

            {/* Guardian Setup Modal */}
            <GuardianSetupModal
                isOpen={isGuardianSetupOpen}
                initialGuardianDids={Array.from(guardianDids)}
                onClose={() => {
                    setIsGuardianSetupOpen(false);
                    loadContacts();
                }}
            />
        </PageLayout>
    );
};

export default AddressBook;
