// src/components/marketplace/ProfileCardMediaSection.tsx
import React, { useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Upload, User } from 'lucide-react';
import { CardFieldVisibility, FrontendUserProfile } from '../../types';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { VisibilityCycleButton } from './ProfileCardVisibilitySection';

export interface ProfileCardMediaSectionProps {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    onProfileChange: React.Dispatch<React.SetStateAction<FrontendUserProfile>>;
    onCycleVisibility: (key: keyof CardFieldVisibility) => void;
    onAvatarFile: (file: File) => Promise<void>;
}

/**
 * Identity details and 96x96 WebP avatar upload section for the profile card editor.
 */
export const ProfileCardMediaSection: React.FC<ProfileCardMediaSectionProps> = ({
    profile,
    visibility,
    onProfileChange,
    onCycleVisibility,
    onAvatarFile,
}) => {
    const { t } = useTranslation();
    const avatarInputRef = useRef<HTMLInputElement | null>(null);

    const handleAvatarFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        try {
            await onAvatarFile(file);
        } finally {
            e.target.value = '';
        }
    };

    return (
        <div className="space-y-4">
            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                    <User size={14} className="text-theme-primary" />
                    <span>{t('marketplace.sectionAboutMe', 'Über mich')}</span>
                </h3>
            </div>

            <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                    <label className="text-[11px] font-bold text-theme-secondary">
                        {t('profile.name', 'Name')}
                    </label>
                    <VisibilityCycleButton field="name" visibility={visibility} onCycle={onCycleVisibility} />
                </div>
                <div className="grid grid-cols-2 gap-2">
                    <Input
                        placeholder={t('profile.firstName', 'Vorname')}
                        value={profile.firstName || ''}
                        maxLength={80}
                        onChange={(e) => onProfileChange({ ...profile, firstName: e.target.value })}
                    />
                    <Input
                        placeholder={t('profile.lastName', 'Nachname')}
                        value={profile.lastName || ''}
                        maxLength={80}
                        onChange={(e) => onProfileChange({ ...profile, lastName: e.target.value })}
                    />
                </div>
            </div>

            <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                    <label className="text-[11px] font-bold text-theme-secondary">
                        {t('profile.organization', 'Organisation / Betrieb')}
                    </label>
                    <VisibilityCycleButton field="organization" visibility={visibility} onCycle={onCycleVisibility} />
                </div>
                <Input
                    placeholder={t('profile.organizationPlaceholder', 'z. B. Naturwerkstatt')}
                    value={profile.organization || ''}
                    maxLength={100}
                    onChange={(e) => onProfileChange({ ...profile, organization: e.target.value })}
                />
            </div>

            <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                    <label className="text-[11px] font-bold text-theme-secondary">
                        {t('profile.community', 'Gemeinschaft')}
                    </label>
                    <VisibilityCycleButton field="community" visibility={visibility} onCycle={onCycleVisibility} />
                </div>
                <Input
                    placeholder={t('profile.communityPlaceholder', 'z. B. Tauschring Alztal')}
                    value={profile.community || ''}
                    maxLength={100}
                    onChange={(e) => onProfileChange({ ...profile, community: e.target.value })}
                />
            </div>

            <div className="space-y-2">
                <div className="flex items-center justify-between">
                    <label className="text-[11px] font-bold text-theme-secondary">
                        {t('marketplace.avatarLabel', 'Profilbild / Logo')}
                    </label>
                    <VisibilityCycleButton field="picture" visibility={visibility} onCycle={onCycleVisibility} />
                </div>
                <div className="flex items-center gap-4">
                    <div className="w-24 h-24 rounded-full overflow-hidden border border-theme-subtle bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0 shadow-xs">
                        {profile.pictureUrl ? (
                            <img
                                src={profile.pictureUrl}
                                alt="Avatar"
                                className="w-full h-full object-cover rounded-full"
                            />
                        ) : (
                            <User size={36} className="text-theme-primary/60" />
                        )}
                    </div>
                    <div className="space-y-1.5 flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                            <input
                                ref={avatarInputRef}
                                type="file"
                                accept="image/*"
                                data-testid="avatar-file-input"
                                onChange={handleAvatarFileChange}
                                className="hidden"
                            />
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() => avatarInputRef.current?.click()}
                                className="flex items-center gap-1.5"
                            >
                                <Upload size={13} />
                                <span>{t('marketplace.avatarUpload', 'Bild auswählen')}</span>
                            </Button>
                            {profile.pictureUrl && (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="secondary"
                                    data-testid="avatar-remove-btn"
                                    onClick={() => onProfileChange((prev) => ({ ...prev, pictureUrl: undefined }))}
                                >
                                    {t('marketplace.avatarRemove', 'Entfernen')}
                                </Button>
                            )}
                        </div>
                        <p className="text-[10px] text-theme-light leading-snug">
                            {t('marketplace.avatarHint', 'Wird quadratisch auf 96×96 Pixel zugeschnitten (WebP, max. 4 KB).')}
                        </p>
                    </div>
                </div>
            </div>

            <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                    <label className="text-[11px] font-bold text-theme-secondary">
                        {t('profile.gender', 'Geschlecht')}
                    </label>
                    <VisibilityCycleButton field="gender" visibility={visibility} onCycle={onCycleVisibility} />
                </div>
                <Input
                    placeholder={t('profile.genderPlaceholder', 'z. B. weiblich / männlich')}
                    value={profile.gender || ''}
                    maxLength={20}
                    onChange={(e) => onProfileChange({ ...profile, gender: e.target.value })}
                />
            </div>
        </div>
    );
};
