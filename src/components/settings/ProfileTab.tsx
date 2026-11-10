// src/components/settings/ProfileTab.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Fingerprint, Globe, User, Copy, CheckCircle2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { LanguageSelector } from '../ui/LanguageSelector';
import { InfoHint } from '../ui/InfoHint';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { copyToClipboard } from '../../utils/clipboard';

export interface ProfileTabProps {
    userId: string;
}

export const ProfileTab: React.FC<ProfileTabProps> = ({ userId }) => {
    const { t } = useTranslation();
    const nav = useOptionalNavigation();
    const [copiedUserId, setCopiedUserId] = useState(false);

    const handleCopyUserId = async () => {
        if (!userId) return;
        const ok = await copyToClipboard(userId);
        if (ok) {
            setCopiedUserId(true);
            setTimeout(() => setCopiedUserId(false), 2000);
        }
    };

    return (
        <div id="settings-profile-section" className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Benutzer-ID */}
            <Card header={
                <div className="flex items-center justify-between w-full">
                    <div className="flex items-center gap-2">
                        <Fingerprint size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.userIdTitle')}</span>
                    </div>
                </div>
            }>
                <div className="space-y-3">
                    <InfoHint topic="benutzerId" />
                    <div className="flex items-center gap-2">
                        <Input
                            readOnly
                            value={userId}
                            className="font-mono text-xs bg-bg-app border-theme-subtle"
                        />
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={handleCopyUserId}
                            className="shrink-0 flex items-center gap-1.5"
                            aria-label={t('common.copy')}
                        >
                            {copiedUserId ? (
                                <>
                                    <CheckCircle2 size={14} className="text-theme-success" />
                                    <span>{t('common.copied')}</span>
                                </>
                            ) : (
                                <>
                                    <Copy size={14} />
                                    <span>{t('common.copy')}</span>
                                </>
                            )}
                        </Button>
                    </div>
                </div>
            </Card>

            {/* Sprache */}
            <Card header={
                <div className="flex items-center gap-2">
                    <Globe size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.languageTitle')}</span>
                </div>
            }>
                <LanguageSelector />
            </Card>

            {/* Verweis auf Marktplatz */}
            <Card header={
                <div className="flex items-center gap-2">
                    <User size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('profile.businessCardSectionTitle')}</span>
                </div>
            }>
                <div className="space-y-4">
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('profile.marketplaceRedirectDesc')}
                    </p>
                    <Button
                        type="button"
                        variant="primary"
                        size="sm"
                        onClick={() => nav?.navigate({ view: 'marketplace', initialTab: 'mine', initialOpenEditor: true })}
                        className="flex items-center gap-2"
                    >
                        <span>{t('profile.openInMarketplaceBtn')}</span>
                    </Button>
                </div>
            </Card>
        </div>
    );
};

export default ProfileTab;
