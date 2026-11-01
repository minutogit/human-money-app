// src/components/AddressBook.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { PageLayout } from './ui/PageLayout';
import { Button } from './ui/Button';
import { contactService } from '../services/contactService';
import { wotService } from '../services/wotService';
import { settingsService } from '../services/settingsService';
import { save, open } from '@tauri-apps/plugin-dialog';
import { writeFile, readFile } from '@tauri-apps/plugin-fs';
import { copyToClipboard } from '../utils/clipboard';
import { HelpIcon } from './ui/HelpIcon';
import { Contact, WotSummaryDto, WotProviderDto } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import Avatar from 'boring-avatars';
import ContactDialog from './ContactDialog';
import { logger } from '../utils/log';
import { stringifyError, translateError } from '../utils/errorHelper';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { useSession } from '../context/SessionContext';
import { profileService } from '../services/profileService';
import { voucherService } from '../services/voucherService';
import { buildContactCardText } from '../utils/humocoUri';
import { AlertBanner } from './ui/AlertBanner';
import { ModalShell } from './ui/ModalShell';
import { PageLoadingState, PageEmptyState } from './ui/PageState';
import { truncateUserId } from '../utils/userIdHelper';
import { SegmentedTabs } from './ui/SegmentedTabs';
import { 
    UserPlus, 
    Search, 
    Trash2, 
    Edit3, 
    Tag, 
    Shield, 
    Building2, 
    Users,
    X,
    Share2,
    Copy,
    Check,
    Upload,
    RefreshCw
} from 'lucide-react';

interface AddressBookProps {
    onBack: () => void;
    initialSearchQuery?: string;
}

const AddressBook: React.FC<AddressBookProps> = ({ onBack, initialSearchQuery }) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [wotSummary, setWotSummary] = useState<WotSummaryDto | null>(null);
    const [providers, setProviders] = useState<WotProviderDto[]>([]);
    const [activeTab, setActiveTab] = useState<'contacts' | 'providers'>('contacts');
    const [isUpdatingHops, setIsUpdatingHops] = useState(false);
    const [searchQuery, setSearchQuery] = useState(initialSearchQuery || '');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [editingContact, setEditingContact] = useState<Contact | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [deleteRequest, setDeleteRequest] = useState<string | null>(null);
    const [isExportModalOpen, setIsExportModalOpen] = useState(false);
    const [exportMode, setExportMode] = useState<'contacts' | 'password'>('contacts');
    const [exportPassword, setExportPassword] = useState('');
    const [isExporting, setIsExporting] = useState(false);
    const [refreshingProviderDid, setRefreshingProviderDid] = useState<string | null>(null);
    const [deleteProviderRequest, setDeleteProviderRequest] = useState<WotProviderDto | null>(null);
    const [copiedContactCard, setCopiedContactCard] = useState(false);
    const [pendingWotImportBytes, setPendingWotImportBytes] = useState<number[] | null>(null);
    const [isWotPasswordModalOpen, setIsWotPasswordModalOpen] = useState(false);
    const [wotContainerPassword, setWotContainerPassword] = useState('');
    const [isImportingWot, setIsImportingWot] = useState(false);
    const [feedbackMsg, setFeedbackMsg] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    const loadContacts = useCallback(async () => {
        setIsLoading(true);
        try {
            const [contactsResult, summaryResult, providersResult] = await Promise.all([
                contactService.getContacts(),
                wotService.getSummary().catch(e => {
                    logger.warn("Failed to load WoT summary: " + stringifyError(e));
                    return null;
                }),
                wotService.getProviders().catch(e => {
                    logger.warn("Failed to load WoT providers: " + stringifyError(e));
                    return [];
                })
            ]);
            setContacts(contactsResult);
            setWotSummary(summaryResult ?? null);
            setProviders(providersResult ?? []);
        } catch (err) {
            logger.error("Failed to load contacts: " + stringifyError(err));
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadContacts();
    }, [loadContacts]);

    useEffect(() => {
        if (initialSearchQuery) {
            setSearchQuery(initialSearchQuery);
        }
    }, [initialSearchQuery]);

    const handleSaveContact = async (contact: Contact) => {
        try {
            await protectAction(async (pwd) => {
                await contactService.saveContact(contact, pwd || undefined);
            });
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
            await loadContacts();
            logger.info("Contact deleted");
        } catch (err) {
            logger.error("Failed to delete contact: " + stringifyError(err));
        } finally {
            setDeleteRequest(null);
        }
    };

    const handleSetMaxHops = async (hops: number) => {
        const currentHops = wotSummary?.ownListMaxHops ?? 3;
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
                const safeName = (contact.profile.firstName || 'contact').toLowerCase().replace(/\s+/g, '-');
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

    const handleCopyContactCard = async () => {
        try {
            const [did, profile] = await Promise.all([
                voucherService.getUserId().catch(() => ''),
                profileService.getProfile().catch(() => null)
            ]);
            if (!did) {
                logger.warn("No user ID found for contact card");
                setFeedbackMsg({ type: 'error', message: t('contacts.noOwnDidError') });
                return;
            }
            const nameParts = [profile?.firstName, profile?.lastName].filter(Boolean);
            const name = nameParts.length > 0 ? nameParts.join(' ') : profile?.organization;
            const cardText = buildContactCardText({ did, name: name || undefined });
            const success = await copyToClipboard(cardText);
            if (!success) {
                setFeedbackMsg({ type: 'error', message: t('contacts.copyContactCardFailed') });
                return;
            }
            setCopiedContactCard(true);
            setTimeout(() => setCopiedContactCard(false), 2500);
        } catch (err) {
            logger.error("Failed to copy contact card: " + stringifyError(err));
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

    const handleSetProviderTrust = async (issuerDid: string, level: number) => {
        try {
            await protectAction(async (pwd) => {
                await wotService.setProviderTrust(issuerDid, level, pwd || null);
            });
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
            setFeedbackMsg({ type: 'success', message: t('contacts.providerRemovedSuccess') });
            await loadContacts();
        } catch (err) {
            logger.error("Failed to remove provider: " + stringifyError(err));
            setFeedbackMsg({ type: 'error', message: translateError(err, t) });
        } finally {
            setDeleteProviderRequest(null);
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

    return (
        <PageLayout 
            title={t('contacts.title')} 
            description={t('contacts.description')} 
            onBack={onBack}
            actions={
                <>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={handleCopyContactCard}
                        className="gap-2"
                        title={copiedContactCard ? t('contacts.copiedContactCard') : t('contacts.copyContactCard')}
                    >
                        {copiedContactCard ? <Check size={18} className="text-emerald-500" /> : <Copy size={18} />}
                        <span className="hidden sm:inline">
                            {copiedContactCard ? t('contacts.copiedContactCard') : t('contacts.copyContactCard')}
                        </span>
                    </Button>
                    <Button
                        size="sm"
                        onClick={() => {
                            setEditingContact(null);
                            setIsDialogOpen(true);
                        }}
                        className="gap-2"
                        title={t('contacts.newContact')}
                    >
                        <UserPlus size={18} />
                        <span>{t('contacts.newContact')}</span>
                    </Button>
                </>
            }
        >
            <div className="max-w-6xl mx-auto space-y-8">
                {feedbackMsg && (
                    <AlertBanner
                        variant={feedbackMsg.type}
                        onDismiss={() => setFeedbackMsg(null)}
                    >
                        {feedbackMsg.message}
                    </AlertBanner>
                )}

                {/* Trust Network Banner */}
                <div className="bg-white/60 backdrop-blur-sm border border-theme-subtle rounded-3xl p-5 shadow-sm space-y-4">
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                        <div className="space-y-1">
                            <div className="flex items-center gap-2">
                                <h4 className="text-sm font-black text-theme-secondary tracking-tight">
                                    {t('contacts.wotBannerTitle')}
                                </h4>
                                <HelpIcon topic="wotNetwork" size={15} />
                            </div>
                            <p className="text-xs text-theme-light font-medium">
                                {wotSummary && (wotSummary.ratedContactsCount > 0 || wotSummary.importedPeerListsCount > 0)
                                    ? t('contacts.wotBannerStats', {
                                          rated: wotSummary.ratedContactsCount,
                                          lists: wotSummary.importedPeerListsCount
                                      })
                                    : t('contacts.wotBannerEmpty')}
                            </p>
                        </div>
                        <div className="flex items-center gap-2 self-start sm:self-auto flex-wrap">
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={handleImportWot}
                                disabled={isImportingWot}
                                className="rounded-xl text-xs py-2 px-3 gap-1.5 whitespace-nowrap"
                            >
                                <Upload size={14} />
                                {isImportingWot ? t('common.loading') : t('contacts.importWotButton')}
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={() => {
                                    setExportMode((wotSummary?.ratedContactsCount ?? 0) > 0 ? 'contacts' : 'password');
                                    setExportPassword('');
                                    setIsExportModalOpen(true);
                                }}
                                className="rounded-xl text-xs py-2 px-3 gap-1.5 whitespace-nowrap"
                            >
                                <Share2 size={14} />
                                {t('contacts.exportWotButton')}
                            </Button>
                        </div>
                    </div>

                    {/* Global Reach Selector (1..4 Hops) */}
                    <div className="pt-3 border-t border-theme-subtle/50 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                        <div className="space-y-0.5">
                            <div className="flex items-center gap-1.5">
                                <span className="text-xs font-bold text-theme-secondary">
                                    {t('contacts.wotReachLabel')}
                                </span>
                                <HelpIcon topic="wotReach" size={14} />
                            </div>
                            <p className="text-[11px] text-theme-light">
                                {t('contacts.wotReachSubtitle')}
                            </p>
                        </div>
                        <div className="inline-flex p-1 bg-white/70 rounded-xl border border-theme-subtle/60 gap-1 self-start sm:self-auto">
                            {[
                                { hops: 1, labelKey: 'contacts.wotHops1' },
                                { hops: 2, labelKey: 'contacts.wotHops2' },
                                { hops: 3, labelKey: 'contacts.wotHops3' },
                                { hops: 4, labelKey: 'contacts.wotHops4' }
                            ].map(({ hops, labelKey }) => {
                                const currentMaxHops = wotSummary?.ownListMaxHops ?? 3;
                                const isSelected = currentMaxHops === hops;
                                return (
                                    <button
                                        key={hops}
                                        type="button"
                                        disabled={isUpdatingHops}
                                        onClick={() => handleSetMaxHops(hops)}
                                        className={`px-3 py-1.5 rounded-lg text-xs font-black transition-all cursor-pointer disabled:opacity-50 ${
                                            isSelected
                                                ? 'bg-theme-secondary text-white shadow-sm'
                                                : 'text-theme-light hover:text-theme-secondary hover:bg-white/60'
                                        }`}
                                    >
                                        {t(labelKey)}
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                </div>

                {/* Progressive Disclosure Tab Bar */}
                {providers.length > 0 && (
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
                )}


                {activeTab === 'providers' && providers.length > 0 ? (
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

                                        <div className="flex items-center gap-1.5 mt-2 mb-3">
                                            <Shield size={12} className="text-theme-light/60" />
                                            <p className="text-[10px] font-mono font-bold text-theme-light/60 truncate" title={provider.issuerDid}>
                                                {truncateUserId(provider.issuerDid)}
                                            </p>
                                        </div>

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

                                        {/* Trust level selector (0, 1, 2, 3) */}
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
                ) : (
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
                        ) : filteredContacts.length === 0 ? (
                            <PageEmptyState
                                icon={<Users size={32} />}
                                title={contacts.length === 0 ? t('contacts.emptyTitle') : t('contacts.noContactsFound')}
                                description={t('contacts.emptyDescription')}
                                action={
                                    <Button
                                        variant="secondary"
                                        onClick={() => {
                                            setEditingContact(null);
                                            setIsDialogOpen(true);
                                        }}
                                    >
                                        {t('contacts.addFirstContact')}
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
                                                    <div className="w-16 h-16 rounded-[24px] overflow-hidden border-2 border-theme-subtle group-hover:border-theme-primary/30 transition-all p-1 bg-white shadow-inner">
                                                        <div className="w-full h-full rounded-[18px] overflow-hidden">
                                                            <Avatar
                                                                size={64}
                                                                name={contact.did}
                                                                variant="beam"
                                                                colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]}
                                                            />
                                                        </div>
                                                    </div>
                                                    <div className="absolute -bottom-1 -right-1 bg-white p-1 rounded-full shadow-sm border border-theme-subtle">
                                                        <div className="bg-emerald-500 w-3 h-3 rounded-full border-2 border-white"></div>
                                                    </div>
                                                </div>
                                                <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                                    <button
                                                        onClick={() => handleShareContactWot(contact)}
                                                        className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                                                        title={t('contacts.shareWotForContact')}
                                                    >
                                                        <Share2 size={18} />
                                                    </button>
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

                                                <div className="flex items-center gap-1.5 mt-2 mb-4">
                                                    <Shield size={12} className="text-theme-light/60" />
                                                    <p className="text-[10px] font-mono font-bold text-theme-light/60 truncate" title={contact.did}>
                                                        {truncateUserId(contact.did)}
                                                    </p>
                                                </div>

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
                                                </div>

                                                <div className="flex flex-wrap gap-1.5 mt-auto">
                                                    {contact.tags.map(tag => (
                                                        <span key={tag} className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm">
                                                            <Tag size={8} />
                                                            {tag}
                                                        </span>
                                                    ))}
                                                    {contact.tags.length === 0 && (
                                                        <span className="text-[9px] font-black uppercase tracking-widest text-theme-light/40 italic">{t('contacts.noTagsAssigned')}</span>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </>
                )}

            </div>

            <ContactDialog
                isOpen={isDialogOpen}
                onClose={() => setIsDialogOpen(false)}
                onSave={handleSaveContact}
                existingContact={editingContact}
            />

            <ConfirmationModal
                isOpen={!!deleteRequest}
                title={t('contacts.deleteTitle')}
                description={t('contacts.deleteDescription')}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={handleDeleteContact}
                onCancel={() => setDeleteRequest(null)}
            />

            <ConfirmationModal
                isOpen={!!deleteProviderRequest}
                title={t('contacts.deleteProviderTitle')}
                description={t('contacts.deleteProviderDesc', { name: deleteProviderRequest?.name })}
                confirmText={t('contacts.deleteConfirmButton')}
                confirmVariant="danger"
                onConfirm={handleDeleteProvider}
                onCancel={() => setDeleteProviderRequest(null)}
            />

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
        </PageLayout>
    );
};

export default AddressBook;
