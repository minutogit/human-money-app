// src/components/marketplace/BoardTab.tsx
import React, { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, Plus, Cloud, FileUp, RotateCcw, MapPin } from 'lucide-react';
import { Ad, FrontendBoardItem, FrontendMarketPerson } from '../../types';
import { AdCard, resolveAuthorHops } from './AdCard';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { calculateDistanceKm, normalizeCoordinates } from '../../utils/geoUtils';

export interface BoardTabProps {
    items: FrontendBoardItem[];
    people?: FrontendMarketPerson[];
    myCoordinates?: string | null;
    maxDistanceKm?: number | null;
    onMaxDistanceChange?: (val: number | null) => void;
    hasSyncService: boolean;
    onCreateAd: () => void;
    onImportCard: () => void;
    onNavigateToSyncSettings: () => void;
    onInquire?: (ad: Ad, ownerDid: string, ownerName: string) => void;
    onPay?: (ownerDid: string) => void;
    onOwnerClick?: (ownerDid: string, isOwn?: boolean) => void;
    onEditOwnAd?: (ad: Ad) => void;
    mediaAutoLoadMaxHops?: number;
    wifiOnly?: boolean;
}

export type BoardFilter = 'all' | 'offer' | 'request' | 'event';

export const BoardTab: React.FC<BoardTabProps> = ({
    items,
    people,
    myCoordinates,
    maxDistanceKm,
    onMaxDistanceChange,
    hasSyncService,
    onCreateAd,
    onImportCard,
    onNavigateToSyncSettings,
    onInquire,
    onPay,
    onOwnerClick,
    onEditOwnAd,
    mediaAutoLoadMaxHops = 2,
    wifiOnly = false,
}) => {
    const { t } = useTranslation();
    const [searchQuery, setSearchQuery] = useState('');
    const [filterKind, setFilterKind] = useState<BoardFilter>('all');
    const [internalMaxDistance, setInternalMaxDistance] = useState<number | null>(null);

    const effectiveMaxDistance = maxDistanceKm !== undefined ? maxDistanceKm : internalMaxDistance;
    const setEffectiveMaxDistance = onMaxDistanceChange || setInternalMaxDistance;
    const hasCoords = Boolean(myCoordinates && normalizeCoordinates(myCoordinates));

    const hopsByDid = useMemo(() => {
        const map = new Map<string, number>();
        if (!people) return map;
        for (const p of people) {
            const hops = resolveAuthorHops(p.sources);
            if (p.recipientDid) {
                map.set(p.recipientDid, hops);
            }
            if (!map.has(p.did)) {
                map.set(p.did, hops);
            }
        }
        return map;
    }, [people]);

    const filteredItems = useMemo(() => {
        const result = items.filter((item) => {
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
                if (!titleMatch && !textMatch && !ownerMatch) {
                    return false;
                }
            }
            // Distance filter
            if (effectiveMaxDistance !== null && effectiveMaxDistance > 0 && hasCoords) {
                if (item.isOwn) {
                    return true;
                }
                if (!item.ad.isRemote) {
                    const targetDid = item.recipientDid || item.ownerDid;
                    const ownerPerson =
                        (people || []).find((p) => (p.recipientDid || p.did) === targetDid) ||
                        (people || []).find((p) => p.did === item.ownerDid);
                    const ownerCoords = ownerPerson?.profile.coordinates;
                    if (!ownerCoords) {
                        return false;
                    }
                    const dist = calculateDistanceKm(myCoordinates, ownerCoords);
                    if (dist === null || dist > effectiveMaxDistance) {
                        return false;
                    }
                }
            }
            return true;
        });

        if (filterKind === 'event') {
            return [...result].sort((a, b) => {
                const timeA = a.ad.eventAt ?? a.ad.expiresAt;
                const timeB = b.ad.eventAt ?? b.ad.expiresAt;
                return timeA !== timeB ? timeA - timeB : a.ad.id.localeCompare(b.ad.id);
            });
        }

        return result;
    }, [items, people, filterKind, searchQuery, effectiveMaxDistance, myCoordinates, hasCoords]);

    const isBoardEmptyOverall = items.length === 0;

    return (
        <div className="space-y-6">
            {/* Subtle section subtitle */}
            {!isBoardEmptyOverall && (
                <p className="text-xs font-semibold text-theme-light" data-testid="board-subtitle">
                    {t('marketplace.tabBoardSubtitle', 'Aktuelle Anzeigen')}
                </p>
            )}

            {/* Missing coords hint when coordinates are missing */}
            {((!hasCoords && !isBoardEmptyOverall) || (effectiveMaxDistance !== null && effectiveMaxDistance > 0 && !hasCoords)) && (
                <div
                    className="p-3 rounded-xl bg-amber-50 border border-amber-200 text-amber-900 text-xs flex items-center gap-2"
                    data-testid="missing-coordinates-hint"
                >
                    <MapPin size={14} className="text-amber-600 shrink-0" />
                    <span>
                        {t(
                            'marketplace.radiusDisabledNoLocationHint',
                            'Standort in deiner Visitenkarte angeben, um nach Umkreis zu filtern.'
                        )}
                    </span>
                </div>
            )}

            {/* Filter toolbar: Search input + Radius select + Kind chips */}
            {!isBoardEmptyOverall && (
                <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
                    {/* Search box & Radius select */}
                    <div className="flex items-center gap-2 flex-1 max-w-xl">
                        <div className="relative flex-1">
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
                        <select
                            value={effectiveMaxDistance === null ? '' : String(effectiveMaxDistance)}
                            onChange={(e) => setEffectiveMaxDistance(e.target.value === '' ? null : Number(e.target.value))}
                            disabled={!hasCoords}
                            title={!hasCoords ? t('marketplace.radiusDisabledNoLocationHint', 'Standort in deiner Visitenkarte angeben, um nach Umkreis zu filtern.') : undefined}
                            className="px-3 py-2 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer shrink-0 disabled:opacity-50 disabled:bg-slate-100 disabled:cursor-not-allowed"
                            data-testid="board-radius-select"
                            aria-label={t('marketplace.radiusLabel', 'Umkreis')}
                        >
                            <option value="">{t('marketplace.radiusAll', 'Alle Entfernungen')}</option>
                            <option value="10">{t('marketplace.radius10', 'Umkreis ≤ 10 km')}</option>
                            <option value="25">{t('marketplace.radius25', 'Umkreis ≤ 25 km')}</option>
                            <option value="50">{t('marketplace.radius50', 'Umkreis ≤ 50 km')}</option>
                            <option value="100">{t('marketplace.radius100', 'Umkreis ≤ 100 km')}</option>
                        </select>
                    </div>

                    {/* Filter chips: Alle / Angebote / Gesuche / Termine */}
                    <div className="inline-flex flex-wrap rounded-xl p-1 bg-bg-app border border-theme-subtle self-start sm:self-auto">
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
                        <button
                            type="button"
                            onClick={() => setFilterKind('event')}
                            className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all ${
                                filterKind === 'event'
                                    ? 'bg-white text-violet-700 shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                            data-testid="filter-chip-events"
                        >
                            {t('marketplace.filterEvents', 'Termine')}
                        </button>
                    </div>
                </div>
            )}

            {/* List or Empty States */}
            {filteredItems.length > 0 ? (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {filteredItems.map((item) => {
                        const targetOwnerDid = item.recipientDid || item.ownerDid;
                        return (
                            <AdCard
                                key={item.ad.id}
                                ad={item.ad}
                                ownerDid={targetOwnerDid}
                                ownerName={item.ownerName}
                                trust={item.trust}
                                daysLeft={item.daysLeft}
                                isOwn={item.isOwn}
                                authorHops={
                                    item.isOwn
                                        ? 0
                                        : (hopsByDid.get(targetOwnerDid) ?? hopsByDid.get(item.ownerDid) ?? 99)
                                }
                                mediaAutoLoadMaxHops={mediaAutoLoadMaxHops}
                                wifiOnly={wifiOnly}
                                onEditOwnAd={onEditOwnAd}
                                onInquire={onInquire}
                                onPay={onPay ? () => onPay(targetOwnerDid) : undefined}
                                onOwnerClick={onOwnerClick}
                            />
                        );
                    })}
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
                            setEffectiveMaxDistance(null);
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
