// src/components/marketplace/BoardTab.tsx
import React, { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, Plus, Cloud, FileUp, RotateCcw } from 'lucide-react';
import { Ad, FrontendBoardItem } from '../../types';
import { AdCard } from './AdCard';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';

export interface BoardTabProps {
    items: FrontendBoardItem[];
    hasSyncService: boolean;
    onCreateAd: () => void;
    onImportCard: () => void;
    onNavigateToSyncSettings: () => void;
    onInquire?: (ad: Ad, ownerDid: string, ownerName: string) => void;
    onPay?: (ownerDid: string) => void;
    onOwnerClick?: (ownerDid: string) => void;
}

export type BoardFilter = 'all' | 'offer' | 'request';

export const BoardTab: React.FC<BoardTabProps> = ({
    items,
    hasSyncService,
    onCreateAd,
    onImportCard,
    onNavigateToSyncSettings,
    onInquire,
    onPay,
    onOwnerClick,
}) => {
    const { t } = useTranslation();
    const [searchQuery, setSearchQuery] = useState('');
    const [filterKind, setFilterKind] = useState<BoardFilter>('all');

    const filteredItems = useMemo(() => {
        return items.filter((item) => {
            // Kind filter
            if (filterKind !== 'all' && item.ad.kind !== filterKind) {
                return false;
            }
            // Search query filter (title & text)
            if (searchQuery.trim()) {
                const query = searchQuery.trim().toLowerCase();
                const titleMatch = item.ad.title.toLowerCase().includes(query);
                const textMatch = item.ad.text ? item.ad.text.toLowerCase().includes(query) : false;
                const ownerMatch = item.ownerName.toLowerCase().includes(query);
                return titleMatch || textMatch || ownerMatch;
            }
            return true;
        });
    }, [items, filterKind, searchQuery]);

    const isBoardEmptyOverall = items.length === 0;

    return (
        <div className="space-y-6">
            {/* Filter toolbar: Search input + Kind chips */}
            {!isBoardEmptyOverall && (
                <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
                    {/* Search box */}
                    <div className="relative flex-1 max-w-md">
                        <Search
                            size={16}
                            className="absolute left-3.5 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none"
                        />
                        <Input
                            value={searchQuery}
                            onChange={(e) => setSearchQuery(e.target.value)}
                            placeholder={t('marketplace.searchBoardPlaceholder', 'Pinnwand durchsuchen...')}
                            className="pl-9 bg-white border-theme-subtle"
                            data-testid="board-search-input"
                        />
                    </div>

                    {/* Filter chips: Alle / Angebote / Gesuche */}
                    <div className="inline-flex rounded-xl p-1 bg-bg-app border border-theme-subtle self-start sm:self-auto">
                        <button
                            type="button"
                            onClick={() => setFilterKind('all')}
                            className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all ${
                                filterKind === 'all'
                                    ? 'bg-white text-theme-primary shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                            data-testid="filter-chip-all"
                        >
                            {t('marketplace.filterAll', 'Alle')}
                        </button>
                        <button
                            type="button"
                            onClick={() => setFilterKind('offer')}
                            className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all ${
                                filterKind === 'offer'
                                    ? 'bg-white text-emerald-700 shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                            data-testid="filter-chip-offers"
                        >
                            {t('marketplace.filterOffers', 'Angebote')}
                        </button>
                        <button
                            type="button"
                            onClick={() => setFilterKind('request')}
                            className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all ${
                                filterKind === 'request'
                                    ? 'bg-white text-sky-700 shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                            data-testid="filter-chip-requests"
                        >
                            {t('marketplace.filterRequests', 'Gesuche')}
                        </button>
                    </div>
                </div>
            )}

            {/* List or Empty States */}
            {filteredItems.length > 0 ? (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {filteredItems.map((item) => (
                        <AdCard
                            key={item.ad.id}
                            ad={item.ad}
                            ownerDid={item.ownerDid}
                            ownerName={item.ownerName}
                            trust={item.trust}
                            daysLeft={item.daysLeft}
                            onInquire={onInquire}
                            onPay={onPay}
                            onOwnerClick={onOwnerClick}
                        />
                    ))}
                </div>
            ) : isBoardEmptyOverall ? (
                /* Empty state when the board has zero items */
                !hasSyncService ? (
                    <div className="p-8 sm:p-12 rounded-3xl bg-white border border-theme-subtle text-center space-y-4 max-w-xl mx-auto my-6">
                        <div className="w-14 h-14 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                            <Cloud size={28} />
                        </div>
                        <div className="space-y-1.5">
                            <h3 className="text-base font-bold text-theme-secondary">
                                {t('marketplace.emptyBoardNoSyncTitle', 'Keine Inserate vorhanden')}
                            </h3>
                            <p className="text-xs text-theme-light leading-relaxed max-w-md mx-auto">
                                {t(
                                    'marketplace.emptyBoardNoSyncDesc',
                                    'Ohne Sync-Dienst siehst du nur Inserate von Visitenkarten, die du direkt bekommen hast.'
                                )}
                            </p>
                        </div>
                        <div className="flex flex-wrap items-center justify-center gap-3 pt-2">
                            <Button variant="primary" onClick={onNavigateToSyncSettings} className="flex items-center gap-2">
                                <Cloud size={16} />
                                <span>{t('marketplace.setupSyncBtn', 'Sync-Dienst einrichten')}</span>
                            </Button>
                            <Button variant="outline" onClick={onImportCard} className="flex items-center gap-2">
                                <FileUp size={16} />
                                <span>{t('marketplace.openCardFromFileBtn', 'Visitenkarte aus Datei öffnen')}</span>
                            </Button>
                        </div>
                    </div>
                ) : (
                    <div className="p-8 sm:p-12 rounded-3xl bg-white border border-theme-subtle text-center space-y-4 max-w-xl mx-auto my-6">
                        <div className="w-14 h-14 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                            <Plus size={28} />
                        </div>
                        <div className="space-y-1.5">
                            <h3 className="text-base font-bold text-theme-secondary">
                                {t('marketplace.emptyBoardWithSyncTitle', 'Noch keine Inserate in deinem Umfeld')}
                            </h3>
                            <p className="text-xs text-theme-light leading-relaxed max-w-md mx-auto">
                                {t('marketplace.emptyBoardWithSyncDesc', 'Noch keine Inserate in deinem Umfeld. Sei der Erste!')}
                            </p>
                        </div>
                        <div className="pt-2">
                            <Button variant="primary" onClick={onCreateAd} className="flex items-center gap-2">
                                <Plus size={16} />
                                <span>{t('marketplace.createAdBtn', 'Inserat aufgeben')}</span>
                            </Button>
                        </div>
                    </div>
                )
            ) : (
                /* Empty state when filter/search yields no results */
                <div className="p-8 rounded-2xl bg-bg-app border border-theme-subtle text-center space-y-3 max-w-md mx-auto my-6">
                    <p className="text-xs text-theme-light">
                        {t('marketplace.noFilterResults', 'Keine Inserate entsprechen deinen Suchkriterien.')}
                    </p>
                    <Button
                        size="xs"
                        variant="outline"
                        onClick={() => {
                            setSearchQuery('');
                            setFilterKind('all');
                        }}
                        className="flex items-center gap-1.5 mx-auto"
                    >
                        <RotateCcw size={13} />
                        <span>{t('marketplace.resetFilterBtn', 'Filter zurücksetzen')}</span>
                    </Button>
                </div>
            )}
        </div>
    );
};
