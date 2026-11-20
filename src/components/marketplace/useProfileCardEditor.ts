// src/components/marketplace/useProfileCardEditor.ts
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    CardFieldVisibility,
    FieldAudience,
    FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest,
    FrontendUserProfile,
    Skill,
} from '../../types';
import {
    CoordinatePrecision,
    geocodeAddress,
    getCurrentLocation,
    roundCoordinates,
} from '../../utils/geoUtils';
import { compressAvatarToWebpDataUri } from '../../utils/imageCompression';

export type CardPresetId = 'private' | 'local' | 'business';

export const CARD_VISIBILITY_PRESETS: Record<CardPresetId, CardFieldVisibility> = {
    private: {
        name: 'everyone',
        city: 'everyone',
        coordinates: 'everyone',
        organization: 'contacts',
        community: 'contacts',
        email: 'contacts',
        phone: 'contacts',
        picture: 'contacts',
        url: 'contacts',
        street: 'nobody',
        gender: 'nobody',
    },
    local: {
        name: 'everyone',
        organization: 'everyone',
        community: 'everyone',
        city: 'everyone',
        picture: 'everyone',
        url: 'everyone',
        coordinates: 'everyone',
        street: 'contacts',
        email: 'contacts',
        phone: 'contacts',
        gender: 'nobody',
    },
    business: {
        name: 'everyone',
        organization: 'everyone',
        community: 'everyone',
        city: 'everyone',
        picture: 'everyone',
        url: 'everyone',
        email: 'everyone',
        phone: 'everyone',
        coordinates: 'everyone',
        street: 'contacts',
        gender: 'nobody',
    },
};

const VISIBILITY_KEYS: (keyof CardFieldVisibility)[] = [
    'name',
    'organization',
    'community',
    'city',
    'street',
    'email',
    'phone',
    'coordinates',
    'gender',
    'url',
    'picture',
];

export function detectCardPreset(vis: CardFieldVisibility): CardPresetId | 'custom' {
    for (const id of ['local', 'private', 'business'] as const) {
        const preset = CARD_VISIBILITY_PRESETS[id];
        if (VISIBILITY_KEYS.every((k) => vis[k] === preset[k])) {
            return id;
        }
    }
    return 'custom';
}

function clampStreetVisibility(vis: CardFieldVisibility): CardFieldVisibility {
    return {
        ...vis,
        street: vis.street === 'everyone' ? 'contacts' : vis.street,
    };
}

function roundProfileCoordinatesIfPublic(
    profile: FrontendUserProfile,
    coordinatesAudience: FieldAudience,
): FrontendUserProfile {
    if (coordinatesAudience !== 'everyone' || !profile.coordinates?.trim()) {
        return profile;
    }
    const rounded = roundCoordinates(profile.coordinates, 2);
    return rounded ? { ...profile, coordinates: rounded } : profile;
}

interface UseProfileCardEditorParams {
    isOpen: boolean;
    myProfile: FrontendMyMarketProfile;
    onSave: (req: FrontendSaveMyMarketProfileRequest) => Promise<void>;
    onClose: () => void;
}

export function useProfileCardEditor({
    isOpen,
    myProfile,
    onSave,
    onClose,
}: UseProfileCardEditorParams) {
    const { t } = useTranslation();

    const [profile, setProfile] = useState<FrontendUserProfile>(myProfile.profile);
    const [visibility, setVisibility] = useState<CardFieldVisibility>(() =>
        clampStreetVisibility(myProfile.visibility),
    );
    const [skills, setSkills] = useState<Skill[]>(myProfile.skills || []);
    const [allowInquiries, setAllowInquiries] = useState<boolean>(myProfile.allowInquiries);
    const [published, setPublished] = useState<boolean>(myProfile.published);
    const [previewPerspective, setPreviewPerspective] = useState<'contact' | 'forwarded'>('contact');
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [coordPrecision, setCoordPrecision] = useState<CoordinatePrecision>(2);
    const [isLocating, setIsLocating] = useState(false);
    const [geoError, setGeoError] = useState<string | null>(null);
    const wasOpenRef = useRef(false);
    const loadedDidRef = useRef<string | null>(null);

    // Synchronize initial state only on open transition or DID switch so background syncs do not wipe edits.
    useEffect(() => {
        if (!isOpen) {
            wasOpenRef.current = false;
            return;
        }
        const currentDid = myProfile.profile.id ?? null;
        if (wasOpenRef.current && loadedDidRef.current === currentDid) {
            return;
        }
        wasOpenRef.current = true;
        loadedDidRef.current = currentDid;
        const clampedVis = clampStreetVisibility(myProfile.visibility);
        const initialProfile = roundProfileCoordinatesIfPublic(
            { ...myProfile.profile },
            clampedVis.coordinates,
        );
        setProfile(initialProfile);
        setVisibility(clampedVis);
        setSkills(myProfile.skills || []);
        setAllowInquiries(myProfile.allowInquiries);
        setPublished(myProfile.published);
        setPreviewPerspective('contact');
        if (clampedVis.coordinates !== 'contacts') {
            setCoordPrecision(2);
        }
        setGeoError(null);
        setError(null);
    }, [myProfile, isOpen]);

    const applyPreset = (presetId: CardPresetId) => {
        const nextVis = { ...CARD_VISIBILITY_PRESETS[presetId] };
        setVisibility(nextVis);
        if (nextVis.coordinates === 'everyone') {
            setCoordPrecision(2);
            setProfile((prev) => roundProfileCoordinatesIfPublic(prev, 'everyone'));
        }
    };

    const updateVisibility = (key: keyof CardFieldVisibility, val: FieldAudience) => {
        const sanitizedVal: FieldAudience = key === 'street' && val === 'everyone' ? 'contacts' : val;
        setVisibility((prev) => ({ ...prev, [key]: sanitizedVal }));
        if (key === 'coordinates' && sanitizedVal !== 'contacts') {
            setCoordPrecision(2);
            if (sanitizedVal === 'everyone') {
                setProfile((prev) => roundProfileCoordinatesIfPublic(prev, 'everyone'));
            }
        }
    };

    // Cycle visibility: everyone -> contacts -> nobody -> everyone (street only cycles contacts <-> nobody).
    const cycleVisibility = (key: keyof CardFieldVisibility) => {
        const order: FieldAudience[] =
            key === 'street' ? ['contacts', 'nobody'] : ['everyone', 'contacts', 'nobody'];
        const idx = order.indexOf(visibility[key]);
        const nextIdx = idx === -1 ? 0 : (idx + 1) % order.length;
        updateVisibility(key, order[nextIdx]);
    };

    const handlePrecisionChange = (newPrecision: CoordinatePrecision) => {
        const effectivePrecision: CoordinatePrecision =
            visibility.coordinates === 'contacts' ? newPrecision : 2;
        setCoordPrecision(effectivePrecision);
        if (profile.coordinates?.trim()) {
            const rounded = roundCoordinates(profile.coordinates, effectivePrecision);
            if (rounded) {
                setProfile((prev) => ({ ...prev, coordinates: rounded }));
            }
        }
    };

    const resolveCoordinatesWith = async (fetcher: () => Promise<string>) => {
        setIsLocating(true);
        setGeoError(null);
        const targetVisibility: FieldAudience =
            visibility.coordinates === 'contacts' ? 'contacts' : 'everyone';
        const effectivePrecision: CoordinatePrecision =
            targetVisibility === 'contacts' ? coordPrecision : 2;
        try {
            const rawCoords = await fetcher();
            const rounded = roundCoordinates(rawCoords, effectivePrecision) || rawCoords;
            setProfile((prev) => ({ ...prev, coordinates: rounded }));
            setVisibility((prev) => ({ ...prev, coordinates: targetVisibility }));
            if (targetVisibility !== 'contacts') {
                setCoordPrecision(2);
            }
        } catch (err) {
            setGeoError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLocating(false);
        }
    };

    const handleFetchGps = () => resolveCoordinatesWith(() => getCurrentLocation(6));

    const handleGeocodeAddress = () =>
        resolveCoordinatesWith(() => geocodeAddress(profile.address || {}, 6));

    const handleAvatarFile = async (file: File) => {
        try {
            const webpDataUri = await compressAvatarToWebpDataUri(file);
            setProfile((prev) => ({ ...prev, pictureUrl: webpDataUri }));
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        }
    };

    const handleAddSkill = () => {
        if (skills.length >= 8) return;
        setSkills([...skills, { title: '', text: '', pricing: '', isRemote: false }]);
    };

    const handleRemoveSkill = (index: number) => {
        setSkills(skills.filter((_, i) => i !== index));
    };

    const handleMoveSkill = (index: number, direction: 'up' | 'down') => {
        const target = direction === 'up' ? index - 1 : index + 1;
        if (target < 0 || target >= skills.length) return;
        const copy = [...skills];
        const [removed] = copy.splice(index, 1);
        copy.splice(target, 0, removed);
        setSkills(copy);
    };

    const handleSkillChange = <K extends keyof Skill>(index: number, field: K, value: Skill[K]) => {
        setSkills(skills.map((s, i) => (i === index ? { ...s, [field]: value } : s)));
    };

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setError(null);

        for (const s of skills) {
            if (!s.title.trim()) {
                setError(t('marketplace.errorSkillTitleRequired', 'Jede Fähigkeit benötigt einen Titel'));
                return;
            }
            if (s.title.trim().length > 60) {
                setError(t('marketplace.errorSkillTitleTooLong', 'Titel einer Fähigkeit darf maximal 60 Zeichen lang sein'));
                return;
            }
            if (s.text && s.text.trim().length > 160) {
                setError(t('marketplace.errorSkillTextTooLong', 'Beschreibung einer Fähigkeit darf maximal 160 Zeichen lang sein'));
                return;
            }
            if (s.pricing && s.pricing.trim().length > 40) {
                setError(t('marketplace.errorPricingTooLong', 'Maximal {{max}} Zeichen erlaubt', { max: 40 }));
                return;
            }
        }

        setIsSaving(true);
        try {
            let finalProfile: FrontendUserProfile = { ...profile };
            const finalVisibility = clampStreetVisibility(visibility);

            if (
                !finalProfile.coordinates?.trim() &&
                (finalProfile.address?.city?.trim() || finalProfile.address?.zipCode?.trim())
            ) {
                const autoCoords = await geocodeAddress(finalProfile.address, 2).catch(() => null);
                if (autoCoords) {
                    finalProfile = {
                        ...finalProfile,
                        coordinates: roundCoordinates(autoCoords, 2) || autoCoords,
                    };
                }
            }

            finalProfile = roundProfileCoordinatesIfPublic(finalProfile, finalVisibility.coordinates);

            await onSave({
                profile: finalProfile,
                visibility: finalVisibility,
                skills: skills.map((s) => ({
                    title: s.title.trim(),
                    text: s.text?.trim() || undefined,
                    pricing: s.pricing?.trim() || undefined,
                    isRemote: s.isRemote ?? false,
                })),
                maxHops: myProfile.maxHops,
                validityYears: myProfile.validityYears ?? 2,
                allowInquiries,
                published,
            });
            onClose();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsSaving(false);
        }
    };

    return {
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
    };
}
