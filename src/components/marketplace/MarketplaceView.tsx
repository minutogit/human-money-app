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
    Server
} from 'lucide-react';
import { FrontendMarketPerson, FrontendMarketListing } from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { PageLayout } from '../ui/PageLayout';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
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

    const [people, setPeople] = useState<FrontendMarketPerson[]>([]);
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

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
            const data = await marketService.getMarketView();
            setPeople(data || []);
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
    }, [loadMarketView, triggerDropSync]);

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
                                                        </div>

                                                        {/* Date */}
                                                        {formattedDate && (
                                                            <span className="shrink-0 text-[10px] text-theme-light opacity-75">
                                                                {t('marketplace.asOf', { date: formattedDate })}
                                                            </span>
                                                        )}
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
                                                        </div>

                                                        {/* Date */}
                                                        {formattedDate && (
                                                            <span className="shrink-0 text-[10px] text-theme-light opacity-75">
                                                                {t('marketplace.asOf', { date: formattedDate })}
                                                            </span>
                                                        )}
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
        </PageLayout>
    );
}
