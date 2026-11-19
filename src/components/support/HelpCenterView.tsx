// src/components/HelpCenterView.tsx
import React, { useState, useMemo, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { useUiMode } from '../../context/UiModeContext';
import { AuthLayout } from '../auth/AuthLayout';
import { PageLayout } from '../ui/PageLayout';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { BackButton } from '../ui/BackButton';
import { Disclosure } from '../ui/Disclosure';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import {
    HELP_TOPICS,
    HELP_GROUPS,
    HelpGroup,
    HelpTopicId,
    HelpTopic,
} from '../../help/helpTopics';
import { HelpContentRenderer } from '../../help/HelpContentRenderer';
import {
    Search,
    X,
    BookOpen,
    ChevronRight,
    ArrowLeft,
    Compass,
    Heart,
    Terminal,
} from 'lucide-react';

interface HelpCenterViewProps {
    initialTopic?: HelpTopicId;
    onBack?: () => void;
}

export const HelpCenterView: React.FC<HelpCenterViewProps> = ({ initialTopic, onBack }) => {
    const { t } = useTranslation();
    const { isAuthenticated } = useSession();
    const { goBack, navigate } = useNavigation();
    const { isExpert } = useUiMode();

    const [searchQuery, setSearchQuery] = useState('');
    const [selectedGroup, setSelectedGroup] = useState<HelpGroup | 'all'>('all');
    const [selectedTopicId, setSelectedTopicId] = useState<HelpTopicId | null>(
        initialTopic || 'conceptParadigm'
    );
    const effectiveGroup: HelpGroup | 'all' =
        !isExpert && selectedGroup === 'expert' ? 'all' : selectedGroup;
    const visibleGroups = useMemo(
        () => (isExpert ? HELP_GROUPS : HELP_GROUPS.filter((g) => g.audience !== 'expert')),
        [isExpert]
    );

    // Update selected topic if prop changes
    useEffect(() => {
        if (initialTopic) {
            setSelectedTopicId(initialTopic);
        }
    }, [initialTopic]);

    // Handle global custom event humoco:navigate-help-center
    useEffect(() => {
        const handleCustomNav = (e: Event) => {
            const customEvent = e as CustomEvent<{ topic?: HelpTopicId }>;
            if (customEvent.detail?.topic) {
                setSelectedTopicId(customEvent.detail.topic);
            }
        };
        window.addEventListener('humoco:navigate-help-center', handleCustomNav);
        return () => window.removeEventListener('humoco:navigate-help-center', handleCustomNav);
    }, []);

    const handleBack = () => {
        if (onBack) {
            onBack();
        } else if (isAuthenticated) {
            goBack();
        } else {
            navigate({ view: 'needs_login' });
        }
    };

    // All topics as an array
    const allTopicsList = useMemo(() => Object.values(HELP_TOPICS), []);

    // Filter topics based on search query and group
    const filteredTopics = useMemo(() => {
        const query = searchQuery.trim().toLowerCase();

        return allTopicsList.filter((topic: HelpTopic) => {
            // Group filter
            if (effectiveGroup !== 'all' && topic.group !== effectiveGroup) {
                return false;
            }

            // Search filter
            if (!query) return true;

            const title = t(topic.titleKey).toLowerCase();
            const short = t(topic.shortKey).toLowerCase();
            const long = t(topic.longKey).toLowerCase();
            const deepDive = topic.deepDiveKey ? t(topic.deepDiveKey).toLowerCase() : '';

            return (
                title.includes(query) ||
                short.includes(query) ||
                long.includes(query) ||
                deepDive.includes(query)
            );
        });
    }, [allTopicsList, searchQuery, effectiveGroup, t]);

    // Selected topic definition
    const activeTopic = useMemo(() => {
        if (selectedTopicId && HELP_TOPICS[selectedTopicId]) {
            return HELP_TOPICS[selectedTopicId];
        }
        return filteredTopics[0] || null;
    }, [selectedTopicId, filteredTopics]);

    const activeGroupDef = activeTopic
        ? HELP_GROUPS.find((g) => g.id === activeTopic.group)
        : null;

    const renderTopicButton = (topic: HelpTopic) => {
        const isSelected = activeTopic?.id === topic.id;
        const grpDef = HELP_GROUPS.find((g) => g.id === topic.group);
        return (
            <div
                key={topic.id}
                onClick={() => setSelectedTopicId(topic.id)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        setSelectedTopicId(topic.id);
                    }
                }}
                className={`w-full text-left p-4 rounded-2xl border transition-all cursor-pointer flex items-center justify-between gap-3 ${
                    isSelected
                        ? 'bg-theme-primary/10 border-theme-primary/40 shadow-sm ring-1 ring-theme-primary/20'
                        : 'bg-white dark:bg-gray-800/60 border-theme-subtle hover:border-theme-primary/30 hover:bg-theme-primary/5'
                }`}
            >
                <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                        <span className="text-[10px] uppercase font-bold tracking-wider px-2 py-0.5 rounded-full bg-theme-primary/10 text-theme-primary">
                            {grpDef ? t(grpDef.labelKey) : topic.group}
                        </span>
                    </div>
                    <h3 className="font-extrabold text-sm text-theme-secondary tracking-tight truncate">
                        {t(topic.titleKey)}
                    </h3>
                    <p className="text-xs text-theme-light line-clamp-1 mt-0.5">
                        {t(topic.shortKey)}
                    </p>
                </div>
                <ChevronRight
                    size={18}
                    className={`shrink-0 transition-transform ${
                        isSelected ? 'text-theme-primary translate-x-0.5' : 'text-theme-placeholder'
                    }`}
                />
            </div>
        );
    };

    const hasActiveSearch = searchQuery.trim().length > 0;
    const shouldGroupExpertTopics = !isExpert && !hasActiveSearch;
    const primaryTopics = shouldGroupExpertTopics
        ? filteredTopics.filter((topic) => topic.audience !== 'expert')
        : filteredTopics;
    const expertTopics = shouldGroupExpertTopics
        ? filteredTopics.filter((topic) => topic.audience === 'expert')
        : [];

    const renderContent = () => (
        <div className="space-y-6">
            {/* Search and Category Filters */}
            <div className="space-y-4">
                {/* Search Bar */}
                <div className="relative">
                    <div className="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none text-theme-placeholder">
                        <Search size={18} />
                    </div>
                    <input
                        type="text"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder={t('helpCenter.searchPlaceholder')}
                        className="w-full pl-11 pr-10 py-3 bg-white dark:bg-gray-800/80 border border-theme-subtle rounded-2xl text-sm text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/30 transition-all shadow-sm"
                    />
                    {searchQuery && (
                        <button
                            type="button"
                            onClick={() => setSearchQuery('')}
                            className="absolute inset-y-0 right-0 pr-3.5 flex items-center text-theme-light hover:text-theme-secondary transition-colors cursor-pointer"
                            aria-label={t('helpCenter.clearSearch')}
                        >
                            <X size={16} />
                        </button>
                    )}
                </div>

                {/* Group Pills */}
                <div className="flex items-center gap-2 overflow-x-auto pb-1 scrollbar-none">
                    <button
                        type="button"
                        onClick={() => setSelectedGroup('all')}
                        className={`px-3.5 py-1.5 rounded-full text-xs font-bold transition-all cursor-pointer whitespace-nowrap ${
                            effectiveGroup === 'all'
                                ? 'bg-theme-primary text-white shadow-sm'
                                : 'bg-theme-subtle/20 text-theme-secondary hover:bg-theme-subtle/35'
                        }`}
                    >
                        {t('helpCenter.categoryAll')}
                    </button>
                    {visibleGroups.map((grp) => (
                        <button
                            key={grp.id}
                            type="button"
                            onClick={() => setSelectedGroup(grp.id)}
                            className={`px-3.5 py-1.5 rounded-full text-xs font-bold transition-all cursor-pointer whitespace-nowrap ${
                                effectiveGroup === grp.id
                                    ? 'bg-theme-primary text-white shadow-sm'
                                    : 'bg-theme-subtle/20 text-theme-secondary hover:bg-theme-subtle/35'
                            }`}
                        >
                            {t(grp.labelKey)}
                        </button>
                    ))}
                </div>
            </div>

            {/* Main Layout: List & Detail View */}
            {filteredTopics.length === 0 ? (
                <div className="p-12 text-center text-theme-light border border-dashed border-theme-subtle rounded-3xl bg-theme-subtle/5">
                    <Compass size={36} className="mx-auto mb-3 opacity-40 text-theme-primary" />
                    <p className="text-sm font-semibold">{t('helpCenter.noResults')}</p>
                </div>
            ) : (
                <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
                    {/* Topic List */}
                    <div
                        className={`lg:col-span-5 space-y-2.5 max-h-[600px] overflow-y-auto custom-scrollbar pr-1 ${
                            selectedTopicId && activeTopic ? 'hidden lg:block' : 'block'
                        }`}
                    >
                        {effectiveGroup === 'all' && !hasActiveSearch ? (
                            visibleGroups.map((grp) => {
                                const groupTopics = filteredTopics.filter(
                                    (topic) =>
                                        topic.group === grp.id &&
                                        (!shouldGroupExpertTopics || topic.audience !== 'expert')
                                );
                                if (groupTopics.length === 0) return null;
                                return (
                                    <div key={grp.id} className="space-y-2 pt-2 first:pt-0">
                                        <h3
                                            data-testid={`help-group-heading-${grp.id}`}
                                            className="text-[11px] font-bold uppercase tracking-wider text-theme-light px-1"
                                        >
                                            {t(grp.labelKey)}
                                        </h3>
                                        <div className="space-y-2.5">
                                            {groupTopics.map(renderTopicButton)}
                                        </div>
                                    </div>
                                );
                            })
                        ) : (
                            primaryTopics.map(renderTopicButton)
                        )}
                        {expertTopics.length > 0 && (
                            <Disclosure
                                title={
                                    <span className="text-xs font-bold text-theme-secondary">
                                        {t('helpCenter.forAdvancedUsers')} ({expertTopics.length})
                                    </span>
                                }
                                toggleTestId="help-center-expert-topics-toggle"
                                contentTestId="help-center-expert-topics"
                                className="border border-theme-subtle rounded-2xl overflow-hidden bg-slate-50/70 dark:bg-slate-800/40"
                                buttonClassName="w-full flex items-center justify-between gap-2 px-4 py-3 text-left hover:bg-slate-100/70 transition-colors cursor-pointer"
                                contentClassName="p-2.5 space-y-2.5 border-t border-theme-subtle bg-white/60 dark:bg-slate-900/40"
                            >
                                {expertTopics.map(renderTopicButton)}
                            </Disclosure>
                        )}
                    </div>

                    {/* Topic Detail View */}
                    <div
                        className={`lg:col-span-7 ${
                            selectedTopicId && activeTopic ? 'block' : 'hidden lg:block'
                        }`}
                    >
                        {activeTopic ? (
                            <div className="bg-white dark:bg-gray-800/80 border border-theme-subtle rounded-3xl p-6 md:p-8 shadow-sm space-y-6">
                                {/* Mobile back to list button */}
                                <div className="block lg:hidden pb-3 border-b border-theme-subtle/50">
                                    <button
                                        type="button"
                                        onClick={() => setSelectedTopicId(null)}
                                        className="text-xs font-bold text-theme-primary hover:underline flex items-center gap-1.5 cursor-pointer"
                                    >
                                        <ArrowLeft size={14} />
                                        {t('helpCenter.backToTopics')}
                                    </button>
                                </div>

                                {/* Detail Header */}
                                <div className="space-y-2">
                                    <span className="inline-block px-3 py-1 rounded-full text-xs font-bold tracking-wider uppercase bg-theme-primary/10 text-theme-primary">
                                        {activeGroupDef ? t(activeGroupDef.labelKey) : activeTopic.group}
                                    </span>
                                    <h2 className="text-xl md:text-2xl font-black text-theme-secondary tracking-tight">
                                        {t(activeTopic.titleKey)}
                                    </h2>
                                </div>

                                {/* Highlighted Short Summary Box */}
                                <div className="p-4 rounded-2xl bg-theme-primary/5 border border-theme-primary/15 text-sm font-semibold text-theme-primary leading-relaxed">
                                    {t(activeTopic.shortKey)}
                                </div>

                                {/* Formatted Long Explanation */}
                                <div className="border-t border-theme-subtle/40 pt-5">
                                    <HelpContentRenderer content={t(activeTopic.longKey)} />
                                </div>

                                {/* Deep dive collapsible (Mehr erfahren / Technischer Hintergrund) */}
                                {activeTopic.deepDiveKey && (
                                    <AdvancedDisclosure
                                        title={
                                            <span className="text-xs font-bold text-theme-primary">
                                                {t('helpCenter.deepDiveTitle', 'Mehr erfahren / Technischer Hintergrund')}
                                            </span>
                                        }
                                        contentTestId="help-center-deep-dive"
                                        className="bg-slate-50 border border-theme-subtle rounded-2xl overflow-hidden mt-4"
                                        buttonClassName="w-full flex items-center justify-between gap-2 p-4 text-left hover:bg-slate-100/70 transition-colors cursor-pointer"
                                        contentClassName="px-4 pb-4 pt-3 border-t border-theme-subtle/50 text-xs text-theme-secondary leading-relaxed"
                                    >
                                        <HelpContentRenderer content={t(activeTopic.deepDiveKey)} />
                                    </AdvancedDisclosure>
                                )}

                                {/* Clickable Related Topics Chips */}
                                {activeTopic.related && activeTopic.related.length > 0 && (
                                    <div className="pt-5 border-t border-theme-subtle/40 space-y-3">
                                        <h4 className="text-xs font-bold uppercase tracking-wider text-theme-light">
                                            {t('helpCenter.relatedTopics')}
                                        </h4>
                                        <div className="flex flex-wrap gap-2">
                                            {activeTopic.related.map((relId) => {
                                                const relTopic = HELP_TOPICS[relId];
                                                if (!relTopic) return null;
                                                return (
                                                    <button
                                                        key={relId}
                                                        type="button"
                                                        onClick={() => setSelectedTopicId(relId)}
                                                        className="px-3.5 py-1.5 rounded-xl text-xs font-semibold bg-theme-subtle/20 hover:bg-theme-primary/15 hover:text-theme-primary text-theme-secondary transition-all cursor-pointer border border-theme-subtle/40 shadow-xs"
                                                    >
                                                        {t(relTopic.titleKey)}
                                                    </button>
                                                );
                                            })}
                                        </div>
                                    </div>
                                )}
                            </div>
                        ) : (
                            <div className="p-12 text-center text-theme-light border border-dashed border-theme-subtle rounded-3xl bg-theme-subtle/5">
                                <p className="text-sm font-semibold">{t('helpCenter.selectTopicPrompt')}</p>
                            </div>
                        )}
                    </div>
                </div>
            )}
        </div>
    );

    if (isAuthenticated) {
        return (
            <PageLayout
                title={t('helpCenter.title')}
                description={t('helpCenter.subtitle')}
                onBack={handleBack}
                actions={
                    <div className="flex items-center gap-2">
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => navigate({ view: 'support' })}
                            className="gap-1.5"
                            data-testid="help-center-support-btn"
                        >
                            <Heart size={15} />
                            <span>{t('common.supportMenu')}</span>
                        </Button>
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => navigate({ view: 'bug_report' })}
                            className="gap-1.5"
                            data-testid="help-center-bug-report-btn"
                        >
                            <Terminal size={15} />
                            <span>{t('common.bugReportTitle')}</span>
                        </Button>
                    </div>
                }
            >
                <div className="max-w-5xl mx-auto py-4">
                    <Card className="p-6 md:p-8 shadow-premium rounded-[32px]">
                        {renderContent()}
                    </Card>
                </div>
            </PageLayout>
        );
    }

    return (
        <AuthLayout maxWidth="max-w-5xl">
            <div className="space-y-6">
                <div className="flex flex-wrap items-center justify-between gap-3 pb-4 border-b border-theme-subtle/50">
                    <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                            <BookOpen size={20} />
                        </div>
                        <div>
                            <h1 className="text-xl font-black text-theme-secondary tracking-tight uppercase">
                                {t('helpCenter.title')}
                            </h1>
                            <p className="text-[10px] font-medium text-theme-light uppercase tracking-wider">
                                {t('helpCenter.subtitle')}
                            </p>
                        </div>
                    </div>
                    <BackButton onClick={handleBack} label={t('helpCenter.backToLogin')} />
                </div>
                {renderContent()}
            </div>
        </AuthLayout>
    );
};
