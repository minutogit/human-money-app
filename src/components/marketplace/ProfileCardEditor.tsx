// src/components/marketplace/ProfileCardEditor.tsx
import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import {
    User,
    MapPin,
    Mail,
    Phone,
    Globe,
    Plus,
    Trash2,
    ChevronUp,
    ChevronDown,
    Eye,
    Users,
    EyeOff,
    Briefcase,
    Search,
    Upload,
} from 'lucide-react';
import {
    CardFieldVisibility,
    FieldAudience,
    FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest,
    FrontendUserProfile,
    Skill,
} from '../../types';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { InfoHint } from '../ui/InfoHint';
import { Advanced } from '../ui/Advanced';
import { PresetOrCustom } from '../ui/PresetOrCustom';
import {
    CoordinatePrecision,
    roundCoordinates,
    getCurrentLocation,
    geocodeAddress,
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

    const [profile, setProfile] = useState<FrontendUserProfile>(myProfile.profile);
    const [visibility, setVisibility] = useState<CardFieldVisibility>(() => ({
        ...myProfile.visibility,
        street: myProfile.visibility.street === 'everyone' ? 'contacts' : myProfile.visibility.street,
    }));
    const [skills, setSkills] = useState<Skill[]>(myProfile.skills || []);
    const [allowInquiries, setAllowInquiries] = useState<boolean>(myProfile.allowInquiries);
    const [published, setPublished] = useState<boolean>(myProfile.published);

    const [previewPerspective, setPreviewPerspective] = useState<'contact' | 'forwarded'>('contact');
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const [coordPrecision, setCoordPrecision] = useState<CoordinatePrecision>(2);
    const [isLocating, setIsLocating] = useState(false);
    const [geoError, setGeoError] = useState<string | null>(null);
    const avatarInputRef = useRef<HTMLInputElement | null>(null);

    const handleAvatarFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        try {
            const webpDataUri = await compressAvatarToWebpDataUri(file);
            setProfile((prev) => ({ ...prev, pictureUrl: webpDataUri }));
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            e.target.value = '';
        }
    };

    // Synchronize initial state and enforce P8 guardrails (street never public, public coords max ~1 km)
    useEffect(() => {
        if (isOpen) {
            const clampedVis: CardFieldVisibility = {
                ...myProfile.visibility,
                street: myProfile.visibility.street === 'everyone' ? 'contacts' : myProfile.visibility.street,
            };
            const initialProfile = { ...myProfile.profile };
            if (clampedVis.coordinates === 'everyone' && initialProfile.coordinates?.trim()) {
                initialProfile.coordinates = roundCoordinates(initialProfile.coordinates, 2) || initialProfile.coordinates;
            }
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
        }
    }, [myProfile, isOpen]);

    const applyPreset = (presetId: CardPresetId) => {
        const nextVis = { ...CARD_VISIBILITY_PRESETS[presetId] };
        setVisibility(nextVis);
        if (nextVis.coordinates === 'everyone') {
            setCoordPrecision(2);
            setProfile((prev) => {
                if (!prev.coordinates?.trim()) return prev;
                const rounded = roundCoordinates(prev.coordinates, 2);
                return rounded ? { ...prev, coordinates: rounded } : prev;
            });
        }
    };

    const updateVisibility = (key: keyof CardFieldVisibility, val: FieldAudience) => {
        const sanitizedVal: FieldAudience = key === 'street' && val === 'everyone' ? 'contacts' : val;
        setVisibility((prev) => ({ ...prev, [key]: sanitizedVal }));
        if (key === 'coordinates' && sanitizedVal !== 'contacts') {
            setCoordPrecision(2);
            if (sanitizedVal === 'everyone') {
                setProfile((prev) => {
                    if (!prev.coordinates?.trim()) return prev;
                    const rounded = roundCoordinates(prev.coordinates, 2);
                    return rounded ? { ...prev, coordinates: rounded } : prev;
                });
            }
        }
    };

    // Cycle visibility: everyone -> contacts -> nobody -> everyone (street only cycles contacts <-> nobody)
    const cycleVisibility = (key: keyof CardFieldVisibility) => {
        const order: FieldAudience[] = key === 'street'
            ? ['contacts', 'nobody']
            : ['everyone', 'contacts', 'nobody'];
        const current = visibility[key];
        const idx = order.indexOf(current);
        const nextIdx = idx === -1 ? 0 : (idx + 1) % order.length;
        updateVisibility(key, order[nextIdx]);
    };

    const renderVisibilityButton = (key: keyof CardFieldVisibility) => {
        const current = visibility[key];
        let label = t('marketplace.visibilityEveryone', 'Alle');
        let icon = <Eye size={13} className="text-emerald-600" />;
        let pillStyle = 'bg-emerald-50 text-emerald-800 border-emerald-200 hover:bg-emerald-100';

        if (current === 'contacts') {
            label = t('marketplace.visibilityContacts', 'Nur Kontakte');
            icon = <Users size={13} className="text-sky-600" />;
            pillStyle = 'bg-sky-50 text-sky-800 border-sky-200 hover:bg-sky-100';
        } else if (current === 'nobody') {
            label = t('marketplace.visibilityNobody', 'Niemand');
            icon = <EyeOff size={13} className="text-slate-500" />;
            pillStyle = 'bg-slate-100 text-slate-700 border-slate-200 hover:bg-slate-200';
        }

        const titleText = key === 'street'
            ? t('marketplace.visibilityCycleStreetTooltip', 'Klicken zum Wechseln: Nur Kontakte ➔ Niemand')
            : t('marketplace.visibilityCycleTooltip', 'Klicken zum Wechseln: Alle ➔ Nur Kontakte ➔ Niemand');

        return (
            <Advanced>
                <button
                    type="button"
                    onClick={() => cycleVisibility(key)}
                    className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-semibold border transition-all cursor-pointer ${pillStyle}`}
                    title={titleText}
                    data-testid={`visibility-btn-${key}`}
                >
                    {icon}
                    <span>{label}</span>
                </button>
            </Advanced>
        );
    };

    const handlePrecisionChange = (newPrecision: CoordinatePrecision) => {
        const effectivePrecision: CoordinatePrecision = visibility.coordinates === 'contacts' ? newPrecision : 2;
        setCoordPrecision(effectivePrecision);
        if (profile.coordinates?.trim()) {
            const rounded = roundCoordinates(profile.coordinates, effectivePrecision);
            if (rounded) {
                setProfile((prev) => ({ ...prev, coordinates: rounded }));
            }
        }
    };

    const handleFetchGps = async () => {
        setIsLocating(true);
        setGeoError(null);
        const targetVisibility: FieldAudience = visibility.coordinates === 'contacts' ? 'contacts' : 'everyone';
        const effectivePrecision: CoordinatePrecision = targetVisibility === 'contacts' ? coordPrecision : 2;
        try {
            const rawCoords = await getCurrentLocation(6);
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

    const handleGeocodeAddress = async () => {
        setIsLocating(true);
        setGeoError(null);
        const targetVisibility: FieldAudience = visibility.coordinates === 'contacts' ? 'contacts' : 'everyone';
        const effectivePrecision: CoordinatePrecision = targetVisibility === 'contacts' ? coordPrecision : 2;
        try {
            const rawCoords = await geocodeAddress(profile.address || {}, 6);
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

    // Skills management
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

    const handleSkillChange = (index: number, field: keyof Skill, value: any) => {
        setSkills(
            skills.map((s, i) => {
                if (i !== index) return s;
                return { ...s, [field]: value };
            })
        );
    };

    // Form submit
    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setError(null);

        // Validate skills
        for (let i = 0; i < skills.length; i++) {
            const s = skills[i];
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
                setError(t('marketplace.errorPricingTooLong', 'Preisangabe darf maximal 40 Zeichen lang sein'));
                return;
            }
        }

        setIsSaving(true);
        try {
            let finalProfile: FrontendUserProfile = { ...profile };
            const finalVisibility: CardFieldVisibility = {
                ...visibility,
                street: visibility.street === 'everyone' ? 'contacts' : visibility.street,
            };

            if (!finalProfile.coordinates?.trim() && (finalProfile.address?.city?.trim() || finalProfile.address?.zipCode?.trim())) {
                const autoCoords = await geocodeAddress(finalProfile.address, 2).catch(() => null);
                if (autoCoords) {
                    finalProfile = {
                        ...finalProfile,
                        coordinates: roundCoordinates(autoCoords, 2) || autoCoords,
                    };
                }
            }

            if (finalVisibility.coordinates === 'everyone' && finalProfile.coordinates?.trim()) {
                finalProfile = {
                    ...finalProfile,
                    coordinates: roundCoordinates(finalProfile.coordinates, 2) || finalProfile.coordinates,
                };
            }

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

    // Live preview filter helper
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

                {/* Publish Toggle Banner */}
                <label className="flex items-start gap-3 p-4 rounded-2xl border border-theme-primary/25 bg-theme-primary/5 cursor-pointer hover:bg-theme-primary/10 transition-colors">
                    <input
                        type="checkbox"
                        checked={published}
                        onChange={(e) => setPublished(e.target.checked)}
                        data-testid="editor-publish-checkbox"
                        className="mt-0.5 w-4 h-4 rounded accent-theme-primary cursor-pointer shrink-0"
                    />
                    <div className="min-w-0 space-y-0.5">
                        <span className="block text-xs font-bold text-theme-secondary">
                            {t('marketplace.publishCardSwitchLabel', 'Visitenkarte veröffentlichen & weiterreichen')}
                        </span>
                        <span className="block text-[11px] text-theme-light leading-relaxed">
                            {t(
                                'marketplace.publishCardSwitchHint',
                                'Aus: Nur lokal auf diesem Gerät gespeichert. · An: Signiert, teilbar und gemäß Sichtbarkeit im Netzwerk auffindbar.'
                            )}
                        </span>
                    </div>
                </label>

                {/* Top audience preset cards & visibility explanation hint */}
                <div className="p-4 rounded-2xl bg-bg-app border border-theme-subtle space-y-3">
                    <PresetOrCustom<CardPresetId>
                        value={detectCardPreset(visibility)}
                        onChange={applyPreset}
                        gridClassName="grid grid-cols-1 sm:grid-cols-3 gap-3"
                        presets={[
                            {
                                id: 'private',
                                title: t('marketplace.cardPresetPrivateTitle', 'Privat'),
                                description: t(
                                    'marketplace.cardPresetPrivateDesc',
                                    'Nur Name & Ort öffentlich, Kontaktangaben nur für deine Kontakte.'
                                ),
                            },
                            {
                                id: 'local',
                                title: t('marketplace.cardPresetLocalTitle', 'Lokal anbieten'),
                                description: t(
                                    'marketplace.cardPresetLocalDesc',
                                    'Name, Angebote, Bild & Ort öffentlich – Telefon, E-Mail & Straße nur für Kontakte.'
                                ),
                                recommended: true,
                            },
                            {
                                id: 'business',
                                title: t('marketplace.cardPresetBusinessTitle', 'Geschäftlich'),
                                description: t(
                                    'marketplace.cardPresetBusinessDesc',
                                    'Kontaktdaten öffentlich auffindbar, Straße nur für Kontakte.'
                                ),
                            },
                        ]}
                    />
                    <InfoHint topic="cardVisibility" variant="inline" />
                </div>

                <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
                    {/* Left Column: Form Fields (7 cols) */}
                    <div className="lg:col-span-7 space-y-6">
                        {/* Section 1: Über mich */}
                        <div className="space-y-4">
                            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                                <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                                    <User size={14} className="text-theme-primary" />
                                    <span>{t('marketplace.sectionAboutMe', 'Über mich')}</span>
                                </h3>
                            </div>

                            {/* Name */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.name', 'Name')}
                                    </label>
                                    {renderVisibilityButton('name')}
                                </div>
                                <div className="grid grid-cols-2 gap-2">
                                    <Input
                                        placeholder={t('profile.firstName', 'Vorname')}
                                        value={profile.firstName || ''}
                                        onChange={(e) => setProfile({ ...profile, firstName: e.target.value })}
                                    />
                                    <Input
                                        placeholder={t('profile.lastName', 'Nachname')}
                                        value={profile.lastName || ''}
                                        onChange={(e) => setProfile({ ...profile, lastName: e.target.value })}
                                    />
                                </div>
                            </div>

                            {/* Organisation */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.organization', 'Organisation / Betrieb')}
                                    </label>
                                    {renderVisibilityButton('organization')}
                                </div>
                                <Input
                                    placeholder={t('profile.organizationPlaceholder', 'z. B. Naturwerkstatt')}
                                    value={profile.organization || ''}
                                    onChange={(e) => setProfile({ ...profile, organization: e.target.value })}
                                />
                            </div>

                            {/* Gemeinschaft */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.community', 'Gemeinschaft')}
                                    </label>
                                    {renderVisibilityButton('community')}
                                </div>
                                <Input
                                    placeholder={t('profile.communityPlaceholder', 'z. B. Tauschring Alztal')}
                                    value={profile.community || ''}
                                    onChange={(e) => setProfile({ ...profile, community: e.target.value })}
                                />
                            </div>

                            {/* Profilbild / Logo (96x96 Avatar Upload) */}
                            <div className="space-y-2">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('marketplace.avatarLabel', 'Profilbild / Logo')}
                                    </label>
                                    {renderVisibilityButton('picture')}
                                </div>
                                <div className="flex items-center gap-4">
                                    {/* Circular 96x96 preview */}
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
                                                    onClick={() => setProfile((prev) => ({ ...prev, pictureUrl: undefined }))}
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

                            {/* Geschlecht */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.gender', 'Geschlecht')}
                                    </label>
                                    {renderVisibilityButton('gender')}
                                </div>
                                <Input
                                    placeholder={t('profile.genderPlaceholder', 'z. B. weiblich / männlich')}
                                    value={profile.gender || ''}
                                    onChange={(e) => setProfile({ ...profile, gender: e.target.value })}
                                />
                            </div>
                        </div>

                        {/* Section 2: Adresse */}
                        <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                                <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                                    <MapPin size={14} className="text-theme-primary" />
                                    <span>{t('marketplace.sectionAddress', 'Adresse')}</span>
                                </h3>
                            </div>

                            {/* Stadt / Ort / Land */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.city', 'Ort & PLZ')}
                                    </label>
                                    {renderVisibilityButton('city')}
                                </div>
                                <div className="grid grid-cols-3 gap-2">
                                    <Input
                                        placeholder={t('profile.zipCode', 'PLZ')}
                                        value={profile.address?.zipCode || ''}
                                        onChange={(e) =>
                                            setProfile({
                                                ...profile,
                                                address: { ...profile.address, zipCode: e.target.value },
                                            })
                                        }
                                    />
                                    <div className="col-span-2">
                                        <Input
                                            placeholder={t('profile.city', 'Ort')}
                                            value={profile.address?.city || ''}
                                            onChange={(e) =>
                                                setProfile({
                                                    ...profile,
                                                    address: { ...profile.address, city: e.target.value },
                                                })
                                            }
                                        />
                                    </div>
                                </div>
                            </div>

                            {/* Straße & Hausnummer */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.street', 'Straße & Hausnummer')}
                                    </label>
                                    {renderVisibilityButton('street')}
                                </div>
                                <div className="grid grid-cols-3 gap-2">
                                    <div className="col-span-2">
                                        <Input
                                            placeholder={t('profile.street', 'Straße')}
                                            value={profile.address?.street || ''}
                                            onChange={(e) =>
                                                setProfile({
                                                    ...profile,
                                                    address: { ...profile.address, street: e.target.value },
                                                })
                                            }
                                        />
                                    </div>
                                    <Input
                                        placeholder={t('profile.houseNumber', 'Nr.')}
                                        value={profile.address?.houseNumber || ''}
                                        onChange={(e) =>
                                            setProfile({
                                                ...profile,
                                                address: { ...profile.address, houseNumber: e.target.value },
                                            })
                                        }
                                    />
                                </div>
                            </div>

                            {/* Coordinates (Expert mode only) */}
                            <Advanced>
                                <div className="space-y-2">
                                    <div className="flex items-center justify-between">
                                        <label className="text-[11px] font-bold text-theme-secondary">
                                            {t('profile.coordinates', 'GPS-Koordinaten')}
                                        </label>
                                        {renderVisibilityButton('coordinates')}
                                    </div>

                                    <div className="flex flex-col sm:flex-row gap-2">
                                        <div className="flex-1">
                                            <Input
                                                placeholder="z. B. 48.14, 11.58"
                                                value={profile.coordinates || ''}
                                                onChange={(e) => setProfile({ ...profile, coordinates: e.target.value })}
                                                data-testid="profile-coordinates-input"
                                            />
                                        </div>
                                        <select
                                            value={coordPrecision}
                                            onChange={(e) => handlePrecisionChange(Number(e.target.value) as CoordinatePrecision)}
                                            className="px-2.5 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                                            data-testid="coord-precision-select"
                                            title={t('profile.coordPrecisionTooltip', 'Genauigkeit der Koordinaten')}
                                        >
                                            <option value={2}>{t('profile.coordPrecision2', 'Ungefähr (~1 km, empfohlen)')}</option>
                                            <option value={3} disabled={visibility.coordinates !== 'contacts'}>
                                                {t('profile.coordPrecision3', 'Mittel – Nachbarschaft (~100 m)')}
                                            </option>
                                            <option value={4} disabled={visibility.coordinates !== 'contacts'}>
                                                {t('profile.coordPrecision4', 'Exakt – Adresse (~10 m)')}
                                            </option>
                                        </select>
                                    </div>

                                    {/* Action Buttons: GPS & Aus Ort ermitteln */}
                                    <div className="flex flex-wrap items-center gap-2">
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="outline"
                                            disabled={isLocating}
                                            onClick={handleFetchGps}
                                            data-testid="fetch-gps-btn"
                                            className="flex items-center gap-1.5"
                                        >
                                            <MapPin size={12} className="text-theme-primary" />
                                            <span>{t('profile.gpsButton', 'GPS')}</span>
                                        </Button>

                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="outline"
                                            disabled={isLocating || (!profile.address?.city?.trim() && !profile.address?.zipCode?.trim())}
                                            onClick={handleGeocodeAddress}
                                            data-testid="geocode-address-btn"
                                            className="flex items-center gap-1.5"
                                        >
                                            <Search size={12} className="text-theme-primary" />
                                            <span>{t('profile.autoAddressButton', 'Aus Ort ermitteln')}</span>
                                        </Button>

                                        {isLocating && (
                                            <span className="text-[11px] text-theme-light italic">
                                                {t('profile.locatingInProgress', 'Wird ermittelt...')}
                                            </span>
                                        )}
                                    </div>

                                    {geoError && (
                                        <p className="text-xs text-rose-600 bg-rose-50 p-2 rounded-lg border border-rose-200">
                                            {geoError}
                                        </p>
                                    )}

                                    {/* Nudge info box if empty or visibility is nobody */}
                                    {(!profile.coordinates?.trim() || visibility.coordinates === 'nobody') && (
                                        <div
                                            className="p-2.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-950 text-xs leading-relaxed"
                                            data-testid="coordinates-recommended-hint"
                                        >
                                            {t(
                                                'marketplace.coordinatesRecommendedHint',
                                                '💡 Empfohlen für lokale Angebote: Trage deinen ungefähren Standort (~1 km Schutz) ein, damit Nachbarn deine Angebote im Umkreis finden können.'
                                            )}
                                        </div>
                                    )}
                                </div>
                            </Advanced>
                        </div>

                        {/* Section 3: Kontakt */}
                        <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                                <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                                    <Mail size={14} className="text-theme-primary" />
                                    <span>{t('marketplace.sectionContact', 'Kontakt')}</span>
                                </h3>
                            </div>

                            {/* E-Mail */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.email', 'E-Mail')}
                                    </label>
                                    {renderVisibilityButton('email')}
                                </div>
                                <Input
                                    type="email"
                                    placeholder="name@beispiel.de"
                                    value={profile.email || ''}
                                    onChange={(e) => setProfile({ ...profile, email: e.target.value })}
                                />
                            </div>

                            {/* Telefon */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.phone', 'Telefon')}
                                    </label>
                                    {renderVisibilityButton('phone')}
                                </div>
                                <Input
                                    type="tel"
                                    placeholder="+49 ..."
                                    value={profile.phone || ''}
                                    onChange={(e) => setProfile({ ...profile, phone: e.target.value })}
                                />
                            </div>

                            {/* Webseite */}
                            <div className="space-y-1.5">
                                <div className="flex items-center justify-between">
                                    <label className="text-[11px] font-bold text-theme-secondary">
                                        {t('profile.url', 'Webseite')}
                                    </label>
                                    {renderVisibilityButton('url')}
                                </div>
                                <Input
                                    placeholder="https://..."
                                    value={profile.url || ''}
                                    onChange={(e) => setProfile({ ...profile, url: e.target.value })}
                                />
                            </div>

                            <Advanced>
                                <label className="flex items-start gap-3 p-3 rounded-xl bg-bg-app/50 border border-theme-subtle cursor-pointer hover:bg-bg-app transition-colors">
                                    <input
                                        type="checkbox"
                                        checked={allowInquiries}
                                        onChange={(e) => setAllowInquiries(e.target.checked)}
                                        className="mt-0.5 rounded text-theme-primary focus:ring-theme-primary cursor-pointer"
                                        data-testid="profile-allow-inquiries-checkbox"
                                    />
                                    <div className="min-w-0">
                                        <span className="text-xs font-bold text-theme-secondary block">
                                            {t('marketplace.allowInquiriesLabel', 'Anfragen erlauben')}
                                        </span>
                                        <span className="text-[11px] text-theme-light block mt-0.5 leading-snug">
                                            {t(
                                                'marketplace.profileAllowInquiriesDesc',
                                                'Andere können dir über ihren Sync-Dienst eine Nachricht oder Reservierungsanfrage senden (gilt für alle deine Inserate).'
                                            )}
                                        </span>
                                    </div>
                                </label>
                            </Advanced>
                        </div>

                        {/* Section 4: Fähigkeiten (max 8) */}
                        <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                                <div>
                                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                                        <Briefcase size={14} className="text-theme-primary" />
                                        <span>{t('marketplace.sectionSkills', 'Fähigkeiten & Angebote')}</span>
                                    </h3>
                                    <p className="text-[11px] text-theme-light">
                                        {t('marketplace.skillsDesc', 'Was du dauerhaft kannst und für andere tust (max. 8)')}
                                    </p>
                                </div>
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    data-testid="add-skill-button"
                                    onClick={handleAddSkill}
                                    disabled={skills.length >= 8}
                                    className="flex items-center gap-1"
                                >
                                    <Plus size={14} />
                                    <span>{t('marketplace.addSkillBtn', 'Fähigkeit hinzufügen')}</span>
                                </Button>
                            </div>

                            {skills.length === 0 ? (
                                <div className="p-4 rounded-xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                                    {t('marketplace.noSkillsYet', 'Noch keine Fähigkeiten eingetragen. Füge oben dein Können hinzu.')}
                                </div>
                            ) : (
                                <div className="space-y-3">
                                    {skills.map((skill, index) => (
                                        <div
                                            key={index}
                                            className="p-3.5 rounded-2xl bg-white border border-theme-subtle space-y-2.5 shadow-xs"
                                        >
                                            <div className="flex items-center justify-between gap-2">
                                                <span className="text-[10px] font-bold text-theme-light uppercase">
                                                    #{index + 1}
                                                </span>
                                                <div className="flex items-center gap-1">
                                                    <button
                                                        type="button"
                                                        onClick={() => handleMoveSkill(index, 'up')}
                                                        disabled={index === 0}
                                                        className="p-1 rounded hover:bg-bg-app text-theme-light disabled:opacity-30"
                                                        title={t('marketplace.moveUpBtn', 'Nach oben')}
                                                    >
                                                        <ChevronUp size={14} />
                                                    </button>
                                                    <button
                                                        type="button"
                                                        onClick={() => handleMoveSkill(index, 'down')}
                                                        disabled={index === skills.length - 1}
                                                        className="p-1 rounded hover:bg-bg-app text-theme-light disabled:opacity-30"
                                                        title={t('marketplace.moveDownBtn', 'Nach unten')}
                                                    >
                                                        <ChevronDown size={14} />
                                                    </button>
                                                    <button
                                                        type="button"
                                                        onClick={() => handleRemoveSkill(index)}
                                                        className="p-1 rounded hover:bg-rose-50 text-rose-600 ml-1"
                                                        title={t('marketplace.deleteSkillBtn', 'Löschen')}
                                                    >
                                                        <Trash2 size={14} />
                                                    </button>
                                                </div>
                                            </div>

                                            <Input
                                                placeholder={t('marketplace.skillTitlePlaceholder', 'Titel, z. B. Fahrradreparatur (max 60)')}
                                                value={skill.title}
                                                data-testid={`skill-title-${index}`}
                                                onChange={(e) => handleSkillChange(index, 'title', e.target.value)}
                                                maxLength={60}
                                                required
                                            />
                                            <Input
                                                placeholder={t('marketplace.skillTextPlaceholder', 'Kurzbeschreibung (optional, max 160)')}
                                                value={skill.text || ''}
                                                data-testid={`skill-text-${index}`}
                                                onChange={(e) => handleSkillChange(index, 'text', e.target.value)}
                                                maxLength={160}
                                            />
                                            <Input
                                                placeholder={t('marketplace.skillPricingPlaceholder', 'Preis / Richtwert, z. B. 1 Minuto/Std. (optional, max 40)')}
                                                value={skill.pricing || ''}
                                                data-testid={`skill-pricing-${index}`}
                                                onChange={(e) => handleSkillChange(index, 'pricing', e.target.value)}
                                                maxLength={40}
                                            />
                                            <label className="flex items-center gap-2 pt-1 cursor-pointer">
                                                <input
                                                    type="checkbox"
                                                    checked={skill.isRemote ?? false}
                                                    onChange={(e) => handleSkillChange(index, 'isRemote', e.target.checked)}
                                                    className="rounded text-theme-primary focus:ring-theme-primary cursor-pointer"
                                                    data-testid={`skill-remote-checkbox-${index}`}
                                                />
                                                <span className="text-xs text-theme-secondary font-medium">
                                                    {t('marketplace.skillIsRemoteLabel', 'Ortsunabhängig / Online möglich (überregional)')}
                                                </span>
                                            </label>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Right Column: Live Preview (5 cols) */}
                    <div className="lg:col-span-5 sticky top-2 space-y-4">
                        <div className="p-4 rounded-2xl bg-bg-app border border-theme-subtle space-y-3">
                            <div className="flex items-center justify-between">
                                <h4 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-1.5">
                                    <Eye size={14} className="text-theme-primary" />
                                    <span>{t('marketplace.livePreviewTitle', 'So sehen andere deine Visitenkarte')}</span>
                                </h4>
                            </div>

                            {/* Perspective switch (Expert mode) */}
                            <Advanced>
                                <SegmentedTabs<'contact' | 'forwarded'>
                                    fullWidth
                                    activeTab={previewPerspective}
                                    onChange={setPreviewPerspective}
                                    tabs={[
                                        { id: 'contact', label: t('marketplace.previewContact', 'Direkter Kontakt') },
                                        { id: 'forwarded', label: t('marketplace.previewForwarded', 'Über Empfehlung') },
                                    ]}
                                />
                            </Advanced>
                        </div>

                        {/* Card preview mock */}
                        <div className="p-5 rounded-2xl bg-white border border-theme-subtle shadow-sm space-y-4">
                            <div className="flex items-start gap-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center font-bold text-base shrink-0 overflow-hidden">
                                    {isVisibleInPreview('picture') && profile.pictureUrl ? (
                                        <img src={profile.pictureUrl} alt="Avatar" className="w-full h-full object-cover" />
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

                            {/* Address & contact previews */}
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

                            {/* Skills in preview */}
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
                </div>

                {/* Footer Actions */}
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
