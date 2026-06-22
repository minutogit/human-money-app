// src/components/ConceptView.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigation } from '../context/NavigationContext';
import { useSession } from '../context/SessionContext';
import { AuthLayout } from './AuthLayout';
import { PageLayout } from './ui/PageLayout';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { 
    BookOpen, 
    ChevronLeft, 
    ChevronRight, 
    Globe, 
    User, 
    Users, 
    Lock, 
    AlertTriangle, 
    ChevronDown, 
    ChevronUp, 
    Sparkles, 
    CheckCircle2 
} from 'lucide-react';

export const ConceptView: React.FC = () => {
    const { t } = useTranslation();
    const { isSessionActive } = useSession();
    const { navigate, goBack } = useNavigation();
    const [activeTab, setActiveTab] = useState<'short' | 'detailed'>('short');
    const [currentSlide, setCurrentSlide] = useState(0);
    const [expandedSection, setExpandedSection] = useState<number | null>(0);

    const handleBack = () => {
        if (isSessionActive) {
            goBack();
        } else {
            navigate({ view: 'needs_login' });
        }
    };

    const shortSlides = [
        {
            title: t('dashboard.conceptScreen1Title'),
            text: t('dashboard.conceptScreen1Text'),
            icon: <Globe className="w-12 h-12 text-theme-primary" />,
            bgColor: 'bg-blue-50/50 dark:bg-blue-950/20'
        },
        {
            title: t('dashboard.conceptScreen2Title'),
            text: t('dashboard.conceptScreen2Text'),
            icon: <User className="w-12 h-12 text-theme-primary" />,
            bgColor: 'bg-indigo-50/50 dark:bg-indigo-950/20'
        },
        {
            title: t('dashboard.conceptScreen3Title'),
            text: t('dashboard.conceptScreen3Text'),
            icon: <Users className="w-12 h-12 text-theme-primary" />,
            bgColor: 'bg-emerald-50/50 dark:bg-emerald-950/20'
        },
        {
            title: t('dashboard.conceptScreen4Title'),
            text: t('dashboard.conceptScreen4Text'),
            icon: <Lock className="w-12 h-12 text-theme-primary" />,
            bgColor: 'bg-purple-50/50 dark:bg-purple-950/20'
        },
        {
            title: t('dashboard.conceptScreen5Title'),
            text: t('dashboard.conceptScreen5Text'),
            icon: <AlertTriangle className="w-12 h-12 text-theme-primary" />,
            bgColor: 'bg-amber-50/50 dark:bg-amber-950/20'
        }
    ];

    const detailedSections = [
        {
            title: t('dashboard.conceptDetailedTitle'),
            text: t('dashboard.conceptDetailedText'),
            icon: <Sparkles className="w-5 h-5 text-theme-primary" />
        },
        {
            title: t('dashboard.conceptIdentityTitle'),
            text: t('dashboard.conceptIdentityText'),
            icon: <User className="w-5 h-5 text-theme-primary" />
        },
        {
            title: t('dashboard.conceptTrustTitle'),
            text: t('dashboard.conceptTrustText'),
            icon: <Users className="w-5 h-5 text-theme-primary" />
        },
        {
            title: t('dashboard.conceptPrivacyTitle'),
            text: t('dashboard.conceptPrivacyText'),
            icon: <Lock className="w-5 h-5 text-theme-primary" />
        },
        {
            title: t('dashboard.conceptSecurityTitle'),
            text: t('dashboard.conceptSecurityText'),
            icon: <AlertTriangle className="w-5 h-5 text-theme-primary" />
        },
        {
            title: t('dashboard.conceptFutureTitle'),
            text: t('dashboard.conceptFutureText'),
            icon: <CheckCircle2 className="w-5 h-5 text-theme-primary" />
        }
    ];

    const handleNextSlide = () => {
        if (currentSlide < shortSlides.length - 1) {
            setCurrentSlide(currentSlide + 1);
        } else {
            handleBack();
        }
    };

    const handlePrevSlide = () => {
        if (currentSlide > 0) {
            setCurrentSlide(currentSlide - 1);
        }
    };

    const toggleSection = (index: number) => {
        setExpandedSection(expandedSection === index ? null : index);
    };

    const renderTabs = () => (
        <div className="flex border-b border-theme-subtle/50 mb-6 bg-slate-50 dark:bg-slate-900/30 p-1.5 rounded-2xl">
            <button
                type="button"
                onClick={() => setActiveTab('short')}
                className={`flex-1 py-3 text-center text-sm font-bold rounded-xl transition-all cursor-pointer ${
                    activeTab === 'short'
                        ? 'bg-white dark:bg-gray-800 text-theme-primary shadow-sm'
                        : 'text-theme-light hover:text-theme-secondary'
                }`}
            >
                {t('dashboard.conceptTabShort')}
            </button>
            <button
                type="button"
                onClick={() => setActiveTab('detailed')}
                className={`flex-1 py-3 text-center text-sm font-bold rounded-xl transition-all cursor-pointer ${
                    activeTab === 'detailed'
                        ? 'bg-white dark:bg-gray-800 text-theme-primary shadow-sm'
                        : 'text-theme-light hover:text-theme-secondary'
                }`}
            >
                {t('dashboard.conceptTabDetailed')}
            </button>
        </div>
    );

    const renderShortVersion = () => {
        const slide = shortSlides[currentSlide];
        return (
            <div className="space-y-6 animate-in fade-in duration-300">
                <div className={`p-8 rounded-[32px] border border-theme-subtle/50 ${slide.bgColor} flex flex-col items-center text-center space-y-6 min-h-[340px] justify-center transition-all duration-300`}>
                    <div className="p-4 bg-white dark:bg-gray-800 rounded-[24px] shadow-premium">
                        {slide.icon}
                    </div>
                    <div className="space-y-3 max-w-xl">
                        <h3 className="text-xl font-black text-theme-secondary tracking-tight">
                            {slide.title}
                        </h3>
                        <p className="text-sm text-theme-light leading-relaxed">
                            {slide.text}
                        </p>
                    </div>
                </div>

                {/* Slider Controls */}
                <div className="flex items-center justify-between pt-2">
                    <Button
                        type="button"
                        variant="secondary"
                        onClick={handlePrevSlide}
                        disabled={currentSlide === 0}
                        className="rounded-2xl px-5 py-2.5 text-xs flex items-center gap-1 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
                    >
                        <ChevronLeft size={16} />
                        {t('dashboard.conceptPrev')}
                    </Button>

                    {/* Dot indicators */}
                    <div className="flex gap-2">
                        {shortSlides.map((_, index) => (
                            <button
                                key={index}
                                type="button"
                                onClick={() => setCurrentSlide(index)}
                                className={`w-2.5 h-2.5 rounded-full transition-all cursor-pointer ${
                                    currentSlide === index
                                        ? 'bg-theme-primary w-6'
                                        : 'bg-theme-placeholder/50 hover:bg-theme-placeholder'
                                }`}
                                aria-label={`Go to slide ${index + 1}`}
                            />
                        ))}
                    </div>

                    <Button
                        type="button"
                        onClick={handleNextSlide}
                        className="rounded-2xl px-6 py-2.5 text-xs flex items-center gap-1 cursor-pointer font-bold"
                    >
                        {currentSlide === shortSlides.length - 1 ? t('dashboard.conceptFinish') : t('dashboard.conceptNext')}
                        <ChevronRight size={16} />
                    </Button>
                </div>
            </div>
        );
    };

    const renderDetailedVersion = () => {
        return (
            <div className="space-y-4 max-h-[500px] overflow-y-auto pr-1 animate-in fade-in duration-300">
                {detailedSections.map((section, index) => {
                    const isExpanded = expandedSection === index;
                    return (
                        <div 
                            key={index} 
                            className={`border border-theme-subtle/50 rounded-2xl overflow-hidden transition-all duration-300 bg-white dark:bg-gray-800/50 hover:border-theme-primary/30 ${
                                isExpanded ? 'shadow-premium ring-1 ring-theme-primary/10' : 'shadow-sm'
                            }`}
                        >
                            <button
                                type="button"
                                onClick={() => toggleSection(index)}
                                className="w-full flex items-center justify-between p-5 text-left font-bold text-sm text-theme-secondary hover:text-theme-primary transition-colors cursor-pointer focus:outline-none"
                            >
                                <div className="flex items-center gap-3">
                                    <div className="p-2 bg-theme-primary/10 rounded-xl">
                                        {section.icon}
                                    </div>
                                    <span className="font-extrabold tracking-tight">{section.title}</span>
                                </div>
                                {isExpanded ? (
                                    <ChevronUp size={18} className="text-theme-light" />
                                ) : (
                                    <ChevronDown size={18} className="text-theme-light" />
                                )}
                            </button>
                            
                            <div 
                                className={`transition-all duration-300 ease-in-out overflow-hidden ${
                                    isExpanded ? 'max-h-[800px] border-t border-theme-subtle/40' : 'max-h-0'
                                }`}
                            >
                                <div className="p-5 text-sm text-theme-light leading-relaxed whitespace-pre-line bg-slate-50/30 dark:bg-slate-900/10">
                                    {section.text}
                                </div>
                            </div>
                        </div>
                    );
                })}
            </div>
        );
    };

    const viewContent = (
        <div className="space-y-4">
            {renderTabs()}
            {activeTab === 'short' ? renderShortVersion() : renderDetailedVersion()}
        </div>
    );

    if (isSessionActive) {
        return (
            <PageLayout
                title={t('dashboard.conceptTitle')}
                description={t('dashboard.conceptSubtitle')}
                onBack={handleBack}
            >
                <div className="max-w-3xl mx-auto py-4">
                    <Card className="p-6 md:p-8 shadow-premium rounded-[32px]">
                        {viewContent}
                    </Card>
                </div>
            </PageLayout>
        );
    } else {
        return (
            <AuthLayout maxWidth="max-w-3xl">
                <div className="space-y-6">
                    <div className="flex items-center justify-between pb-4 border-b border-theme-subtle/50">
                        <div className="flex items-center gap-3">
                            <div className="w-10 h-10 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                                <BookOpen size={20} />
                            </div>
                            <div>
                                <h1 className="text-xl font-black text-theme-secondary tracking-tight uppercase">
                                    {t('dashboard.conceptTitle')}
                                </h1>
                                <p className="text-[10px] font-medium text-theme-light uppercase tracking-wider">
                                    {t('dashboard.conceptSubtitle')}
                                </p>
                            </div>
                        </div>
                        <Button 
                            type="button" 
                            variant="secondary" 
                            onClick={handleBack}
                            className="rounded-2xl px-4 py-2 text-xs flex items-center gap-1 cursor-pointer"
                        >
                            <ChevronLeft size={16} />
                            {t('dashboard.conceptBackToLogin')}
                        </Button>
                    </div>
                    {viewContent}
                </div>
            </AuthLayout>
        );
    }
};
