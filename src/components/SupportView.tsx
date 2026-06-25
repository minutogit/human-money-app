// src/components/SupportView.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { PageLayout } from './ui/PageLayout';
import { Card } from './ui/Card';
import { Button } from './ui/Button';
import { Heart, Globe, MessageSquare, Award } from 'lucide-react';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export const SupportView: React.FC = () => {
    const { t } = useTranslation();

    const handleOpenWebsite = async () => {
        try {
            await openUrl('https://menschlich-miteinander.org');
        } catch (err) {
            logger.error(`Failed to open URL: ${stringifyError(err)}`);
        }
    };

    return (
        <PageLayout
            title={t('common.supportTitle')}
            description={t('common.supportMenu')}
        >
            <div className="max-w-3xl mx-auto flex flex-col items-center justify-center py-8">
                {/* Large animated heart icon */}
                <div className="relative mb-8 group">
                    <div className="absolute inset-0 bg-theme-primary/10 rounded-full blur-2xl group-hover:bg-theme-primary/20 transition-all duration-500 scale-125"></div>
                    <div className="relative w-28 h-28 bg-white border border-theme-subtle rounded-[36px] flex items-center justify-center shadow-premium group-hover:shadow-premium-hover transition-all duration-300 group-hover:-translate-y-1 active:scale-[0.98]">
                        <Heart className="w-14 h-14 text-theme-primary fill-theme-primary/10 group-hover:fill-theme-primary/20 transition-all duration-300 group-hover:scale-110" />
                    </div>
                </div>

                {/* Main Card */}
                <Card className="w-full p-8 md:p-10 text-center space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
                    <div className="space-y-4">
                        <h2 className="text-3xl font-black text-theme-secondary tracking-tight">
                            {t('common.supportTitle')}
                        </h2>
                        <div className="w-12 h-1 bg-theme-primary/30 mx-auto rounded-full"></div>
                        <p className="text-base text-theme-light leading-relaxed max-w-xl mx-auto">
                            {t('common.supportDescription')}
                        </p>
                    </div>

                    <div className="pt-4">
                        <Button
                            onClick={handleOpenWebsite}
                            className="px-8 py-5 rounded-3xl shadow-premium text-lg gap-2 inline-flex items-center group transition-all"
                        >
                            <Globe size={20} className="group-hover:rotate-12 transition-transform duration-300" />
                            {t('common.supportCtaButton')}
                        </Button>
                    </div>
                </Card>

                {/* Additional Info / Vision Grid */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-6 w-full mt-10">
                    <div className="bg-white border border-theme-subtle/50 p-6 rounded-[24px] shadow-premium text-center space-y-3">
                        <div className="w-10 h-10 bg-rose-50 text-theme-primary rounded-xl flex items-center justify-center mx-auto">
                            <Heart size={20} />
                        </div>
                        <h3 className="font-bold text-sm text-theme-secondary">{t('common.supportDonateTitle')}</h3>
                        <p className="text-xs text-theme-light">{t('common.supportDonateDesc')}</p>
                    </div>
                    <div className="bg-white border border-theme-subtle/50 p-6 rounded-[24px] shadow-premium text-center space-y-3">
                        <div className="w-10 h-10 bg-emerald-50 text-theme-success rounded-xl flex items-center justify-center mx-auto">
                            <Award size={20} />
                        </div>
                        <h3 className="font-bold text-sm text-theme-secondary">{t('common.supportMemberTitle')}</h3>
                        <p className="text-xs text-theme-light">{t('common.supportMemberDesc')}</p>
                    </div>
                    <div className="bg-white border border-theme-subtle/50 p-6 rounded-[24px] shadow-premium text-center space-y-3">
                        <div className="w-10 h-10 bg-amber-50 text-theme-accent rounded-xl flex items-center justify-center mx-auto">
                            <MessageSquare size={20} />
                        </div>
                        <h3 className="font-bold text-sm text-theme-secondary">{t('common.supportContributeTitle')}</h3>
                        <p className="text-xs text-theme-light">{t('common.supportContributeDesc')}</p>
                    </div>
                </div>
            </div>
        </PageLayout>
    );
};
