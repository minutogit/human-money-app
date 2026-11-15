// src/components/voucher/CreatorIdentityForm.tsx

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { UserCircle, Mail, Phone, Globe, Briefcase, Heart, ChevronDown, ChevronUp, Info, Edit3 } from "lucide-react";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { ExpertTabsLayout } from "../ui/ExpertTabsLayout";
import { CreatorData } from "../../types";
import { geocodeAddress, getCurrentLocation } from "../../utils/geoUtils";

interface CreatorIdentityFormProps {
    identity: CreatorData;
    onIdentityChange: (identity: CreatorData) => void;
    onLoadProfile: () => void;
    onCoordBlur: () => void;
    coordWarning: string;
    errors: Record<string, boolean>;
    firstNameRef: React.RefObject<HTMLInputElement | null>;
    lastNameRef: React.RefObject<HTMLInputElement | null>;
    requiresGender?: boolean;
    genderRef?: React.RefObject<HTMLSelectElement | null>;
}

export function CreatorIdentityForm({
    identity,
    onIdentityChange,
    onLoadProfile,
    onCoordBlur,
    coordWarning,
    errors,
    firstNameRef,
    lastNameRef,
    requiresGender = false,
    genderRef
}: CreatorIdentityFormProps) {
    const { t } = useTranslation();
    const [isLocating, setIsLocating] = useState(false);
    const [isGeocoding, setIsGeocoding] = useState(false);
    const [geoFeedback, setGeoFeedback] = useState("");
    const [geoFeedbackError, setGeoFeedbackError] = useState(false);
    const [isNeedsExpanded, setIsNeedsExpanded] = useState(false);
    const [isEditingSimple, setIsEditingSimple] = useState(false);

    const handleUseGPS = async () => {
        setIsLocating(true);
        setGeoFeedback(t('profile.locating'));
        setGeoFeedbackError(false);
        try {
            const coords = await getCurrentLocation();
            handleChange("coordinates", coords);
            setGeoFeedback(t('profile.locationDetected'));
            setTimeout(() => setGeoFeedback(""), 3000);
        } catch (e) {
            const msg = e instanceof Error ? e.message : t('profile.gpsFailed');
            setGeoFeedback(msg);
            setGeoFeedbackError(true);
            setTimeout(() => setGeoFeedback(""), 5000);
        } finally {
            setIsLocating(false);
        }
    };

    const handleGeocodeAddress = async () => {
        if (!identity.address) return;
        setIsGeocoding(true);
        setGeoFeedback(t('profile.geocodingAddress'));
        setGeoFeedbackError(false);
        try {
            const coords = await geocodeAddress(identity.address);
            handleChange("coordinates", coords);
            setGeoFeedback(t('profile.coordinatesResolved'));
            setTimeout(() => setGeoFeedback(""), 3000);
        } catch (e) {
            const msg = e instanceof Error ? e.message : t('profile.lookupFailed');
            setGeoFeedback(msg);
            setGeoFeedbackError(true);
            setTimeout(() => setGeoFeedback(""), 5000);
        } finally {
            setIsGeocoding(false);
        }
    };
    const handleChange = (field: string, value: string) => {
        onIdentityChange({ ...identity, [field]: value });
    };

    const handleAddressChange = (field: string, value: string) => {
        onIdentityChange({
            ...identity,
            address: { ...identity.address, [field]: value }
        });
    };

    const hasCompleteName = Boolean(identity.firstName.trim() && identity.lastName.trim());
    const showInlineEditor = isEditingSimple || !hasCompleteName || Boolean(errors.firstName || errors.lastName);
    const displaySummary = [
        `${identity.firstName.trim()} ${identity.lastName.trim()}`.trim(),
        identity.address?.city?.trim(),
    ]
        .filter(Boolean)
        .join(", ");

    const renderGenderField = () => {
        if (!requiresGender) return null;
        return (
            <div className="space-y-2">
                <label htmlFor="creator-gender" className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                    {t('profile.gender')}
                </label>
                <select
                    id="creator-gender"
                    data-testid="creator-gender-select"
                    ref={genderRef}
                    value={identity.gender}
                    onChange={(e) => handleChange('gender', e.target.value)}
                    className={`w-full bg-white border rounded-xl px-4 py-3 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none ${errors.gender ? 'border-rose-500' : 'border-theme-subtle'}`}
                >
                    <option value="1">{t('gender.male')}</option>
                    <option value="2">{t('gender.female')}</option>
                    <option value="0">{t('gender.notKnown')}</option>
                    <option value="9">{t('gender.notApplicable')}</option>
                </select>
                <p className="text-[11px] text-theme-light">
                    {t('voucher.creator.genderRequiredHint', 'Für diesen Gutscheintyp wegen der Bürgen-Regeln erforderlich.')}
                </p>
            </div>
        );
    };

    const simpleContent = (
        <Card header={
            <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                    <UserCircle size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.creator.yourDetails')}</span>
                </div>
                {showInlineEditor && (
                    <Button type="button" variant="secondary" size="sm" onClick={onLoadProfile} className="rounded-xl px-3 h-8 text-[9px] uppercase tracking-widest">
                        {t('voucher.creator.syncFromProfile')}
                    </Button>
                )}
            </div>
        }>
            <div className="space-y-4">
                {!showInlineEditor ? (
                    <div
                        className="flex flex-wrap items-center justify-between gap-3 p-3.5 rounded-2xl bg-bg-app/60 border border-theme-subtle"
                        data-testid="creator-profile-summary"
                    >
                        <span className="text-sm font-medium text-theme-secondary">
                            {t('voucher.creator.profileAdoptedLine', {
                                summary: displaySummary,
                                defaultValue: 'Deine Angaben ({{summary}}) werden übernommen',
                            })}
                        </span>
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setIsEditingSimple(true)}
                            data-testid="creator-profile-edit-btn"
                            className="gap-1.5"
                        >
                            <Edit3 size={14} />
                            <span>{t('common.edit', 'Bearbeiten')}</span>
                        </Button>
                    </div>
                ) : (
                    <div className="space-y-4" data-testid="creator-simple-editor">
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                            <div className="space-y-2">
                                <label htmlFor="creator-first-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.creator.firstNameRequired')}</label>
                                <Input id="creator-first-name" ref={firstNameRef} value={identity.firstName} onChange={(e) => handleChange('firstName', e.target.value)} className={errors.firstName ? 'border-rose-500' : ''} />
                            </div>
                            <div className="space-y-2">
                                <label htmlFor="creator-last-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.creator.lastNameRequired')}</label>
                                <Input id="creator-last-name" ref={lastNameRef} value={identity.lastName} onChange={(e) => handleChange('lastName', e.target.value)} className={errors.lastName ? 'border-rose-500' : ''} />
                            </div>
                            <div className="space-y-2">
                                <label htmlFor="creator-city" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.city')}</label>
                                <Input id="creator-city" value={identity.address.city || ""} onChange={(e) => handleAddressChange('city', e.target.value)} />
                            </div>
                        </div>
                        {hasCompleteName && (
                            <div className="flex justify-end">
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    onClick={() => setIsEditingSimple(false)}
                                    data-testid="creator-profile-done-btn"
                                >
                                    {t('common.done', 'Fertig')}
                                </Button>
                            </div>
                        )}
                    </div>
                )}

                {renderGenderField()}
            </div>
        </Card>
    );

    const expertContent = (
        <Card header={
            <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                    <UserCircle size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.creator.yourDetails')}</span>
                </div>
                <Button type="button" variant="secondary" size="sm" onClick={onLoadProfile} className="rounded-xl px-3 h-8 text-[9px] uppercase tracking-widest">
                    {t('voucher.creator.syncFromProfile')}
                </Button>
            </div>
        }>
            <div className="space-y-8">
                {/* Personal Details Section */}
                <div className="space-y-4">
                    <h4 className="text-[10px] font-black text-theme-primary uppercase tracking-widest">{t('voucher.creator.personalDetails')}</h4>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div className="space-y-2">
                            <label htmlFor="creator-first-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.creator.firstNameRequired')}</label>
                            <Input id="creator-first-name" ref={firstNameRef} value={identity.firstName} onChange={(e) => handleChange('firstName', e.target.value)} className={errors.firstName ? 'border-rose-500' : ''} />
                        </div>
                        <div className="space-y-2">
                            <label htmlFor="creator-last-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.creator.lastNameRequired')}</label>
                            <Input id="creator-last-name" ref={lastNameRef} value={identity.lastName} onChange={(e) => handleChange('lastName', e.target.value)} className={errors.lastName ? 'border-rose-500' : ''} />
                        </div>
                    </div>
                    <div className={`grid grid-cols-1 gap-4 ${requiresGender ? 'md:grid-cols-3' : 'md:grid-cols-2'}`}>
                        {renderGenderField()}
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.community')}</label>
                            <Input value={identity.community || ""} onChange={(e) => handleChange('community', e.target.value)} placeholder={t('common.optional')} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.organizationCompany')}</label>
                            <Input value={identity.organization || ""} onChange={(e) => handleChange('organization', e.target.value)} placeholder={t('common.optional')} />
                        </div>
                    </div>
                </div>

                {/* Address & Location Section */}
                <div className="border-t border-theme-subtle/40 pt-6 space-y-4">
                    <h4 className="text-[10px] font-black text-theme-primary uppercase tracking-widest">{t('voucher.creator.addressSection')}</h4>
                    <div className="grid grid-cols-4 gap-4">
                        <div className="col-span-3 space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.street')}</label>
                            <Input value={identity.address.street || ""} onChange={(e) => handleAddressChange('street', e.target.value)} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.houseNumber')}</label>
                            <Input value={identity.address.houseNumber || ""} onChange={(e) => handleAddressChange('houseNumber', e.target.value)} />
                        </div>
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.zipCode')}</label>
                            <Input value={identity.address.zipCode || ""} onChange={(e) => handleAddressChange('zipCode', e.target.value)} />
                        </div>
                        <div className="col-span-1 md:col-span-2 space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.city')}</label>
                            <Input value={identity.address.city || ""} onChange={(e) => handleAddressChange('city', e.target.value)} />
                        </div>
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.country')}</label>
                            <Input value={identity.address.country || ""} onChange={(e) => handleAddressChange('country', e.target.value)} />
                        </div>
                        <div className="col-span-1 md:col-span-2 space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.coordinatesLabel')}</label>
                            <Input value={identity.coordinates} placeholder={t('profile.coordinatesPlaceholder')} onChange={(e) => handleChange('coordinates', e.target.value)} onBlur={onCoordBlur} className={coordWarning ? 'border-rose-500' : ''} />
                            <div className="flex items-center justify-between flex-wrap gap-2 min-h-[26px]">
                                <div className="flex-1 min-w-[120px]">
                                    {coordWarning && <p className="text-[10px] text-rose-500 font-bold">{t('voucher.creator.invalidCoordinates')}</p>}
                                    {geoFeedback && (
                                        <p className={`text-[10px] font-bold ${geoFeedbackError ? 'text-rose-500 animate-pulse' : 'text-emerald-500'}`}>
                                            {geoFeedback}
                                        </p>
                                    )}
                                </div>
                                <div className="flex gap-2">
                                    <button
                                        type="button"
                                        onClick={handleUseGPS}
                                        disabled={isLocating || isGeocoding}
                                        className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20 disabled:opacity-50 cursor-pointer"
                                        title={t('profile.gpsTooltip')}
                                    >
                                        {isLocating ? <span className="w-2.5 h-2.5 border border-theme-primary border-t-transparent rounded-full animate-spin inline-block"></span> : "📍"}
                                        <span>{t('profile.gpsButton')}</span>
                                    </button>
                                    <button
                                        type="button"
                                        onClick={handleGeocodeAddress}
                                        disabled={isLocating || isGeocoding || !identity.address.city}
                                        className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20 disabled:opacity-50 cursor-pointer"
                                        title={t('profile.autoAddressTooltip')}
                                    >
                                        {isGeocoding ? <span className="w-2.5 h-2.5 border border-theme-primary border-t-transparent rounded-full animate-spin inline-block"></span> : "🔍"}
                                        <span>{t('profile.autoAddressButton')}</span>
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-6 pt-4 border-t border-theme-subtle/40">
                    <div className="space-y-6">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Mail size={10}/> {t('profile.publicEmail')}</label>
                            <Input type="email" value={identity.email || ""} onChange={(e) => handleChange('email', e.target.value)} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Phone size={10}/> {t('profile.publicPhone')}</label>
                            <Input type="tel" value={identity.phone || ""} onChange={(e) => handleChange('phone', e.target.value)} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Globe size={10}/> {t('profile.website')}</label>
                            <Input value={identity.url || ""} onChange={(e) => handleChange('url', e.target.value)} placeholder={t('profile.websitePlaceholder')} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><UserCircle size={10}/> {t('profile.pictureUrl')}</label>
                            <Input value={identity.pictureUrl || ""} onChange={(e) => handleChange('pictureUrl', e.target.value)} placeholder={t('profile.pictureUrlPlaceholder')} />
                        </div>
                    </div>
                    <div className="space-y-6">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Briefcase size={10}/> {t('profile.serviceOffer')}</label>
                            <textarea value={identity.serviceOffer || ""} onChange={(e) => handleChange('serviceOffer', e.target.value)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[140px]" placeholder={t('voucher.creator.serviceOfferPlaceholder')} />
                        </div>
                        <div className="border border-theme-subtle rounded-2xl p-4 bg-bg-app/40 space-y-3">
                            <button
                                type="button"
                                onClick={() => setIsNeedsExpanded(!isNeedsExpanded)}
                                className="w-full flex items-center justify-between text-left text-xs font-bold text-theme-secondary hover:text-theme-primary transition-colors"
                            >
                                <span className="flex items-center gap-1.5">
                                    <Heart size={14} className="text-theme-primary" />
                                    <span>{t('voucher.creator.needsLabel', 'Was ich im Gegenzug gebrauchen kann (optional)')}</span>
                                </span>
                                {isNeedsExpanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                            </button>
                            {isNeedsExpanded && (
                                <div className="pt-2 animate-in fade-in duration-200">
                                    <textarea
                                        value={identity.needs || ""}
                                        onChange={(e) => handleChange('needs', e.target.value)}
                                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[100px]"
                                        placeholder={t('voucher.creator.needsPlaceholder')}
                                    />
                                </div>
                            )}
                        </div>
                    </div>
                </div>

                {/* Bottom hint for profile unification */}
                <div className="mt-6 pt-4 border-t border-theme-subtle/50 flex items-center gap-2 text-xs text-theme-light">
                    <Info size={15} className="shrink-0 text-theme-primary" />
                    <span>{t('voucher.creator.voucherOnlyHint', 'Änderungen hier gelten nur für diesen Gutschein. Dein Profil bearbeitest du im Marktplatz.')}</span>
                </div>
            </div>
        </Card>
    );

    return (
        <ExpertTabsLayout
            simpleContent={simpleContent}
            expertContent={expertContent}
        />
    );
}
