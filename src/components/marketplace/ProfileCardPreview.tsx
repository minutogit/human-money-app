// src/components/marketplace/ProfileCardPreview.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, Globe, Mail, MapPin, Phone, User } from 'lucide-react';
import { CardFieldVisibility, FrontendUserProfile, Skill } from '../../types';
import { Advanced } from '../ui/Advanced';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { isSafeAvatarDataUri } from '../ui/UserAvatar';

export interface ProfileCardPreviewProps {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    skills: Skill[];
    previewPerspective: 'contact' | 'forwarded';
    onPerspectiveChange: (perspective: 'contact' | 'forwarded') => void;
}

/**
 * Live audience-filtered preview of the edited business card.
 */
export const ProfileCardPreview: React.FC<ProfileCardPreviewProps> = ({
    profile,
    visibility,
    skills,
    previewPerspective,
    onPerspectiveChange,
}) => {
    const { t } = useTranslation();

    const isVisibleInPreview = (key: keyof CardFieldVisibility): boolean => {
        const aud = visibility[key];
        if (aud === 'nobody') return false;
        if (previewPerspective === 'forwarded' && aud === 'contacts') return false;
        return true;
    };

    const previewName = [
        isVisibleInPreview('name') ? profile.firstName : null,
        isVisibleInPreview('name') ? profile.lastName : null,
    ]
        .filter(Boolean)
        .join(' ');

    const safeAvatarUri =
        isVisibleInPreview('picture') && isSafeAvatarDataUri(profile.pictureUrl)
            ? profile.pictureUrl.trim()
            : undefined;

    return (
        <div className="lg:col-span-5 sticky top-2 space-y-4">
            <div className="p-4 rounded-2xl bg-bg-app border border-theme-subtle space-y-3">
                <div className="flex items-center justify-between">
                    <h4 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-1.5">
                        <Eye size={14} className="text-theme-primary" />
                        <span>{t('marketplace.livePreviewTitle', 'So sehen andere deine Visitenkarte')}</span>
                    </h4>
                </div>

                <Advanced>
                    <SegmentedTabs<'contact' | 'forwarded'>
                        fullWidth
                        activeTab={previewPerspective}
                        onChange={onPerspectiveChange}
                        tabs={[
                            { id: 'contact', label: t('marketplace.previewContact', 'Direkter Kontakt') },
                            { id: 'forwarded', label: t('marketplace.previewForwarded', 'Über Empfehlung') },
                        ]}
                    />
                </Advanced>
            </div>

            <div className="p-5 rounded-2xl bg-white border border-theme-subtle shadow-sm space-y-4">
                <div className="flex items-start gap-3">
                    <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center font-bold text-base shrink-0 overflow-hidden">
                        {safeAvatarUri ? (
                            <img src={safeAvatarUri} alt="Avatar" className="w-full h-full object-cover" />
                        ) : (
                            previewName.charAt(0) || <User size={20} />
                        )}
                    </div>
                    <div className="min-w-0">
                        <h4 className="text-sm font-bold text-theme-secondary truncate">
                            {previewName || t('marketplace.previewNoName', 'Anonymer Kontakt')}
                        </h4>
                        {isVisibleInPreview('organization') && profile.organization && (
                            <p className="text-xs text-theme-light truncate">{profile.organization}</p>
                        )}
                        {isVisibleInPreview('community') && profile.community && (
                            <p className="text-[11px] text-theme-primary truncate font-medium">{profile.community}</p>
                        )}
                    </div>
                </div>

                <div className="space-y-1.5 text-xs text-theme-light pt-2 border-t border-theme-subtle/50">
                    {isVisibleInPreview('city') && profile.address?.city && (
                        <div className="flex items-center gap-2 truncate">
                            <MapPin size={13} className="shrink-0 text-theme-primary" />
                            <span>
                                {[profile.address?.zipCode, profile.address?.city].filter(Boolean).join(' ')}
                            </span>
                        </div>
                    )}
                    {isVisibleInPreview('street') && profile.address?.street && (
                        <div className="flex items-center gap-2 truncate">
                            <MapPin size={13} className="shrink-0 text-theme-light" />
                            <span>
                                {[profile.address?.street, profile.address?.houseNumber].filter(Boolean).join(' ')}
                            </span>
                        </div>
                    )}
                    {isVisibleInPreview('email') && profile.email && (
                        <div className="flex items-center gap-2 truncate">
                            <Mail size={13} className="shrink-0 text-theme-primary" />
                            <span>{profile.email}</span>
                        </div>
                    )}
                    {isVisibleInPreview('phone') && profile.phone && (
                        <div className="flex items-center gap-2 truncate">
                            <Phone size={13} className="shrink-0 text-theme-primary" />
                            <span>{profile.phone}</span>
                        </div>
                    )}
                    {isVisibleInPreview('url') && profile.url && (
                        <div className="flex items-center gap-2 truncate">
                            <Globe size={13} className="shrink-0 text-theme-primary" />
                            <span>{profile.url}</span>
                        </div>
                    )}
                </div>

                <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                    <h5 className="text-[11px] font-bold text-theme-secondary uppercase tracking-wider">
                        {t('marketplace.skillsHeader', 'Fähigkeiten')} ({skills.length})
                    </h5>
                    {skills.length === 0 ? (
                        <p className="text-xs text-theme-light italic">
                            {t('marketplace.noSkillsListed', 'Keine Fähigkeiten angegeben.')}
                        </p>
                    ) : (
                        <div className="space-y-1.5">
                            {skills.map((s, i) => (
                                <div key={i} className="p-2 rounded-xl bg-bg-app border border-theme-subtle/50 text-xs">
                                    <div className="font-bold text-theme-secondary flex items-center justify-between gap-1">
                                        <div className="flex items-center gap-1.5 min-w-0">
                                            <span className="truncate">{s.title || t('marketplace.untitled', 'Ohne Titel')}</span>
                                            {s.isRemote && (
                                                <span className="text-[9px] px-1 py-0.2 rounded font-semibold bg-sky-50 text-sky-700 border border-sky-200 shrink-0">
                                                    🌐 {t('marketplace.remoteBadgeShort', 'Online')}
                                                </span>
                                            )}
                                        </div>
                                        {s.pricing && (
                                            <span className="text-[10px] text-theme-primary font-semibold shrink-0">
                                                {s.pricing}
                                            </span>
                                        )}
                                    </div>
                                    {s.text && <p className="text-[11px] text-theme-light mt-0.5 line-clamp-2">{s.text}</p>}
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
};
