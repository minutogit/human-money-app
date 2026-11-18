// src/components/marketplace/ProfileCardContactSection.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Mail, MapPin, Search } from 'lucide-react';
import { CardFieldVisibility, FrontendUserProfile } from '../../types';
import { Advanced } from '../ui/Advanced';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { CoordinatePrecision } from '../../utils/geoUtils';
import { VisibilityCycleButton } from './ProfileCardVisibilitySection';

export interface ProfileCardContactSectionProps {
    profile: FrontendUserProfile;
    visibility: CardFieldVisibility;
    coordPrecision: CoordinatePrecision;
    isLocating: boolean;
    geoError: string | null;
    allowInquiries: boolean;
    onProfileChange: React.Dispatch<React.SetStateAction<FrontendUserProfile>>;
    onCycleVisibility: (key: keyof CardFieldVisibility) => void;
    onPrecisionChange: (precision: CoordinatePrecision) => void;
    onFetchGps: () => void;
    onGeocodeAddress: () => void;
    onAllowInquiriesChange: (allow: boolean) => void;
}

/**
 * Address, Expert-mode GPS coordinate precision/lookup, and contact channel section.
 */
export const ProfileCardContactSection: React.FC<ProfileCardContactSectionProps> = ({
    profile,
    visibility,
    coordPrecision,
    isLocating,
    geoError,
    allowInquiries,
    onProfileChange,
    onCycleVisibility,
    onPrecisionChange,
    onFetchGps,
    onGeocodeAddress,
    onAllowInquiriesChange,
}) => {
    const { t } = useTranslation();

    return (
        <>
            <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <MapPin size={14} className="text-theme-primary" />
                        <span>{t('marketplace.sectionAddress', 'Adresse')}</span>
                    </h3>
                </div>

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-bold text-theme-secondary">
                            {t('profile.city', 'Ort & PLZ')}
                        </label>
                        <VisibilityCycleButton field="city" visibility={visibility} onCycle={onCycleVisibility} />
                    </div>
                    <div className="grid grid-cols-3 gap-2">
                        <Input
                            placeholder={t('profile.zipCode', 'PLZ')}
                            value={profile.address?.zipCode || ''}
                            onChange={(e) =>
                                onProfileChange({
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
                                    onProfileChange({
                                        ...profile,
                                        address: { ...profile.address, city: e.target.value },
                                    })
                                }
                            />
                        </div>
                    </div>
                </div>

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-bold text-theme-secondary">
                            {t('profile.street', 'Straße & Hausnummer')}
                        </label>
                        <VisibilityCycleButton field="street" visibility={visibility} onCycle={onCycleVisibility} />
                    </div>
                    <div className="grid grid-cols-3 gap-2">
                        <div className="col-span-2">
                            <Input
                                placeholder={t('profile.street', 'Straße')}
                                value={profile.address?.street || ''}
                                onChange={(e) =>
                                    onProfileChange({
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
                                onProfileChange({
                                    ...profile,
                                    address: { ...profile.address, houseNumber: e.target.value },
                                })
                            }
                        />
                    </div>
                </div>

                <Advanced>
                    <div className="space-y-2">
                        <div className="flex items-center justify-between">
                            <label className="text-[11px] font-bold text-theme-secondary">
                                {t('profile.coordinates', 'GPS-Koordinaten')}
                            </label>
                            <VisibilityCycleButton field="coordinates" visibility={visibility} onCycle={onCycleVisibility} />
                        </div>

                        <div className="flex flex-col sm:flex-row gap-2">
                            <div className="flex-1">
                                <Input
                                    placeholder="z. B. 48.14, 11.58"
                                    value={profile.coordinates || ''}
                                    onChange={(e) => onProfileChange({ ...profile, coordinates: e.target.value })}
                                    data-testid="profile-coordinates-input"
                                />
                            </div>
                            <select
                                value={coordPrecision}
                                onChange={(e) => onPrecisionChange(Number(e.target.value) as CoordinatePrecision)}
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

                        <div className="flex flex-wrap items-center gap-2">
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                disabled={isLocating}
                                onClick={onFetchGps}
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
                                onClick={onGeocodeAddress}
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

                        {(!profile.coordinates?.trim() || visibility.coordinates === 'nobody') && (
                            <div
                                className="p-2.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-950 text-xs leading-relaxed"
                                data-testid="coordinates-recommended-hint"
                            >
                                {t(
                                    'marketplace.coordinatesRecommendedHint',
                                    '💡 Empfohlen für lokale Angebote: Trage deinen ungefähren Standort (~1 km Schutz) ein, damit Nachbarn deine Angebote im Umkreis finden können.',
                                )}
                            </div>
                        )}
                    </div>
                </Advanced>
            </div>

            <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
                <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <Mail size={14} className="text-theme-primary" />
                        <span>{t('marketplace.sectionContact', 'Kontakt')}</span>
                    </h3>
                </div>

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-bold text-theme-secondary">
                            {t('profile.email', 'E-Mail')}
                        </label>
                        <VisibilityCycleButton field="email" visibility={visibility} onCycle={onCycleVisibility} />
                    </div>
                    <Input
                        type="email"
                        placeholder="name@beispiel.de"
                        value={profile.email || ''}
                        onChange={(e) => onProfileChange({ ...profile, email: e.target.value })}
                    />
                </div>

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-bold text-theme-secondary">
                            {t('profile.phone', 'Telefon')}
                        </label>
                        <VisibilityCycleButton field="phone" visibility={visibility} onCycle={onCycleVisibility} />
                    </div>
                    <Input
                        type="tel"
                        placeholder="+49 ..."
                        value={profile.phone || ''}
                        onChange={(e) => onProfileChange({ ...profile, phone: e.target.value })}
                    />
                </div>

                <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                        <label className="text-[11px] font-bold text-theme-secondary">
                            {t('profile.url', 'Webseite')}
                        </label>
                        <VisibilityCycleButton field="url" visibility={visibility} onCycle={onCycleVisibility} />
                    </div>
                    <Input
                        placeholder="https://..."
                        value={profile.url || ''}
                        onChange={(e) => onProfileChange({ ...profile, url: e.target.value })}
                    />
                </div>

                <Advanced>
                    <label className="flex items-start gap-3 p-3 rounded-xl bg-bg-app/50 border border-theme-subtle cursor-pointer hover:bg-bg-app transition-colors">
                        <input
                            type="checkbox"
                            checked={allowInquiries}
                            onChange={(e) => onAllowInquiriesChange(e.target.checked)}
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
                                    'Andere können dir über ihren Sync-Dienst eine Nachricht oder Reservierungsanfrage senden (gilt für alle deine Inserate).',
                                )}
                            </span>
                        </div>
                    </label>
                </Advanced>
            </div>
        </>
    );
};
