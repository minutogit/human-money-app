// src/components/marketplace/ProfileCardEditor.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { User } from 'lucide-react';
import {
    FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest,
} from '../../types';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import {
    CARD_VISIBILITY_PRESETS,
    CardPresetId,
    detectCardPreset,
    useProfileCardEditor,
} from './useProfileCardEditor';
import { ProfileCardVisibilitySection } from './ProfileCardVisibilitySection';
import { ProfileCardMediaSection } from './ProfileCardMediaSection';
import { ProfileCardContactSection } from './ProfileCardContactSection';
import { ProfileCardBioSkillsSection } from './ProfileCardBioSkillsSection';
import { ProfileCardPreview } from './ProfileCardPreview';
import { extractUserIdPrefix } from '../../utils/userIdHelper';

export type { CardPresetId };
export { CARD_VISIBILITY_PRESETS, detectCardPreset };

export interface ProfileCardEditorProps {
    isOpen: boolean;
    onClose: () => void;
    myProfile: FrontendMyMarketProfile;
    onSave: (req: FrontendSaveMyMarketProfileRequest) => Promise<void>;
}

export const ProfileCardEditor: React.FC<ProfileCardEditorProps> = ({
    isOpen,
    onClose,
    myProfile,
    onSave,
}) => {
    const { t } = useTranslation();
    const {
        profile,
        setProfile,
        visibility,
        skills,
        allowInquiries,
        setAllowInquiries,
        published,
        setPublished,
        previewPerspective,
        setPreviewPerspective,
        isSaving,
        error,
        coordPrecision,
        isLocating,
        geoError,
        applyPreset,
        cycleVisibility,
        handlePrecisionChange,
        handleFetchGps,
        handleGeocodeAddress,
        handleAvatarFile,
        handleAddSkill,
        handleRemoveSkill,
        handleMoveSkill,
        handleSkillChange,
        handleSubmit,
    } = useProfileCardEditor({ isOpen, myProfile, onSave, onClose });

    const activePrefix = extractUserIdPrefix(myProfile.profile.id || '');

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="2xl"
            className="max-w-4xl"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <User size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">
                            {t('marketplace.profileCardEditorTitle', 'Mein Profil & Visitenkarte')}
                        </h2>
                        <p className="text-xs text-theme-light">
                            {t('marketplace.profileCardEditorSubtitle', 'Gemeinsamer Editor für dein Profil und deine Visitenkarte')}
                        </p>
                    </div>
                </div>
            }
        >
            <form onSubmit={handleSubmit} className="space-y-8 pt-2">
                {error && (
                    <div className="text-xs p-3 rounded-xl bg-rose-50 text-rose-800 border border-rose-200">
                        {error}
                    </div>
                )}

                <ProfileCardVisibilitySection
                    published={published}
                    onPublishedChange={setPublished}
                    visibility={visibility}
                    onApplyPreset={applyPreset}
                    activePrefix={activePrefix}
                />

                <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
                    <div className="lg:col-span-7 space-y-6">
                        <ProfileCardMediaSection
                            profile={profile}
                            visibility={visibility}
                            onProfileChange={setProfile}
                            onCycleVisibility={cycleVisibility}
                            onAvatarFile={handleAvatarFile}
                        />

                        <ProfileCardContactSection
                            profile={profile}
                            visibility={visibility}
                            coordPrecision={coordPrecision}
                            isLocating={isLocating}
                            geoError={geoError}
                            allowInquiries={allowInquiries}
                            onProfileChange={setProfile}
                            onCycleVisibility={cycleVisibility}
                            onPrecisionChange={handlePrecisionChange}
                            onFetchGps={handleFetchGps}
                            onGeocodeAddress={handleGeocodeAddress}
                            onAllowInquiriesChange={setAllowInquiries}
                        />

                        <ProfileCardBioSkillsSection
                            skills={skills}
                            onAddSkill={handleAddSkill}
                            onRemoveSkill={handleRemoveSkill}
                            onMoveSkill={handleMoveSkill}
                            onSkillChange={handleSkillChange}
                        />
                    </div>

                    <ProfileCardPreview
                        profile={profile}
                        visibility={visibility}
                        skills={skills}
                        previewPerspective={previewPerspective}
                        onPerspectiveChange={setPreviewPerspective}
                    />
                </div>

                <div className="flex items-center justify-end gap-3 pt-4 border-t border-theme-subtle/60">
                    <Button variant="secondary" type="button" onClick={onClose} disabled={isSaving}>
                        {t('common.cancel', 'Abbrechen')}
                    </Button>
                    <Button variant="primary" type="submit" data-testid="save-profile-card-button" disabled={isSaving}>
                        {isSaving ? t('common.saving', 'Speichern...') : t('common.save', 'Speichern')}
                    </Button>
                </div>
            </form>
        </ModalShell>
    );
};
