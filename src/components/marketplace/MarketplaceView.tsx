// src/components/marketplace/MarketplaceView.tsx
import { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import Avatar from 'boring-avatars';
import { 
    Store, 
    Search, 
    RefreshCw, 
    Send, 
    CheckCircle2, 
    HelpCircle, 
    Ticket, 
    Tag, 
    SlidersHorizontal,
    X,
    Download,
    Server,
    MessageSquare,
    Check,
    Clock,
    User
} from 'lucide-react';
import { FrontendMarketPerson, FrontendMarketListing, FrontendMarketInquiryItem } from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { marketService } from '../../services/marketService';
import { postService, POST_INBOX_UPDATED_EVENT } from '../../services/postService';
import { profileService } from '../../services/profileService';
import { settingsService } from '../../services/settingsService';
import { PageLayout } from '../ui/PageLayout';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { Textarea } from '../ui/Textarea';
import { ModalShell } from '../ui/ModalShell';
import { AlertBanner } from '../ui/AlertBanner';
import { PageLoadingState, PageEmptyState } from '../ui/PageState';
import { MarketPersonView } from './MarketPersonView';
import { MyOfferModal } from './MyOfferModal';
import { ImportCardModal } from './ImportCardModal';
import { DropManagementModal } from './DropManagementModal';
import { truncateUserId } from '../../utils/userIdHelper';
import { translateError } from '../../utils/errorHelper';

export interface MarketplaceViewProps {
    initialSelectedDid?: string;
}

export function MarketplaceView({ initialSelectedDid }: MarketplaceViewProps) {
    const { t } = useTranslation();
    const { navigate, goBack, appState } = useNavigation();
    const { protectAction } = useSession();

    const [people, setPeople] = useState<FrontendMarketPerson[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);
    const [ownDid, setOwnDid] = useState<string>('');

    // Marketplace post inquiries (§6.4.3)
    const [sellerInquiries, setSellerInquiries] = useState<FrontendMarketInquiryItem[]>([]);
    const [buyerInquiries, setBuyerInquiries] = useState<FrontendMarketInquiryItem[]>([]);

    // 1-Step inquiry sending state (NO Chat! max 280 chars)
    const [inquiryTarget, setInquiryTarget] = useState<{ person: FrontendMarketPerson; listing: FrontendMarketListing } | null>(null);
    const [inquiryMessage, setInquiryMessage] = useState('');
    const [isSendingInquiry, setIsSendingInquiry] = useState(false);
    const [inquirySendError, setInquirySendError] = useState<string | null>(null);

    // 1-Step inquiry replying state
    const [replyingInquiry, setReplyingInquiry] = useState<FrontendMarketInquiryItem | null>(null);
    const [replyDecision, setReplyDecision] = useState<'confirm' | 'decline'>('confirm');
    const [replyNote, setReplyNote] = useState('');
    const [isReplyingInquiry, setIsReplyingInquiry] = useState(false);

    // Navigation sub-view state
    const didFromNav = appState.view === 'marketplace' ? appState.selectedDid : undefined;
    const [selectedPersonDid, setSelectedPersonDid] = useState<string | null>(initialSelectedDid || didFromNav || null);

    useEffect(() => {
        if (appState.view === 'marketplace' && appState.selectedDid) {
            setSelectedPersonDid(appState.selectedDid);
        }
    }, [appState]);

    // Modals
    const [isMyOfferModalOpen, setIsMyOfferModalOpen] = useState<boolean>(false);
    const [isImportModalOpen, setIsImportModalOpen] = useState<boolean>(false);
    const [isDropModalOpen, setIsDropModalOpen] = useState<boolean>(false);

    // Filters and tabs
    const [activeTab, setActiveTab] = useState<'offer' | 'request' | 'people'>('offer');
    const [searchQuery, setSearchQuery] = useState<string>('');
    const [selectedStandardUuid, setSelectedStandardUuid] = useState<string>('all');
    const [trustFilter, setTrustFilter] = useState<'all' | 'trusted'>('all');
    const [showUnknown, setShowUnknown] = useState<boolean>(true);

    const loadMarketView = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [data, postSummary, myId] = await Promise.all([
                marketService.getMarketView(),
                postService.getPostInboxSummary().catch(() => null),
                profileService.getUserId().catch(() => '')
            ]);
            setPeople(data || []);
            if (postSummary) {
                setSellerInquiries(postSummary.sellerInquiries || []);
                setBuyerInquiries(postSummary.buyerInquiries || []);
            }
            if (myId) {
                setOwnDid(myId);
            }
        } catch (err) {
            setError(translateError(err, t));
        } finally {
            setIsLoading(false);
        }
    }, [t]);

    const triggerDropSync = useCallback(() => {
        marketService.syncMarketDrops()
            .then((res) => {
                if (res && res.importedCards > 0) {
                    loadMarketView();
                }
            })
            .catch(() => {
                // Non-blocking sync error
            });
    }, [loadMarketView]);

    useEffect(() => {
        loadMarketView();
        triggerDropSync();
        postService.syncPostDrops().catch(() => {});
        window.addEventListener(POST_INBOX_UPDATED_EVENT, loadMarketView);
        return () => {
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, loadMarketView);
        };
    }, [loadMarketView, triggerDropSync]);

    const handleSendInquiry = async () => {
        if (!inquiryTarget || !inquiryMessage.trim()) return;
        setIsSendingInquiry(true);
        setInquirySendError(null);
        try {
            const created = await protectAction(async (pwd) => {
                return await postService.sendMarketInquiryViaPost(
                    inquiryTarget.person.did,
                    inquiryTarget.listing.listingId,
                    inquiryTarget.listing.title,
                    inquiryMessage.trim(),
                    inquiryTarget.person.cardDrops,
                    pwd || undefined
                );
            });
            if (created) {
                setBuyerInquiries(prev => [...prev.filter(i => i.inquiryId !== created.inquiryId), created]);
                setInquiryTarget(null);
                setInquiryMessage('');
            }
        } catch (err) {
            setInquirySendError(translateError(err, t));
        } finally {
            setIsSendingInquiry(false);
        }
    };

    const handleExecuteReply = async () => {
        if (!replyingInquiry) return;
        setIsReplyingInquiry(true);
        try {
            const updated = await protectAction(async (pwd) => {
                return await postService.replyMarketInquiryViaPost(
                    replyingInquiry.inquiryId,
                    replyDecision,
                    replyNote.trim() || undefined,
                    undefined,
                    undefined,
                    undefined,
                    pwd || undefined
                );
            });
            if (updated) {
                setSellerInquiries(prev => prev.map(i => i.inquiryId === updated.inquiryId ? updated : i));
                setReplyingInquiry(null);
                setReplyNote('');
            }
        } catch (err) {
            console.error("Failed to reply to inquiry:", err);
        } finally {
            setIsReplyingInquiry(false);
        }
    };

    const handleDismissInquiry = async (kind: 'seller_inquiry' | 'buyer_inquiry', id: string) => {
        try {
            await postService.dismissPendingPostItem(kind, id);
            if (kind === 'seller_inquiry') {
                setSellerInquiries(prev => prev.filter(i => i.inquiryId !== id));
            } else {
                setBuyerInquiries(prev => prev.filter(i => i.inquiryId !== id));
            }
        } catch (err) {
            console.error("Failed to dismiss inquiry:", err);
        }
    };

    useEffect(() => {
        async function fetchSettings() {
            try {
                const s = await settingsService.getSettings();
                if (s && typeof s.marketShowUnknown === 'boolean') {
                    setShowUnknown(s.marketShowUnknown);
                }
            } catch {
                // Keep default true
            }
        }
        fetchSettings();
    }, []);

    // Collect all unique voucher standards across all people
    const availableStandards = useMemo(() => {
        const map = new Map<string, string>();
        for (const person of people) {
            for (const std of person.standards) {
                if (std.uuid && !map.has(std.uuid)) {
                    map.set(std.uuid, std.name);
                }
            }
        }
        return Array.from(map.entries()).map(([uuid, name]) => ({ uuid, name }));
    }, [people]);

    // Helpers to extract display name
    const getDisplayName = useCallback((person: FrontendMarketPerson): string => {
        const fullName = [person.profile.firstName, person.profile.lastName].filter(Boolean).join(' ');
        return fullName || person.profile.organization || person.profile.community || truncateUserId(person.did);
    }, []);

    // Filter people according to search query, standard, trust, and showUnknown
    const filteredPeople = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();

        return people.filter((person) => {
            // Hidden is already filtered out by backend, but guard here too
            if (person.trust === 'hidden') return false;

            // Setting toggle: Hide unknown if showUnknown is false
            if (!showUnknown && person.trust !== 'trusted') return false;

            // Filter dropdown: Trusted only
            if (trustFilter === 'trusted' && person.trust !== 'trusted') return false;

            // Filter standard
            if (selectedStandardUuid !== 'all') {
                const hasStandard = person.standards.some((s) => s.uuid === selectedStandardUuid);
                if (!hasStandard) return false;
            }

            // Search query filter
            if (q) {
                const name = getDisplayName(person).toLowerCase();
                const firstName = (person.profile.firstName || '').toLowerCase();
                const lastName = (person.profile.lastName || '').toLowerCase();
                const org = (person.profile.organization || '').toLowerCase();
                const comm = (person.profile.community || '').toLowerCase();
                const did = person.did.toLowerCase();
                const stdMatches = person.standards.some((s) => s.name.toLowerCase().includes(q));

                const personMatches = name.includes(q) || firstName.includes(q) || lastName.includes(q) ||
                    org.includes(q) || comm.includes(q) || did.includes(q) || stdMatches;

                const listingsMatch = person.listings.some((l) =>
                    l.title.toLowerCase().includes(q) || (l.text && l.text.toLowerCase().includes(q))
                );

                if (!personMatches && !listingsMatch) return false;
            }

            return true;
        });
    }, [people, showUnknown, trustFilter, selectedStandardUuid, searchQuery, getDisplayName]);

    // Flatten listings for Offer and Request tabs
    const filteredOffers = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();
        const items: Array<{ person: FrontendMarketPerson; listing: FrontendMarketListing }> = [];

        for (const person of filteredPeople) {
            for (const listing of person.listings) {
                if (listing.listingType !== 'offer') continue;

                if (q) {
                    const titleMatch = listing.title.toLowerCase().includes(q);
                    const textMatch = listing.text ? listing.text.toLowerCase().includes(q) : false;
                    const personNameMatch = getDisplayName(person).toLowerCase().includes(q);
                    const stdMatch = person.standards.some((s) => s.name.toLowerCase().includes(q));

                    if (!titleMatch && !textMatch && !personNameMatch && !stdMatch) continue;
                }

                items.push({ person, listing });
            }
        }
        return items;
    }, [filteredPeople, searchQuery, getDisplayName]);

    const filteredRequests = useMemo(() => {
        const q = searchQuery.trim().toLowerCase();
        const items: Array<{ person: FrontendMarketPerson; listing: FrontendMarketListing }> = [];

        for (const person of filteredPeople) {
            for (const listing of person.listings) {
                if (listing.listingType !== 'request') continue;

                if (q) {
                    const titleMatch = listing.title.toLowerCase().includes(q);
                    const textMatch = listing.text ? listing.text.toLowerCase().includes(q) : false;
                    const personNameMatch = getDisplayName(person).toLowerCase().includes(q);
                    const stdMatch = person.standards.some((s) => s.name.toLowerCase().includes(q));

                    if (!titleMatch && !textMatch && !personNameMatch && !stdMatch) continue;
                }

                items.push({ person, listing });
            }
        }
        return items;
    }, [filteredPeople, searchQuery, getDisplayName]);

    // Count badges
    const totalOffersCount = useMemo(() => {
        return people.reduce((acc, p) => acc + p.listings.filter((l) => l.listingType === 'offer').length, 0);
    }, [people]);

    const totalRequestsCount = useMemo(() => {
        return people.reduce((acc, p) => acc + p.listings.filter((l) => l.listingType === 'request').length, 0);
    }, [people]);

    // If viewing a detailed person view
    const selectedPerson = useMemo(() => {
        if (!selectedPersonDid) return null;
        return people.find((p) => p.did === selectedPersonDid) || null;
    }, [people, selectedPersonDid]);

    if (selectedPerson) {
        return (
            <PageLayout 
                title={t('marketplace.title')} 
                description={t('marketplace.subtitle')}
                onBack={() => setSelectedPersonDid(null)}
            >
                <MarketPersonView
                    person={selectedPerson}
                    onBack={() => setSelectedPersonDid(null)}
                />
            </PageLayout>
        );
    }

    const hasActiveFilters = searchQuery !== '' || selectedStandardUuid !== 'all' || trustFilter !== 'all' || !showUnknown;

    const handleResetFilters = () => {
        setSearchQuery('');
        setSelectedStandardUuid('all');
        setTrustFilter('all');
        setShowUnknown(true);
    };

    return (
        <PageLayout
            title={t('marketplace.title')}
            description={t('marketplace.subtitle')}
            onBack={goBack}
            actions={
                <div className="flex items-center gap-2">
                    <Button
                        variant="primary"
                        size="sm"
                        onClick={() => setIsMyOfferModalOpen(true)}
                        className="gap-2 shadow-xs font-semibold"
                    >
                        <Tag size={14} />
                        <span>{t('marketplace.myOfferBtn')}</span>
                    </Button>

                    <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setIsImportModalOpen(true)}
                        className="gap-2 shadow-xs"
                    >
                        <Download size={14} />
                        <span>{t('marketplace.importBtn')}</span>
                    </Button>

                    <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setIsDropModalOpen(true)}
                        className="gap-2 shadow-xs"
                        title={t('marketplace.dropsBtnTitle')}
                    >
                        <Server size={14} />
                        <span>{t('marketplace.dropsBtn')}</span>
                    </Button>

                    <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                            loadMarketView();
                            triggerDropSync();
                        }}
                        disabled={isLoading}
                        className="gap-2 shadow-xs"
                    >
                        <RefreshCw size={14} className={isLoading ? "animate-spin" : ""} />
                        <span>{t('marketplace.refresh')}</span>
                    </Button>
                </div>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6">
                {/* Search & Filter Bar */}
                <div className="p-4 bg-white border border-theme-subtle rounded-3xl shadow-soft space-y-3">
                    <div className="flex flex-col md:flex-row items-stretch md:items-center gap-3">
                        {/* Substring Search Input */}
                        <div className="relative flex-1">
                            <Search size={16} className="absolute left-3.5 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none" />
                            <Input
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                                placeholder={t('marketplace.searchPlaceholder')}
                                className="pl-10 h-10 text-xs bg-bg-app border-theme-subtle rounded-2xl"
                            />
                            {searchQuery && (
                                <button
                                    type="button"
                                    onClick={() => setSearchQuery('')}
                                    className="absolute right-3 top-1/2 -translate-y-1/2 text-theme-light hover:text-theme-secondary p-1"
                                >
                                    <X size={14} />
                                </button>
                            )}
                        </div>

                        {/* Standard Filter Dropdown */}
                        {availableStandards.length > 0 && (
                            <select
                                aria-label={t('marketplace.allStandards')}
                                value={selectedStandardUuid}
                                onChange={(e) => setSelectedStandardUuid(e.target.value)}
                                className="h-10 px-3 py-2 bg-bg-app border border-theme-subtle rounded-2xl text-xs font-medium text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                            >
                                <option value="all">{t('marketplace.allStandards')}</option>
                                {availableStandards.map((std) => (
                                    <option key={std.uuid} value={std.uuid}>{std.name}</option>
                                ))}
                            </select>
                        )}

                        {/* Trust Filter Dropdown */}
                        <select
                            aria-label={t('marketplace.allTrustLevels')}
                            value={trustFilter}
                            onChange={(e) => setTrustFilter(e.target.value as 'all' | 'trusted')}
                            className="h-10 px-3 py-2 bg-bg-app border border-theme-subtle rounded-2xl text-xs font-medium text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                        >
                            <option value="all">{t('marketplace.allTrustLevels')}</option>
                            <option value="trusted">{t('marketplace.trustedOnly')}</option>
                        </select>
                    </div>

                    {/* Filter Toggle and Reset */}
                    <div className="flex flex-wrap items-center justify-between gap-3 pt-2 border-t border-theme-subtle/40 text-xs text-theme-light">
                        <label className="inline-flex items-center gap-2 cursor-pointer select-none">
                            <input
                                type="checkbox"
                                checked={showUnknown}
                                onChange={(e) => setShowUnknown(e.target.checked)}
                                className="accent-theme-primary rounded cursor-pointer"
                            />
                            <span className="text-[11px] font-medium text-theme-secondary">
                                {t('marketplace.showUnknown')}
                            </span>
                        </label>

                        {hasActiveFilters && (
                            <button
                                type="button"
                                onClick={handleResetFilters}
                                className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-theme-primary hover:underline cursor-pointer"
                            >
                                <SlidersHorizontal size={12} />
                                <span>{t('marketplace.resetFilter')}</span>
                            </button>
                        )}
                    </div>
                </div>

                {/* Error Banner */}
                {error && (
                    <AlertBanner variant="error">
                        {t('marketplace.loadFailed', { error })}
                    </AlertBanner>
                )}

                {/* Seller Inbox Section (§6.4.3) */}
                {sellerInquiries.length > 0 && (
                    <div className="p-5 bg-teal-50/70 border border-teal-200 rounded-3xl space-y-3" data-testid="seller-inbox-section">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 text-teal-950 font-bold text-sm">
                                <MessageSquare size={18} className="text-teal-600" />
                                <span>{t('marketplace.sellerInboxTitle', 'Anfragen zu deinen Angeboten ({{count}})', { count: sellerInquiries.length })}</span>
                            </div>
                        </div>
                        <div className="space-y-3 pt-1">
                            {sellerInquiries.map(inq => {
                                const buyerName = inq.counterpartyName || truncateUserId(inq.counterpartyDid);
                                const hasReplied = !!inq.reply;

                                return (
                                    <div key={inq.inquiryId} className="p-4 bg-white rounded-2xl border border-teal-150 shadow-xs flex flex-col md:flex-row md:items-start justify-between gap-4">
                                        <div className="space-y-1.5 min-w-0 flex-1">
                                            <div className="flex items-center gap-2 flex-wrap">
                                                <span className="font-bold text-sm text-theme-primary">{inq.listingTitle}</span>
                                                <span className="text-[11px] text-theme-light flex items-center gap-1">
                                                    <User size={11} /> {buyerName}
                                                </span>
                                            </div>
                                            <p className="text-xs text-theme-secondary/90 bg-bg-app p-2.5 rounded-xl border border-theme-subtle/50 whitespace-pre-wrap leading-relaxed">
                                                "{inq.message}"
                                            </p>
                                            {hasReplied && (
                                                <div className="flex items-center gap-2 text-xs pt-1">
                                                    <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${inq.reply?.decision === 'confirm' ? 'bg-emerald-100 text-emerald-800' : 'bg-rose-100 text-rose-800'}`}>
                                                        {inq.reply?.decision === 'confirm' ? t('marketplace.inquiryStatusConfirmed', 'Bestätigt') : t('marketplace.inquiryStatusDeclined', 'Abgesagt')}
                                                    </span>
                                                    {inq.reply?.note && (
                                                        <span className="text-theme-light italic">"{inq.reply.note}"</span>
                                                    )}
                                                </div>
                                            )}
                                        </div>

                                        <div className="flex items-center gap-2 shrink-0 self-end md:self-center">
                                            {!hasReplied ? (
                                                <>
                                                    <Button
                                                        size="xs"
                                                        variant="outline"
                                                        onClick={() => {
                                                            setReplyingInquiry(inq);
                                                            setReplyDecision('decline');
                                                            setReplyNote('');
                                                        }}
                                                        data-testid="decline-inquiry-button"
                                                        className="text-theme-light hover:text-rose-600"
                                                    >
                                                        {t('marketplace.inquiryDeclineBtn', 'Absagen')}
                                                    </Button>
                                                    <Button
                                                        size="xs"
                                                        variant="primary"
                                                        onClick={() => {
                                                            setReplyingInquiry(inq);
                                                            setReplyDecision('confirm');
                                                            setReplyNote('');
                                                        }}
                                                        data-testid="confirm-inquiry-button"
                                                        className="bg-teal-600 hover:bg-teal-700 text-white"
                                                    >
                                                        {t('marketplace.inquiryConfirmBtn', 'Bestätigen')}
                                                    </Button>
                                                </>
                                            ) : (
                                                <Button
                                                    size="xs"
                                                    variant="outline"
                                                    onClick={() => handleDismissInquiry('seller_inquiry', inq.inquiryId)}
                                                    data-testid="dismiss-seller-inquiry-button"
                                                    className="text-theme-light"
                                                >
                                                    {t('common.dismiss', 'Verwerfen')}
                                                </Button>
                                            )}
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    </div>
                )}

                {/* Buyer Sent Inquiries Section (§6.4.3) */}
                {buyerInquiries.length > 0 && (
                    <div className="p-4 bg-sky-50/60 border border-sky-200 rounded-3xl space-y-2.5" data-testid="buyer-inquiries-section">
                        <div className="flex items-center gap-2 text-sky-950 font-bold text-xs uppercase tracking-wider">
                            <Clock size={14} className="text-sky-600" />
                            <span>{t('marketplace.buyerInquiriesTitle', 'Deine gesendeten Anfragen')}</span>
                        </div>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
                            {buyerInquiries.map(inq => {
                                const isConfirmed = inq.reply?.decision === 'confirm';
                                const isDeclined = inq.reply?.decision === 'decline';
                                const isWaiting = !inq.reply;

                                return (
                                    <div key={inq.inquiryId} className="p-3 bg-white rounded-2xl border border-sky-100 shadow-xs flex items-center justify-between gap-3 text-xs">
                                        <div className="min-w-0">
                                            <p className="font-bold text-theme-primary truncate">{inq.listingTitle}</p>
                                            <div className="flex items-center gap-1.5 mt-0.5">
                                                {isWaiting && (
                                                    <span className="px-1.5 py-0.2 rounded text-[10px] font-bold bg-amber-50 text-amber-800 border border-amber-200">
                                                        {t('marketplace.inquiryWaiting', 'Wartet auf Antwort')}
                                                    </span>
                                                )}
                                                {isConfirmed && (
                                                    <span className="px-1.5 py-0.2 rounded text-[10px] font-bold bg-emerald-50 text-emerald-800 border border-emerald-200">
                                                        {t('marketplace.inquiryConfirmed', 'Bestätigt')}
                                                    </span>
                                                )}
                                                {isDeclined && (
                                                    <span className="px-1.5 py-0.2 rounded text-[10px] font-bold bg-rose-50 text-rose-800 border border-rose-200">
                                                        {t('marketplace.inquiryDeclined', 'Abgesagt')}
                                                    </span>
                                                )}
                                                {inq.reply?.note && (
                                                    <span className="text-[10px] text-theme-light italic truncate max-w-[140px]">"{inq.reply.note}"</span>
                                                )}
                                            </div>
                                        </div>

                                        <div className="flex items-center gap-2 shrink-0">
                                            {isConfirmed && (
                                                <Button
                                                    size="xs"
                                                    variant="primary"
                                                    onClick={() => navigate({ view: 'send_vouchers', initialRecipientId: inq.counterpartyDid, initialNote: inq.reply?.note || inq.listingTitle })}
                                                    data-testid="pay-confirmed-inquiry-button"
                                                    className="bg-emerald-600 hover:bg-emerald-700 text-white gap-1"
                                                >
                                                    <Send size={11} />
                                                    <span>{t('marketplace.payNowBtn', 'Jetzt bezahlen')}</span>
                                                </Button>
                                            )}
                                            <Button
                                                size="xs"
                                                variant="outline"
                                                onClick={() => handleDismissInquiry('buyer_inquiry', inq.inquiryId)}
                                                className="text-theme-light hover:text-rose-600 p-1 h-auto"
                                                title={t('common.dismiss')}
                                            >
                                                <X size={12} />
                                            </Button>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    </div>
                )}

                {/* Navigation Segments / Tabs */}
                <div className="flex items-center gap-2 border-b border-theme-subtle pb-1">
                    <button
                        type="button"
                        onClick={() => setActiveTab('offer')}
                        className={`inline-flex items-center gap-2 px-4 py-2.5 rounded-2xl text-xs font-bold transition-all cursor-pointer ${
                            activeTab === 'offer'
                                ? 'bg-theme-primary text-white shadow-xs'
                                : 'text-theme-light hover:text-theme-secondary hover:bg-theme-subtle/20'
                        }`}
                    >
                        <span>{t('marketplace.tabOffers')}</span>
                        <span className={`px-1.5 py-0.5 rounded-full text-[10px] font-extrabold ${
                            activeTab === 'offer' ? 'bg-white/20 text-white' : 'bg-theme-subtle/40 text-theme-secondary'
                        }`}>
                            {totalOffersCount}
                        </span>
                    </button>

                    <button
                        type="button"
                        onClick={() => setActiveTab('request')}
                        className={`inline-flex items-center gap-2 px-4 py-2.5 rounded-2xl text-xs font-bold transition-all cursor-pointer ${
                            activeTab === 'request'
                                ? 'bg-theme-primary text-white shadow-xs'
                                : 'text-theme-light hover:text-theme-secondary hover:bg-theme-subtle/20'
                        }`}
                    >
                        <span>{t('marketplace.tabRequests')}</span>
                        <span className={`px-1.5 py-0.5 rounded-full text-[10px] font-extrabold ${
                            activeTab === 'request' ? 'bg-white/20 text-white' : 'bg-theme-subtle/40 text-theme-secondary'
                        }`}>
                            {totalRequestsCount}
                        </span>
                    </button>

                    <button
                        type="button"
                        onClick={() => setActiveTab('people')}
                        className={`inline-flex items-center gap-2 px-4 py-2.5 rounded-2xl text-xs font-bold transition-all cursor-pointer ${
                            activeTab === 'people'
                                ? 'bg-theme-primary text-white shadow-xs'
                                : 'text-theme-light hover:text-theme-secondary hover:bg-theme-subtle/20'
                        }`}
                    >
                        <span>{t('marketplace.tabPeople')}</span>
                        <span className={`px-1.5 py-0.5 rounded-full text-[10px] font-extrabold ${
                            activeTab === 'people' ? 'bg-white/20 text-white' : 'bg-theme-subtle/40 text-theme-secondary'
                        }`}>
                            {people.length}
                        </span>
                    </button>
                </div>

                {/* Content Area */}
                {isLoading ? (
                    <PageLoadingState message={t('common.loading')} />
                ) : people.length === 0 ? (
                    <PageEmptyState
                        icon={<Store size={28} className="text-theme-primary" />}
                        title={t('marketplace.emptyTitle')}
                        description={t('marketplace.emptyDescription')}
                    />
                ) : (
                    <>
                        {/* Biete / Offers Tab */}
                        {activeTab === 'offer' && (
                            <div className="space-y-3">
                                {filteredOffers.length === 0 ? (
                                    <PageEmptyState
                                        title={t('marketplace.noMatchingFilter')}
                                        action={hasActiveFilters ? (
                                            <Button variant="outline" size="sm" onClick={handleResetFilters}>
                                                {t('marketplace.resetFilter')}
                                            </Button>
                                        ) : undefined}
                                    />
                                ) : (
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                        {filteredOffers.map(({ person, listing }, index) => {
                                            const personName = getDisplayName(person);
                                            const formattedDate = person.asOf ? new Date(person.asOf * 1000).toLocaleDateString() : '';

                                            return (
                                                <Card
                                                    key={`${person.did}-offer-${index}`}
                                                    onClick={() => setSelectedPersonDid(person.did)}
                                                    className="p-5 bg-white border border-theme-subtle hover:border-theme-primary/30 rounded-3xl shadow-soft hover:shadow-md transition-all cursor-pointer flex flex-col justify-between gap-4 group"
                                                >
                                                    <div className="space-y-2">
                                                        <div className="flex items-start justify-between gap-3">
                                                            <h3 className="text-base font-bold text-theme-secondary group-hover:text-theme-primary transition-colors leading-snug">
                                                                {listing.title}
                                                            </h3>
                                                            {listing.pricing && (
                                                                <span className="shrink-0 px-2.5 py-0.5 rounded-lg bg-emerald-50 text-emerald-800 border border-emerald-200 text-xs font-bold">
                                                                    {listing.pricing}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {listing.text && (
                                                            <p className="text-xs text-theme-secondary/80 line-clamp-3 leading-relaxed">
                                                                {listing.text}
                                                            </p>
                                                        )}
                                                    </div>

                                                    <div className="pt-3 border-t border-theme-subtle/40 flex items-center justify-between gap-3 text-xs">
                                                        {/* Person snippet */}
                                                        <div className="flex items-center gap-2 min-w-0">
                                                            <div className="shrink-0">
                                                                <Avatar
                                                                    size={24}
                                                                    name={personName}
                                                                    variant="beam"
                                                                    colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
                                                                />
                                                            </div>
                                                            <span className="font-semibold text-theme-secondary truncate text-[11px]">
                                                                {personName}
                                                            </span>

                                                            {/* Discrete Trust Badge */}
                                                            {person.trust === 'trusted' ? (
                                                                <span title={t('marketplace.trustTrusted')} className="shrink-0 inline-flex items-center text-emerald-600">
                                                                    <CheckCircle2 size={13} />
                                                                </span>
                                                            ) : (
                                                                <span title={t('marketplace.trustUnknown')} className="shrink-0 inline-flex items-center text-slate-400">
                                                                    <HelpCircle size={13} />
                                                                </span>
                                                            )}

                                                            {/* Source icon */}
                                                            {person.sources.length > 0 && person.sources[0].kind === 'voucher' && (
                                                                <span title={t('marketplace.sourceVoucher')} className="shrink-0 inline-flex items-center text-amber-600">
                                                                    <Ticket size={12} />
                                                                </span>
                                                            )}
                                                            {formattedDate && (
                                                                <span className="shrink-0 text-[10px] text-theme-light opacity-75">
                                                                    {t('marketplace.asOf', { date: formattedDate })}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {/* Actions & Inquiry Status (§6.4.3) */}
                                                        <div className="flex items-center gap-2 shrink-0">
                                                            {(() => {
                                                                const buyerInquiry = buyerInquiries.find(i => i.listingId === listing.listingId);
                                                                const isOwn = person.did === ownDid;

                                                                if (buyerInquiry) {
                                                                    if (!buyerInquiry.reply) {
                                                                        return (
                                                                            <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-amber-50 text-amber-800 border border-amber-200">
                                                                                {t('marketplace.inquiryWaiting', 'Wartet auf Antwort')}
                                                                            </span>
                                                                        );
                                                                    }
                                                                    if (buyerInquiry.reply.decision === 'confirm') {
                                                                        return (
                                                                            <div className="flex items-center gap-1.5">
                                                                                <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-50 text-emerald-800 border border-emerald-200">
                                                                                    {t('marketplace.inquiryConfirmed', 'Bestätigt')}
                                                                                </span>
                                                                                <Button
                                                                                    size="xs"
                                                                                    variant="primary"
                                                                                    onClick={(e) => {
                                                                                        e.stopPropagation();
                                                                                        navigate({ view: 'send_vouchers', initialRecipientId: person.did, initialNote: buyerInquiry.reply?.note || listing.title });
                                                                                    }}
                                                                                    data-testid="pay-confirmed-listing-button"
                                                                                    className="bg-emerald-600 hover:bg-emerald-700 text-white gap-1 text-[11px]"
                                                                                >
                                                                                    <Send size={11} />
                                                                                    <span>{t('marketplace.payNowBtn', 'Jetzt bezahlen')}</span>
                                                                                </Button>
                                                                            </div>
                                                                        );
                                                                    }
                                                                    if (buyerInquiry.reply.decision === 'decline') {
                                                                        return (
                                                                            <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-rose-50 text-rose-800 border border-rose-200">
                                                                                {t('marketplace.inquiryDeclined', 'Abgesagt')}
                                                                            </span>
                                                                        );
                                                                    }
                                                                }

                                                                return (
                                                                    <>
                                                                        {!isOwn && listing.allowInquiries && person.cardDrops && person.cardDrops.length > 0 && (
                                                                            <Button
                                                                                size="xs"
                                                                                variant="outline"
                                                                                onClick={(e) => {
                                                                                    e.stopPropagation();
                                                                                    setInquiryTarget({ person, listing });
                                                                                    setInquiryMessage('');
                                                                                }}
                                                                                data-testid="inquiry-button"
                                                                                className="gap-1 text-[11px] border-theme-subtle"
                                                                            >
                                                                                <MessageSquare size={11} />
                                                                                <span>{t('marketplace.inquiryButton', 'Reservieren / Anfragen')}</span>
                                                                            </Button>
                                                                        )}
                                                                        <Button
                                                                            size="xs"
                                                                            variant="outline"
                                                                            onClick={(e) => {
                                                                                e.stopPropagation();
                                                                                navigate({ view: 'send_vouchers', initialRecipientId: person.did, initialNote: listing.title });
                                                                            }}
                                                                            className="gap-1 text-[11px]"
                                                                        >
                                                                            <Send size={11} />
                                                                            <span>{t('marketplace.pay', 'Bezahlen')}</span>
                                                                        </Button>
                                                                    </>
                                                                );
                                                            })()}
                                                        </div>
                                                    </div>
                                                </Card>
                                            );
                                        })}
                                    </div>
                                )}
                            </div>
                        )}

                        {/* Suche / Requests Tab */}
                        {activeTab === 'request' && (
                            <div className="space-y-3">
                                {filteredRequests.length === 0 ? (
                                    <PageEmptyState
                                        title={t('marketplace.noMatchingFilter')}
                                        action={hasActiveFilters ? (
                                            <Button variant="outline" size="sm" onClick={handleResetFilters}>
                                                {t('marketplace.resetFilter')}
                                            </Button>
                                        ) : undefined}
                                    />
                                ) : (
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                        {filteredRequests.map(({ person, listing }, index) => {
                                            const personName = getDisplayName(person);
                                            const formattedDate = person.asOf ? new Date(person.asOf * 1000).toLocaleDateString() : '';

                                            return (
                                                <Card
                                                    key={`${person.did}-request-${index}`}
                                                    onClick={() => setSelectedPersonDid(person.did)}
                                                    className="p-5 bg-white border border-theme-subtle hover:border-theme-primary/30 rounded-3xl shadow-soft hover:shadow-md transition-all cursor-pointer flex flex-col justify-between gap-4 group"
                                                >
                                                    <div className="space-y-2">
                                                        <div className="flex items-start justify-between gap-3">
                                                            <h3 className="text-base font-bold text-theme-secondary group-hover:text-theme-primary transition-colors leading-snug">
                                                                {listing.title}
                                                            </h3>
                                                            {listing.pricing && (
                                                                <span className="shrink-0 px-2.5 py-0.5 rounded-lg bg-amber-50 text-amber-800 border border-amber-200 text-xs font-bold">
                                                                    {listing.pricing}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {listing.text && (
                                                            <p className="text-xs text-theme-secondary/80 line-clamp-3 leading-relaxed">
                                                                {listing.text}
                                                            </p>
                                                        )}
                                                    </div>

                                                    <div className="pt-3 border-t border-theme-subtle/40 flex items-center justify-between gap-3 text-xs">
                                                        {/* Person snippet */}
                                                        <div className="flex items-center gap-2 min-w-0">
                                                            <div className="shrink-0">
                                                                <Avatar
                                                                    size={24}
                                                                    name={personName}
                                                                    variant="beam"
                                                                    colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
                                                                />
                                                            </div>
                                                            <span className="font-semibold text-theme-secondary truncate text-[11px]">
                                                                {personName}
                                                            </span>

                                                            {/* Discrete Trust Badge */}
                                                            {person.trust === 'trusted' ? (
                                                                <span title={t('marketplace.trustTrusted')} className="shrink-0 inline-flex items-center text-emerald-600">
                                                                    <CheckCircle2 size={13} />
                                                                </span>
                                                            ) : (
                                                                <span title={t('marketplace.trustUnknown')} className="shrink-0 inline-flex items-center text-slate-400">
                                                                    <HelpCircle size={13} />
                                                                </span>
                                                            )}

                                                            {/* Source icon */}
                                                            {person.sources.length > 0 && person.sources[0].kind === 'voucher' && (
                                                                <span title={t('marketplace.sourceVoucher')} className="shrink-0 inline-flex items-center text-amber-600">
                                                                    <Ticket size={12} />
                                                                </span>
                                                            )}
                                                            {formattedDate && (
                                                                <span className="shrink-0 text-[10px] text-theme-light opacity-75">
                                                                    {t('marketplace.asOf', { date: formattedDate })}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {/* Actions & Inquiry Status (§6.4.3) */}
                                                        <div className="flex items-center gap-2 shrink-0">
                                                            {(() => {
                                                                const buyerInquiry = buyerInquiries.find(i => i.listingId === listing.listingId);
                                                                const isOwn = person.did === ownDid;

                                                                if (buyerInquiry) {
                                                                    if (!buyerInquiry.reply) {
                                                                        return (
                                                                            <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-amber-50 text-amber-800 border border-amber-200">
                                                                                {t('marketplace.inquiryWaiting', 'Wartet auf Antwort')}
                                                                            </span>
                                                                        );
                                                                    }
                                                                    if (buyerInquiry.reply.decision === 'confirm') {
                                                                        return (
                                                                            <div className="flex items-center gap-1.5">
                                                                                <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-50 text-emerald-800 border border-emerald-200">
                                                                                    {t('marketplace.inquiryConfirmed', 'Bestätigt')}
                                                                                </span>
                                                                                <Button
                                                                                    size="xs"
                                                                                    variant="primary"
                                                                                    onClick={(e) => {
                                                                                        e.stopPropagation();
                                                                                        navigate({ view: 'send_vouchers', initialRecipientId: person.did, initialNote: buyerInquiry.reply?.note || listing.title });
                                                                                    }}
                                                                                    data-testid="pay-confirmed-listing-button"
                                                                                    className="bg-emerald-600 hover:bg-emerald-700 text-white gap-1 text-[11px]"
                                                                                >
                                                                                    <Send size={11} />
                                                                                    <span>{t('marketplace.payNowBtn', 'Jetzt bezahlen')}</span>
                                                                                </Button>
                                                                            </div>
                                                                        );
                                                                    }
                                                                    if (buyerInquiry.reply.decision === 'decline') {
                                                                        return (
                                                                            <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-rose-50 text-rose-800 border border-rose-200">
                                                                                {t('marketplace.inquiryDeclined', 'Abgesagt')}
                                                                            </span>
                                                                        );
                                                                    }
                                                                }

                                                                return (
                                                                    <>
                                                                        {!isOwn && listing.allowInquiries && person.cardDrops && person.cardDrops.length > 0 && (
                                                                            <Button
                                                                                size="xs"
                                                                                variant="outline"
                                                                                onClick={(e) => {
                                                                                    e.stopPropagation();
                                                                                    setInquiryTarget({ person, listing });
                                                                                    setInquiryMessage('');
                                                                                }}
                                                                                data-testid="inquiry-button"
                                                                                className="gap-1 text-[11px] border-theme-subtle"
                                                                            >
                                                                                <MessageSquare size={11} />
                                                                                <span>{t('marketplace.inquiryButton', 'Reservieren / Anfragen')}</span>
                                                                            </Button>
                                                                        )}
                                                                        <Button
                                                                            size="xs"
                                                                            variant="outline"
                                                                            onClick={(e) => {
                                                                                e.stopPropagation();
                                                                                navigate({ view: 'send_vouchers', initialRecipientId: person.did, initialNote: listing.title });
                                                                            }}
                                                                            className="gap-1 text-[11px]"
                                                                        >
                                                                            <Send size={11} />
                                                                            <span>{t('marketplace.pay', 'Bezahlen')}</span>
                                                                        </Button>
                                                                    </>
                                                                );
                                                            })()}
                                                        </div>
                                                    </div>
                                                </Card>
                                            );
                                        })}
                                    </div>
                                )}
                            </div>
                        )}

                        {/* Personen / People Tab */}
                        {activeTab === 'people' && (
                            <div className="space-y-3">
                                {filteredPeople.length === 0 ? (
                                    <PageEmptyState
                                        title={t('marketplace.noMatchingFilter')}
                                        action={hasActiveFilters ? (
                                            <Button variant="outline" size="sm" onClick={handleResetFilters}>
                                                {t('marketplace.resetFilter')}
                                            </Button>
                                        ) : undefined}
                                    />
                                ) : (
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                        {filteredPeople.map((person) => {
                                            const personName = getDisplayName(person);
                                            const formattedDate = person.asOf ? new Date(person.asOf * 1000).toLocaleDateString() : '';
                                            const offersCount = person.listings.filter((l) => l.listingType === 'offer').length;
                                            const requestsCount = person.listings.filter((l) => l.listingType === 'request').length;

                                            return (
                                                <Card
                                                    key={person.did}
                                                    onClick={() => setSelectedPersonDid(person.did)}
                                                    className="p-5 bg-white border border-theme-subtle hover:border-theme-primary/30 rounded-3xl shadow-soft hover:shadow-md transition-all cursor-pointer flex flex-col justify-between gap-4 group"
                                                >
                                                    <div className="space-y-3">
                                                        <div className="flex items-start justify-between gap-3">
                                                            <div className="flex items-center gap-3 min-w-0">
                                                                <div className="shrink-0 ring-2 ring-theme-primary/10 rounded-full p-0.5 bg-white">
                                                                    <Avatar
                                                                        size={44}
                                                                        name={personName}
                                                                        variant="beam"
                                                                        colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
                                                                    />
                                                                </div>
                                                                <div className="min-w-0">
                                                                    <h3 className="text-base font-bold text-theme-secondary group-hover:text-theme-primary transition-colors truncate">
                                                                        {personName}
                                                                    </h3>
                                                                    {(person.profile.organization || person.profile.community) && (
                                                                        <p className="text-[11px] font-medium text-theme-light truncate">
                                                                            {person.profile.organization || person.profile.community}
                                                                        </p>
                                                                    )}
                                                                </div>
                                                            </div>

                                                            {/* Discrete Trust Badge */}
                                                            {person.trust === 'trusted' ? (
                                                                <span className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[10px] font-bold bg-emerald-50 text-emerald-800 border border-emerald-200">
                                                                    <CheckCircle2 size={11} className="text-emerald-600" />
                                                                    {t('marketplace.trustTrusted')}
                                                                </span>
                                                            ) : (
                                                                <span className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[10px] font-bold bg-slate-100 text-slate-700 border border-slate-200">
                                                                    <HelpCircle size={11} className="text-slate-500" />
                                                                    {t('marketplace.trustUnknown')}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {/* Standards tags */}
                                                        {person.standards.length > 0 && (
                                                            <div className="flex flex-wrap gap-1.5">
                                                                {person.standards.map((std) => (
                                                                    <span
                                                                        key={std.uuid}
                                                                        className="inline-flex items-center gap-1 px-2 py-0.5 bg-bg-app border border-theme-subtle rounded-lg text-[10px] font-semibold text-theme-secondary"
                                                                    >
                                                                        <Tag size={10} className="text-theme-light" />
                                                                        {std.name}
                                                                    </span>
                                                                ))}
                                                            </div>
                                                        )}

                                                        {/* Status note */}
                                                        <div className="text-xs text-theme-secondary/80">
                                                            {person.listings.length === 0 ? (
                                                                <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-theme-primary bg-theme-primary/5 px-2.5 py-1 rounded-xl">
                                                                    <Store size={12} />
                                                                    {t('marketplace.acceptsOwnVouchers')}
                                                                </span>
                                                            ) : (
                                                                <span className="text-[11px] font-medium text-theme-light">
                                                                    {offersCount > 0 && `${offersCount} ${t('marketplace.tabOffers')}`}
                                                                    {offersCount > 0 && requestsCount > 0 && ' · '}
                                                                    {requestsCount > 0 && `${requestsCount} ${t('marketplace.tabRequests')}`}
                                                                </span>
                                                            )}
                                                        </div>
                                                    </div>

                                                    <div className="pt-3 border-t border-theme-subtle/40 flex items-center justify-between gap-3">
                                                        {formattedDate ? (
                                                            <span className="text-[10px] text-theme-light opacity-75">
                                                                {t('marketplace.asOf', { date: formattedDate })}
                                                            </span>
                                                        ) : <span />}

                                                        {/* Direct Pay Action */}
                                                        <Button
                                                            size="sm"
                                                            onClick={(e) => {
                                                                e.stopPropagation();
                                                                navigate({ view: 'send_vouchers', initialRecipientId: person.did });
                                                            }}
                                                            className="gap-1.5 text-xs shadow-xs"
                                                        >
                                                            <Send size={13} />
                                                            <span>{t('marketplace.pay')}</span>
                                                        </Button>
                                                    </div>
                                                </Card>
                                            );
                                        })}
                                    </div>
                                )}
                            </div>
                        )}
                    </>
                )}
            </div>

            {/* My Offer Modal */}
            <MyOfferModal
                isOpen={isMyOfferModalOpen}
                onClose={() => setIsMyOfferModalOpen(false)}
                onCardUpdated={loadMarketView}
            />

            {/* Import Card Modal */}
            <ImportCardModal
                isOpen={isImportModalOpen}
                onClose={() => setIsImportModalOpen(false)}
                onImportSuccess={(did) => {
                    loadMarketView();
                    if (did) {
                        setSelectedPersonDid(did);
                    }
                }}
            />

            {/* Drop Management Modal */}
            <DropManagementModal
                isOpen={isDropModalOpen}
                onClose={() => setIsDropModalOpen(false)}
                onSyncComplete={(importedCards) => {
                    if (importedCards > 0) {
                        loadMarketView();
                    }
                }}
            />

            {/* 1-Step Inquiry Modal (§6.4.3 - NO Chat! max 280 chars) */}
            {inquiryTarget && (
                <ModalShell
                    isOpen={!!inquiryTarget}
                    onClose={() => { setInquiryTarget(null); setInquiryMessage(''); setInquirySendError(null); }}
                    title={t('marketplace.inquiryModalTitle', 'Anfrage senden')}
                >
                    <div className="space-y-4 p-2">
                        <div className="p-3 bg-bg-app rounded-xl border border-theme-subtle space-y-1">
                            <span className="text-[10px] font-black uppercase tracking-wider text-theme-light">{t('marketplace.targetListing', 'Angebot')}</span>
                            <p className="font-bold text-sm text-theme-primary">{inquiryTarget.listing.title}</p>
                            <p className="text-xs text-theme-light">
                                {t('marketplace.toRecipient', 'An: {{name}}', { name: getDisplayName(inquiryTarget.person) })}
                            </p>
                        </div>

                        <div className="space-y-1.5">
                            <div className="flex items-center justify-between text-xs">
                                <label className="font-bold text-theme-secondary">{t('marketplace.inquiryMessageLabel', 'Deine kurze Nachricht')}</label>
                                <span className={`text-[10px] font-mono ${inquiryMessage.length > 280 ? 'text-rose-600 font-bold' : 'text-theme-light'}`}>
                                    {inquiryMessage.length} / 280
                                </span>
                            </div>
                            <Textarea
                                value={inquiryMessage}
                                onChange={(e) => setInquiryMessage(e.target.value.slice(0, 280))}
                                placeholder={t('marketplace.inquiryMessagePlaceholder', 'z.B. Ich würde das Angebot gerne reservieren / anfragen. Wann passt es dir?')}
                                rows={3}
                                className="text-xs"
                                autoFocus
                                data-testid="inquiry-message-input"
                            />
                        </div>

                        {inquirySendError && (
                            <div className="p-2.5 bg-rose-50 text-rose-600 rounded-xl text-xs font-semibold border border-rose-200">
                                {inquirySendError}
                            </div>
                        )}

                        <div className="flex items-center justify-end gap-2 pt-2">
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={() => { setInquiryTarget(null); setInquiryMessage(''); setInquirySendError(null); }}
                            >
                                {t('common.cancel', 'Abbrechen')}
                            </Button>
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={!inquiryMessage.trim() || isSendingInquiry}
                                onClick={handleSendInquiry}
                                data-testid="send-inquiry-confirm-button"
                                className="gap-1.5"
                            >
                                {isSendingInquiry ? <RefreshCw size={14} className="animate-spin" /> : <Send size={14} />}
                                <span>{isSendingInquiry ? t('marketplace.sendingInquiry', 'Sende...') : t('marketplace.sendInquiryBtn', 'Anfrage absenden')}</span>
                            </Button>
                        </div>
                    </div>
                </ModalShell>
            )}

            {/* 1-Step Reply Modal (§6.4.3 - strictly Confirm or Decline) */}
            {replyingInquiry && (
                <ModalShell
                    isOpen={!!replyingInquiry}
                    onClose={() => { setReplyingInquiry(null); setReplyNote(''); }}
                    title={replyDecision === 'confirm' ? t('marketplace.replyConfirmTitle', 'Anfrage bestätigen') : t('marketplace.replyDeclineTitle', 'Anfrage absagen')}
                >
                    <div className="space-y-4 p-2">
                        <div className="p-3 bg-bg-app rounded-xl border border-theme-subtle space-y-1">
                            <p className="font-bold text-sm text-theme-primary">{replyingInquiry.listingTitle}</p>
                            <p className="text-xs text-theme-secondary italic">"{replyingInquiry.message}"</p>
                        </div>

                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary">
                                {replyDecision === 'confirm' 
                                    ? t('marketplace.confirmNoteLabel', 'Optionaler Hinweis oder Zahlungsdetails für den Käufer:')
                                    : t('marketplace.declineNoteLabel', 'Optionaler Grund für die Absage:')}
                            </label>
                            <Input
                                value={replyNote}
                                onChange={(e) => setReplyNote(e.target.value)}
                                placeholder={replyDecision === 'confirm' 
                                    ? t('marketplace.confirmNotePlaceholder', 'z.B. Abholung morgen ab 18:00 Uhr möglich.')
                                    : t('marketplace.declineNotePlaceholder', 'z.B. Leider bereits vergeben.')}
                                className="text-xs"
                                autoFocus
                                data-testid="reply-note-input"
                            />
                        </div>

                        <div className="flex items-center justify-end gap-2 pt-2">
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={() => { setReplyingInquiry(null); setReplyNote(''); }}
                            >
                                {t('common.cancel', 'Abbrechen')}
                            </Button>
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={isReplyingInquiry}
                                onClick={handleExecuteReply}
                                data-testid="execute-reply-button"
                                className={replyDecision === 'confirm' ? 'bg-emerald-600 hover:bg-emerald-700 text-white' : 'bg-rose-600 hover:bg-rose-700 text-white'}
                            >
                                {isReplyingInquiry ? <RefreshCw size={14} className="animate-spin" /> : <Check size={14} />}
                                <span>{replyDecision === 'confirm' ? t('marketplace.inquiryConfirmBtn', 'Bestätigen') : t('marketplace.inquiryDeclineBtn', 'Absagen')}</span>
                            </Button>
                        </div>
                    </div>
                </ModalShell>
            )}
        </PageLayout>
    );
}
