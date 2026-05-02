// src/components/ProfileSettings.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { Input } from './ui/Input';
import { PublicProfile, Address } from '../types';
import { useSession } from '../context/SessionContext';
import { normalizeCoordinates } from '../utils/geoUtils';
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
    const { protectAction } = useSession();
    const [profile, setProfile] = useState<PublicProfile | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');
    const [coordWarning, setCoordWarning] = useState('');

    useEffect(() => {
        async function fetchProfile() {
            try {
                logger.info("ProfileSettings: Fetching current profile.");
                const currentProfile = await invoke<PublicProfile>('get_user_profile');
                setProfile(currentProfile);
            } catch (e) {
                const msg = `Failed to load profile: ${e}`;
                logger.error(msg);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProfile();
    }, []);

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
            setCoordWarning('Invalid format. Please use "Latitude, Longitude"');
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
                await invoke('update_user_profile', { profile, password });
            });
            setSuccess("Profile updated!");
            logger.info("Profile updated successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            setError(`Failed to update profile: ${e}`);
        } finally {
            setIsSaving(false);
        }
    };

    const updateAddress = (field: keyof Address, value: string) => {
        setProfile(prev => {
            if (!prev) return null;
            const newAddress = { ...(prev.address || {}), [field]: value };
            const parts = [newAddress.street, newAddress.house_number, newAddress.zip_code, newAddress.city, newAddress.country].filter(Boolean);
            newAddress.full_address = parts.join(', ');
            return { ...prev, address: newAddress };
        });
    };

    if (isLoading) return <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">Loading Profile...</div>;
    if (!profile) return <div className="p-8 text-center text-rose-500 font-bold">Profile not found.</div>;

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500 pb-20">
            {/* Privacy Shield */}
            <div className="bg-emerald-500/10 border border-emerald-500/20 p-6 rounded-[32px] flex items-start gap-5 shadow-sm">
                <div className="p-3 bg-white rounded-2xl shadow-sm text-emerald-600">
                    <Lock size={24} />
                </div>
                <div>
                    <h3 className="text-sm font-black text-emerald-900 uppercase tracking-widest mb-1">Vault Security</h3>
                    <p className="text-xs text-emerald-800/80 font-medium leading-relaxed">
                        Your identity is stored encrypted on your local device. It is only shared with counterparties when you explicitly sign a voucher.
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
                {/* Basic ID */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <User size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">Profile Details</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    First Name
                                    <span className="text-theme-accent text-[8px]">(Required)</span>
                                </label>
                                <Input
                                    value={profile.first_name || ''}
                                    onChange={(e) => setProfile({ ...profile, first_name: e.target.value })}
                                    placeholder="e.g. Alice"
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    Last Name
                                    <span className="text-theme-accent text-[8px]">(Required)</span>
                                </label>
                                <Input
                                    value={profile.last_name || ''}
                                    onChange={(e) => setProfile({ ...profile, last_name: e.target.value })}
                                    placeholder="e.g. Smith"
                                />
                            </div>
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Gender</label>
                            <select
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                value={profile.gender || ''}
                                onChange={(e) => setProfile({ ...profile, gender: e.target.value })}
                            >
                                <option value="">Select...</option>
                                <option value="1">Male</option>
                                <option value="2">Female</option>
                                <option value="0">Other / Not Declared</option>
                                <option value="9">Not Applicable</option>
                            </select>
                        </div>
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Organization / Company</label>
                                <Input
                                    value={profile.organization || ''}
                                    onChange={(e) => setProfile({ ...profile, organization: e.target.value })}
                                    placeholder="Organization Name"
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Community</label>
                                <Input
                                    value={profile.community || ''}
                                    onChange={(e) => setProfile({ ...profile, community: e.target.value })}
                                    placeholder="Community Name"
                                />
                            </div>
                        </div>
                    </div>
                </Card>

                {/* Location */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <MapPin size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">Location</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="grid grid-cols-4 gap-4">
                            <div className="col-span-3 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Street</label>
                                <Input
                                    value={profile.address?.street || ''}
                                    onChange={(e) => updateAddress('street', e.target.value)}
                                    placeholder="Street Address"
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Nr.</label>
                                <Input
                                    value={profile.address?.house_number || ''}
                                    onChange={(e) => updateAddress('house_number', e.target.value)}
                                    placeholder="123"
                                />
                            </div>
                        </div>
                        <div className="grid grid-cols-3 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">ZIP</label>
                                <Input
                                    value={profile.address?.zip_code || ''}
                                    onChange={(e) => updateAddress('zip_code', e.target.value)}
                                    placeholder="12345"
                                />
                            </div>
                            <div className="col-span-2 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                    City
                                    <span className="text-theme-accent text-[8px]">(Required)</span>
                                </label>
                                <Input
                                    value={profile.address?.city || ''}
                                    onChange={(e) => updateAddress('city', e.target.value)}
                                    placeholder="City Name"
                                />
                            </div>
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Country</label>
                            <Input
                                value={profile.address?.country || ''}
                                onChange={(e) => updateAddress('country', e.target.value)}
                                placeholder="Country Name"
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Map Coordinates (Lat, Long)</label>
                            <Input
                                value={profile.coordinates || ''}
                                onChange={(e) => {
                                    setProfile({ ...profile, coordinates: e.target.value });
                                    if (coordWarning) setCoordWarning('');
                                }}
                                onBlur={handleCoordBlur}
                                className={coordWarning ? 'border-rose-500 focus:ring-rose-500' : ''}
                                placeholder="51.16, 10.45"
                            />
                            {coordWarning && <p className="text-[10px] text-rose-500 font-bold">{coordWarning}</p>}
                        </div>
                    </div>
                </Card>

                {/* Social & Web */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <Globe size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">Contact & Links</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Mail size={10}/> Public Email</label>
                            <Input
                                type="email"
                                value={profile.email || ''}
                                onChange={(e) => setProfile({ ...profile, email: e.target.value })}
                                placeholder="identity@domain.com"
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Phone size={10}/> Public Phone</label>
                            <Input
                                type="tel"
                                value={profile.phone || ''}
                                onChange={(e) => setProfile({ ...profile, phone: e.target.value })}
                                placeholder="+49 000 000000"
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Globe size={10}/> Website</label>
                            <Input
                                value={profile.url || ''}
                                onChange={(e) => setProfile({ ...profile, url: e.target.value })}
                                placeholder="https://profile.com"
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><User size={10}/> Profile Picture URL</label>
                            <Input
                                value={profile.picture_url || ''}
                                onChange={(e) => setProfile({ ...profile, picture_url: e.target.value })}
                                placeholder="https://domain.com/avatar.jpg"
                            />
                        </div>
                    </div>
                </Card>

                {/* Offerings */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <Briefcase size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">Community</span>
                    </div>
                }>
                    <div className="space-y-6 p-2">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Briefcase size={10}/> Service Offer (I can help with)</label>
                            <textarea
                                className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[120px]"
                                value={profile.service_offer || ''}
                                onChange={(e) => setProfile({ ...profile, service_offer: e.target.value })}
                                placeholder="Describe what you bring to the network..."
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Heart size={10}/> Needs (I'm looking for)</label>
                            <textarea
                                className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[120px]"
                                value={profile.needs || ''}
                                onChange={(e) => setProfile({ ...profile, needs: e.target.value })}
                                placeholder="Describe what you are looking for..."
                            />
                        </div>
                    </div>
                </Card>
            </div>

            <div className="pt-6 flex flex-col items-center gap-4">
                <Button onClick={handleSave} disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                    {isSaving ? <CheckCircle2 className="animate-pulse" size={18} /> : <Save size={18} />}
                    {isSaving ? 'Updating Profile...' : 'Update Profile'}
                </Button>
                {error && <p className="text-sm font-bold text-rose-500 animate-bounce">{error}</p>}
                {success && <p className="text-sm font-bold text-emerald-500 flex items-center gap-2"><CheckCircle2 size={16}/> {success}</p>}
            </div>
        </div>
    );
}
