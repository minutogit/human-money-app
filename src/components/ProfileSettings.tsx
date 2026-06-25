// src/components/ProfileSettings.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { profileService } from '../services/profileService';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { Input } from './ui/Input';
import { PublicProfile, Address } from '../types';
import { useSession } from '../context/SessionContext';
import { normalizeCoordinates, geocodeAddress, getCurrentLocation } from '../utils/geoUtils';
import { translateError, stringifyError } from '../utils/errorHelper';
import { 
    User, 
    MapPin, 
    Globe, 
    Mail, 
    Phone, 
    Briefcase, 
    Heart, 
    Lock,
    Save,
    CheckCircle2
} from 'lucide-react';

export function ProfileSettings() {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [profile, setProfile] = useState<PublicProfile | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');
    const [coordWarning, setCoordWarning] = useState('');
    const [isLocating, setIsLocating] = useState(false);
    const [isGeocoding, setIsGeocoding] = useState(false);
    const [geoFeedback, setGeoFeedback] = useState('');
    const [geoFeedbackError, setGeoFeedbackError] = useState(false);

    const handleUseGPS = async () => {
        setIsLocating(true);
        setGeoFeedback(t('profile.locating'));
        setGeoFeedbackError(false);
        try {
            const coords = await getCurrentLocation();
            if (profile) {
                setProfile({ ...profile, coordinates: coords });
            }
            setCoordWarning('');
            setGeoFeedback(t('profile.locationDetected'));
            setTimeout(() => setGeoFeedback(''), 3000);
        } catch (e) {
            const msg = stringifyError(e) || t('profile.gpsFailed');
            setGeoFeedback(msg);
            setGeoFeedbackError(true);
            setTimeout(() => setGeoFeedback(''), 5000);
        } finally {
            setIsLocating(false);
        }
    };

    const handleGeocodeAddress = async () => {
        if (!profile?.address) return;
        setIsGeocoding(true);
        setGeoFeedback(t('profile.geocodingAddress'));
        setGeoFeedbackError(false);
        try {
            const coords = await geocodeAddress(profile.address);
            setProfile({ ...profile, coordinates: coords });
            setCoordWarning('');
            setGeoFeedback(t('profile.coordinatesResolved'));
            setTimeout(() => setGeoFeedback(''), 3000);
        } catch (e) {
            const msg = stringifyError(e) || t('profile.lookupFailed');
            setGeoFeedback(msg);
            setGeoFeedbackError(true);
            setTimeout(() => setGeoFeedback(''), 5000);
        } finally {
            setIsGeocoding(false);
        }
    };

    useEffect(() => {
        async function fetchProfile() {
            try {
                logger.info("ProfileSettings: Fetching current profile.");
                const currentProfile = await profileService.getProfile();
                setProfile(currentProfile);
            } catch (e) {
                const msg = `${t('profile.errorLoading')}: ${translateError(e, t)}`;
                logger.error(msg);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProfile();
    }, [t]);

    const handleCoordBlur = () => {
        if (!profile?.coordinates) {
            setCoordWarning('');
            return;
        }
        const normalized = normalizeCoordinates(profile.coordinates);
        if (normalized) {
            setProfile({ ...profile, coordinates: normalized });
            setCoordWarning('');
        } else {
            setCoordWarning(t('profile.invalidCoordinateFormat'));
        }
    };

    const handleSave = async () => {
        if (!profile) return;
        setIsSaving(true);
        setError('');
        setSuccess('');
        try {
            logger.info("Attempting to save profile...");
            await protectAction(async (password) => {
                await profileService.saveProfile(profile, password || undefined);
            });
            setSuccess(t('profile.updatedSuccess'));
            logger.info("Profile updated successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            setError(`${t('profile.errorUpdating')}: ${translateError(e, t)}`);
        } finally {
            setIsSaving(false);
        }
    };

    const updateAddress = (field: keyof Address, value: string) => {
        setProfile(prev => {
            if (!prev) return null;
            const newAddress = { ...(prev.address || {}), [field]: value };
            const parts = [newAddress.street, newAddress.houseNumber, newAddress.zipCode, newAddress.city, newAddress.country].filter(Boolean);
            newAddress.fullAddress = parts.join(', ');
            return { ...prev, address: newAddress };
        });
    };

    if (isLoading) return <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">{t('profile.loadingProfile')}</div>;
    if (!profile) return <div className="p-8 text-center text-rose-500 font-bold">{t('profile.notFound')}</div>;

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500 pb-20">
            {/* Privacy Shield */}
            <div className="bg-emerald-500/10 border border-emerald-500/20 p-6 rounded-[32px] flex items-start gap-5 shadow-sm">
                <div className="p-3 bg-white rounded-2xl shadow-sm text-emerald-600">
                    <Lock size={24} />
                </div>
                <div>
                    <h3 className="text-sm font-black text-emerald-900 uppercase tracking-widest mb-1">{t('profile.walletSecurity')}</h3>
                    <p className="text-xs text-emerald-800/80 font-medium leading-relaxed">
                        {t('profile.walletSecurityDesc')}
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
                {/* Basic ID */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <User size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.profileDetails')}</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    {t('profile.firstName')}
                                    <span className="text-theme-accent text-[8px]">({t('common.required')})</span>
                                </label>
                                <Input
                                    value={profile.firstName || ''}
                                    onChange={(e) => setProfile({ ...profile, firstName: e.target.value })}
                                    placeholder={t('profile.firstNamePlaceholder')}
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    {t('profile.lastName')}
                                    <span className="text-theme-accent text-[8px]">({t('common.required')})</span>
                                </label>
                                <Input
                                    value={profile.lastName || ''}
                                    onChange={(e) => setProfile({ ...profile, lastName: e.target.value })}
                                    placeholder={t('profile.lastNamePlaceholder')}
                                />
                            </div>
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.gender')}</label>
                            <select
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                value={profile.gender || ''}
                                onChange={(e) => setProfile({ ...profile, gender: e.target.value })}
                            >
                                <option value="">{t('common.select')}</option>
                                <option value="1">{t('gender.male')}</option>
                                <option value="2">{t('gender.female')}</option>
                                <option value="0">{t('gender.notKnown')}</option>
                                <option value="9">{t('gender.notApplicable')}</option>
                            </select>
                        </div>
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.organizationCompany')}</label>
                                <Input
                                    value={profile.organization || ''}
                                    onChange={(e) => setProfile({ ...profile, organization: e.target.value })}
                                    placeholder={t('profile.organizationPlaceholder')}
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.community')}</label>
                                <Input
                                    value={profile.community || ''}
                                    onChange={(e) => setProfile({ ...profile, community: e.target.value })}
                                    placeholder={t('profile.communityPlaceholder')}
                                />
                            </div>
                        </div>
                    </div>
                </Card>

                {/* Location */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <MapPin size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.location')}</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="grid grid-cols-4 gap-4">
                            <div className="col-span-3 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.street')}</label>
                                <Input
                                    value={profile.address?.street || ''}
                                    onChange={(e) => updateAddress('street', e.target.value)}
                                    placeholder={t('profile.streetPlaceholder')}
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.houseNumber')}</label>
                                <Input
                                    value={profile.address?.houseNumber || ''}
                                    onChange={(e) => updateAddress('houseNumber', e.target.value)}
                                    placeholder={t('profile.houseNumberPlaceholder')}
                                />
                            </div>
                        </div>
                        <div className="grid grid-cols-3 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.zipCode')}</label>
                                <Input
                                    value={profile.address?.zipCode || ''}
                                    onChange={(e) => updateAddress('zipCode', e.target.value)}
                                    placeholder="12345"
                                />
                            </div>
                            <div className="col-span-2 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    {t('profile.city')}
                                    <span className="text-theme-accent text-[8px]">({t('common.required')})</span>
                                </label>
                                <Input
                                    value={profile.address?.city || ''}
                                    onChange={(e) => updateAddress('city', e.target.value)}
                                    placeholder={t('profile.cityPlaceholder')}
                                />
                            </div>
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.country')}</label>
                            <Input
                                value={profile.address?.country || ''}
                                onChange={(e) => updateAddress('country', e.target.value)}
                                placeholder={t('profile.countryPlaceholder')}
                            />
                        </div>
                        <div className="space-y-2">
                            <div className="flex items-center justify-between">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.coordinates')}</label>
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
                                        disabled={isLocating || isGeocoding || !profile.address?.city}
                                        className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20 disabled:opacity-50 cursor-pointer"
                                        title={t('profile.autoAddressTooltip')}
                                    >
                                        {isGeocoding ? <span className="w-2.5 h-2.5 border border-theme-primary border-t-transparent rounded-full animate-spin inline-block"></span> : "🔍"}
                                        <span>{t('profile.autoAddressButton')}</span>
                                    </button>
                                </div>
                            </div>
                            <Input
                                value={profile.coordinates || ''}
                                onChange={(e) => {
                                    setProfile({ ...profile, coordinates: e.target.value });
                                    if (coordWarning) setCoordWarning('');
                                }}
                                onBlur={handleCoordBlur}
                                className={coordWarning ? 'border-rose-500 focus:ring-rose-500' : ''}
                                placeholder={t('profile.coordinatesPlaceholder')}
                            />
                            {coordWarning && <p className="text-[10px] text-rose-500 font-bold">{coordWarning}</p>}
                            {geoFeedback && (
                                <p className={`text-[10px] font-bold ${geoFeedbackError ? 'text-rose-500 animate-pulse' : 'text-emerald-500'}`}>
                                    {geoFeedback}
                                </p>
                            )}
                        </div>
                    </div>
                </Card>

                {/* Social & Web */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <Globe size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.contactLinks')}</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Mail size={10}/> {t('profile.publicEmail')}</label>
                            <Input
                                type="email"
                                value={profile.email || ''}
                                onChange={(e) => setProfile({ ...profile, email: e.target.value })}
                                placeholder={t('profile.emailPlaceholder')}
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Phone size={10}/> {t('profile.publicPhone')}</label>
                            <Input
                                type="tel"
                                value={profile.phone || ''}
                                onChange={(e) => setProfile({ ...profile, phone: e.target.value })}
                                placeholder={t('profile.phonePlaceholder')}
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Globe size={10}/> {t('profile.website')}</label>
                            <Input
                                value={profile.url || ''}
                                onChange={(e) => setProfile({ ...profile, url: e.target.value })}
                                placeholder={t('profile.websitePlaceholder')}
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><User size={10}/> {t('profile.pictureUrl')}</label>
                            <Input
                                value={profile.pictureUrl || ''}
                                onChange={(e) => setProfile({ ...profile, pictureUrl: e.target.value })}
                                placeholder={t('profile.pictureUrlPlaceholder')}
                            />
                        </div>
                    </div>
                </Card>

                {/* Offerings */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <Briefcase size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.community')}</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Briefcase size={10}/> {t('profile.serviceOffer')}</label>
                            <textarea
                                className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[140px]"
                                value={profile.serviceOffer || ''}
                                onChange={(e) => setProfile({ ...profile, serviceOffer: e.target.value })}
                                placeholder={t('profile.serviceOfferPlaceholder')}
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Heart size={10}/> {t('profile.needs')}</label>
                            <textarea
                                className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[140px]"
                                value={profile.needs || ''}
                                onChange={(e) => setProfile({ ...profile, needs: e.target.value })}
                                placeholder={t('profile.needsPlaceholder')}
                            />
                        </div>
                    </div>
                </Card>
            </div>

            <div className="pt-6 flex flex-col items-center gap-4">
                <Button onClick={handleSave} disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                    {isSaving ? <CheckCircle2 className="animate-pulse" size={18} /> : <Save size={18} />}
                    {isSaving ? t('profile.updating') : t('profile.updateProfile')}
                </Button>
                {error && <p className="text-sm font-bold text-rose-500 animate-bounce">{error}</p>}
                {success && <p className="text-sm font-bold text-emerald-500 flex items-center gap-2"><CheckCircle2 size={16}/> {success}</p>}
            </div>
        </div>
    );
}
