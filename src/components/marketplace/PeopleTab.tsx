// src/components/marketplace/PeopleTab.tsx
import React, { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, User, MapPin, Tag, Briefcase, RotateCcw } from 'lucide-react';
import { FrontendMarketPerson } from '../../types';
import { TrustBadge } from '../ui/TrustBadge';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { truncateUserId } from '../../utils/userIdHelper';

export interface PeopleTabProps {
    people: FrontendMarketPerson[];
    onSelectPerson: (person: FrontendMarketPerson) => void;
}

export const PeopleTab: React.FC<PeopleTabProps> = ({
    people,
    onSelectPerson,
}) => {
    const { t } = useTranslation();
    const [searchQuery, setSearchQuery] = useState('');

    const filteredPeople = useMemo(() => {
        if (!searchQuery.trim()) return people;
        const query = searchQuery.trim().toLowerCase();

        return people.filter((p) => {
            const nameParts = [p.profile.firstName, p.profile.lastName].filter(Boolean).join(' ').toLowerCase();
            const org = p.profile.organization?.toLowerCase() || '';
            const community = p.profile.community?.toLowerCase() || '';
            const city = p.profile.address?.city?.toLowerCase() || '';

            const skillMatch = p.skills.some(
                (s) => s.title.toLowerCase().includes(query) || (s.text && s.text.toLowerCase().includes(query))
            );

            return (
                nameParts.includes(query) ||
                org.includes(query) ||
                community.includes(query) ||
                city.includes(query) ||
                skillMatch
            );
        });
    }, [people, searchQuery]);

    const isPeopleEmptyOverall = people.length === 0;

    return (
        <div className="space-y-6">
            {/* Search input: "Wer kann ...?" */}
            {!isPeopleEmptyOverall && (
                <div className="relative max-w-md">
                    <Search
                        size={16}
                        className="absolute left-3.5 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none"
                    />
                    <Input
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder={t('marketplace.searchPeoplePlaceholder', 'Wer kann...? Fähigkeiten, Namen oder Ort')}
                        className="pl-9 bg-white border-theme-subtle"
                        data-testid="people-search-input"
                    />
                </div>
            )}

            {filteredPeople.length > 0 ? (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {filteredPeople.map((person) => {
                        const nameParts = [person.profile.firstName, person.profile.lastName].filter(Boolean);
                        const displayName =
                            nameParts.length > 0
                                ? nameParts.join(' ')
                                : person.profile.organization || truncateUserId(person.did);

                        const locationParts = [person.profile.address?.city, person.profile.community].filter(Boolean);
                        const locationText = locationParts.join(' · ');

                        const topSkills = person.skills.slice(0, 3);
                        const activeAdsCount = person.activeAds.length;

                        return (
                            <div
                                key={person.did}
                                onClick={() => onSelectPerson(person)}
                                className="p-4 sm:p-5 rounded-2xl bg-white border border-theme-subtle hover:border-theme-primary/40 hover:shadow-sm transition-all duration-200 cursor-pointer flex flex-col justify-between gap-4"
                                data-testid={`person-card-${person.did}`}
                            >
                                <div className="space-y-3">
                                    {/* Header: Avatar, Name, Location & Trust Badge */}
                                    <div className="flex flex-wrap items-start justify-between gap-2">
                                        <div className="flex items-center gap-3 min-w-[140px] flex-1">
                                            <div className="w-11 h-11 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center font-bold text-sm shrink-0 overflow-hidden">
                                                {person.profile.pictureUrl ? (
                                                    <img
                                                        src={person.profile.pictureUrl}
                                                        alt={displayName}
                                                        className="w-full h-full object-cover"
                                                    />
                                                ) : (
                                                    displayName.charAt(0).toUpperCase() || <User size={18} />
                                                )}
                                            </div>
                                            <div className="min-w-0 flex-1">
                                                <h3 className="text-sm font-bold text-theme-secondary truncate" title={displayName}>
                                                    {displayName}
                                                </h3>
                                                {locationText && (
                                                    <p className="text-[11px] text-theme-light truncate flex items-center gap-1 mt-0.5" title={locationText}>
                                                        <MapPin size={11} className="shrink-0 text-theme-primary" />
                                                        <span>{locationText}</span>
                                                    </p>
                                                )}
                                            </div>
                                        </div>

                                        <div className="shrink-0">
                                            <TrustBadge
                                                trust={{
                                                    score: person.trust === 'trusted' ? 0.85 : 0,
                                                    trustPercent: person.trust === 'trusted' ? 85 : 0,
                                                    isOwnVoucher: false,
                                                    blockedByOffender: false,
                                                    additionalPathsCount: 0,
                                                    viaGuarantor: false,
                                                    bestPathNames: [],
                                                }}
                                                size="sm"
                                            />
                                        </div>
                                    </div>

                                    {/* Active ads badge */}
                                    {activeAdsCount > 0 && (
                                        <div className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-emerald-50 text-emerald-800 border border-emerald-200 text-[11px] font-semibold">
                                            <Tag size={11} />
                                            <span>
                                                {activeAdsCount === 1
                                                    ? t('marketplace.oneActiveAdBadge', '1 aktives Inserat')
                                                    : t('marketplace.activeAdsBadge', '{{count}} aktive Inserate', { count: activeAdsCount })}
                                            </span>
                                        </div>
                                    )}

                                    {/* Up to 3 skill chips */}
                                    {topSkills.length > 0 && (
                                        <div className="space-y-1.5 pt-1">
                                            <div className="flex flex-wrap gap-1.5">
                                                {topSkills.map((skill, idx) => (
                                                    <span
                                                        key={idx}
                                                        className="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg bg-bg-app border border-theme-subtle text-[11px] font-medium text-theme-secondary truncate max-w-full"
                                                    >
                                                        <Briefcase size={10} className="text-theme-primary shrink-0" />
                                                        <span className="truncate">{skill.title}</span>
                                                    </span>
                                                ))}
                                                {person.skills.length > 3 && (
                                                    <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] text-theme-light">
                                                        +{person.skills.length - 3}
                                                    </span>
                                                )}
                                            </div>
                                        </div>
                                    )}
                                </div>

                                <div className="text-[11px] text-theme-primary font-semibold flex items-center justify-end">
                                    <span>{t('marketplace.viewProfileBtn', 'Visitenkarte ansehen →')}</span>
                                </div>
                            </div>
                        );
                    })}
                </div>
            ) : isPeopleEmptyOverall ? (
                <div className="p-8 sm:p-12 rounded-3xl bg-white border border-theme-subtle text-center space-y-3 max-w-md mx-auto my-6">
                    <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                        <User size={24} />
                    </div>
                    <h3 className="text-base font-bold text-theme-secondary">
                        {t('marketplace.noPeopleTitle', 'Noch keine Personen im Umfeld')}
                    </h3>
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('marketplace.noPeopleDesc', 'Sobald du Visitenkarten von Kontakten importierst oder über den Sync-Dienst empfängst, erscheinen die Menschen hier.')}
                    </p>
                </div>
            ) : (
                <div className="p-8 rounded-2xl bg-bg-app border border-theme-subtle text-center space-y-3 max-w-md mx-auto my-6">
                    <p className="text-xs text-theme-light">
                        {t('marketplace.noPeopleFilterResults', 'Keine Personen entsprechen deiner Suche.')}
                    </p>
                    <Button
                        size="xs"
                        variant="outline"
                        onClick={() => setSearchQuery('')}
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
