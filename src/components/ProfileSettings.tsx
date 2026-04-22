// src/components/ProfileSettings.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { PublicProfile, Address } from '../types';
import { useSession } from '../context/SessionContext';
import { normalizeCoordinates } from '../utils/geoUtils';

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
            setCoordWarning('Invalid format. Please use "Latitude, Longitude" (e.g. 51.16, 10.45)');
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
            setSuccess("Profile updated successfully!");
            logger.info("Profile updated successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            const msg = `Failed to update profile: ${e}`;
            logger.error(msg);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

    const updateAddress = (field: keyof Address, value: string) => {
        setProfile(prev => {
            if (!prev) return null;
            const newAddress = { ...(prev.address || {}), [field]: value };
            
            // Auto-generate full address
            const parts = [
                newAddress.street,
                newAddress.house_number,
                newAddress.zip_code,
                newAddress.city,
                newAddress.country
            ].filter(Boolean);
            newAddress.full_address = parts.join(', ');
            
            return { ...prev, address: newAddress };
        });
    };

    if (isLoading) {
        return <p className="text-center py-4">Loading profile data...</p>;
    }

    if (!profile) {
        return <p className="text-red-500 text-center py-4">Error: Profile not loaded. {error}</p>;
    }

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Privacy Guarantee Info Box */}
            <div className="bg-green-50 border-l-4 border-green-500 p-4 rounded-md shadow-sm">
                <div className="flex items-start gap-3">
                    <div className="flex-shrink-0 text-green-600 text-2xl">🔒</div>
                    <div>
                        <p className="text-sm font-semibold text-green-800">
                            Privacy Guarantee
                        </p>
                        <p className="text-xs text-green-700 mt-1">
                            Your profile data is securely encrypted and never leaves your local device. It is only shared when you explicitly sign or create a voucher.
                        </p>
                    </div>
                </div>
            </div>

            <section className="bg-bg-card border border-theme-subtle rounded-xl overflow-hidden shadow-lg">
                <div className="bg-theme-primary/5 px-6 py-4 border-b border-theme-subtle">
                    <h2 className="text-lg font-semibold text-theme-primary">Identity & Role</h2>
                    <p className="text-xs text-theme-light">Basic identification data for your wallet.</p>
                </div>
                <div className="p-6 grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">First Name</label>
                        <Input
                            value={profile.first_name || ''}
                            onChange={(e) => setProfile({ ...profile, first_name: e.target.value })}
                            placeholder="John"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Last Name</label>
                        <Input
                            value={profile.last_name || ''}
                            onChange={(e) => setProfile({ ...profile, last_name: e.target.value })}
                            placeholder="Doe"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Gender</label>
                        <select
                            className="w-full bg-bg-input border border-theme-subtle rounded-lg px-3 py-2 text-theme-main focus:ring-2 focus:ring-theme-primary outline-none"
                            value={profile.gender || ''}
                            onChange={(e) => setProfile({ ...profile, gender: e.target.value })}
                        >
                            <option value="">Select...</option>
                            <option value="1">Male</option>
                            <option value="2">Female</option>
                            <option value="0">Other/Not Known</option>
                            <option value="9">Not Applicable</option>
                        </select>
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Organization</label>
                        <Input
                            value={profile.organization || ''}
                            onChange={(e) => setProfile({ ...profile, organization: e.target.value })}
                            placeholder="My Co-op"
                        />
                    </div>
                    <div className="md:col-span-2">
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Community / Group</label>
                        <Input
                            value={profile.community || ''}
                            onChange={(e) => setProfile({ ...profile, community: e.target.value })}
                            placeholder="Local Exchange Network"
                        />
                    </div>
                </div>
            </section>

            <section className="bg-bg-card border border-theme-subtle rounded-xl overflow-hidden shadow-lg">
                <div className="bg-theme-secondary/5 px-6 py-4 border-b border-theme-subtle">
                    <h2 className="text-lg font-semibold text-theme-secondary">Service Offers & Needs</h2>
                    <p className="text-xs text-theme-light">Describe what you offer and what you are looking for.</p>
                </div>
                <div className="p-6 space-y-6">
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Service Offers</label>
                        <textarea
                            className="w-full bg-bg-input border border-theme-subtle rounded-lg px-3 py-2 text-theme-main focus:ring-2 focus:ring-theme-primary outline-none min-h-[100px]"
                            value={profile.service_offer || ''}
                            onChange={(e) => setProfile({ ...profile, service_offer: e.target.value })}
                            placeholder="List the services or goods you provide..."
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Current Needs</label>
                        <textarea
                            className="w-full bg-bg-input border border-theme-subtle rounded-lg px-3 py-2 text-theme-main focus:ring-2 focus:ring-theme-primary outline-none min-h-[100px]"
                            value={profile.needs || ''}
                            onChange={(e) => setProfile({ ...profile, needs: e.target.value })}
                            placeholder="List what you currently need or are looking for..."
                        />
                    </div>
                </div>
            </section>

            <section className="bg-bg-card border border-theme-subtle rounded-xl overflow-hidden shadow-lg">
                <div className="bg-theme-accent/5 px-6 py-4 border-b border-theme-subtle">
                    <h2 className="text-lg font-semibold text-theme-accent">Contact & Social</h2>
                    <p className="text-xs text-theme-light">Public contact information.</p>
                </div>
                <div className="p-6 grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Email</label>
                        <Input
                            type="email"
                            value={profile.email || ''}
                            onChange={(e) => setProfile({ ...profile, email: e.target.value })}
                            placeholder="john@example.com"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Phone</label>
                        <Input
                            type="tel"
                            value={profile.phone || ''}
                            onChange={(e) => setProfile({ ...profile, phone: e.target.value })}
                            placeholder="+49 123 456789"
                        />
                    </div>
                    <div className="md:col-span-2">
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Website / URL</label>
                        <Input
                            value={profile.url || ''}
                            onChange={(e) => setProfile({ ...profile, url: e.target.value })}
                            placeholder="https://example.com"
                        />
                    </div>
                </div>
            </section>

            <section className="bg-bg-card border border-theme-subtle rounded-xl overflow-hidden shadow-lg">
                <div className="bg-theme-main/5 px-6 py-4 border-b border-theme-subtle">
                    <h2 className="text-lg font-semibold text-theme-main">Location</h2>
                    <p className="text-xs text-theme-light">Physical address for voucher verification.</p>
                </div>
                <div className="p-6 space-y-6">
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div className="md:col-span-2">
                            <label className="block text-sm font-medium text-theme-light mb-1.5">Street</label>
                            <Input
                                value={profile.address?.street || ''}
                                onChange={(e) => updateAddress('street', e.target.value)}
                                placeholder="Main St"
                            />
                        </div>
                        <div>
                            <label className="block text-sm font-medium text-theme-light mb-1.5">House No.</label>
                            <Input
                                value={profile.address?.house_number || ''}
                                onChange={(e) => updateAddress('house_number', e.target.value)}
                                placeholder="123"
                            />
                        </div>
                    </div>
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div>
                            <label className="block text-sm font-medium text-theme-light mb-1.5">ZIP Code</label>
                            <Input
                                value={profile.address?.zip_code || ''}
                                onChange={(e) => updateAddress('zip_code', e.target.value)}
                                placeholder="12345"
                            />
                        </div>
                        <div>
                            <label className="block text-sm font-medium text-theme-light mb-1.5">City</label>
                            <Input
                                value={profile.address?.city || ''}
                                onChange={(e) => updateAddress('city', e.target.value)}
                                placeholder="Metropolis"
                            />
                        </div>
                        <div>
                            <label className="block text-sm font-medium text-theme-light mb-1.5">Country</label>
                            <Input
                                value={profile.address?.country || ''}
                                onChange={(e) => updateAddress('country', e.target.value)}
                                placeholder="Earth"
                            />
                        </div>
                    </div>
                    <div>
                        <label className="block text-sm font-medium text-theme-light mb-1.5">Coordinates (Latitude, Longitude)</label>
                        <Input
                            value={profile.coordinates || ''}
                            onChange={(e) => {
                                setProfile({ ...profile, coordinates: e.target.value });
                                if (coordWarning) setCoordWarning(''); // Clear warning while typing
                            }}
                            onBlur={handleCoordBlur}
                            className={coordWarning ? 'border-red-500 focus:ring-red-500' : ''}
                            placeholder="e.g. 51.16, 10.45"
                        />
                        {coordWarning ? (
                            <p className="text-[10px] text-red-500 mt-1 font-medium">{coordWarning}</p>
                        ) : (
                            <p className="text-[10px] text-theme-light mt-1 italic">Format: Decimal degrees. Used for map-based voucher discovery.</p>
                        )}
                    </div>
                </div>
            </section>

            <div className="flex flex-col items-center gap-4 py-8">
                <Button 
                    variant="primary" 
                    size="lg" 
                    onClick={handleSave} 
                    disabled={isSaving}
                    className="min-w-[200px] shadow-lg shadow-theme-primary/20"
                >
                    {isSaving ? 'Updating Profile...' : 'Save Profile Changes'}
                </Button>
                {error && <p className="text-sm text-red-500 animate-bounce">{error}</p>}
                {success && <p className="text-sm text-green-500 font-medium">{success}</p>}
            </div>
        </div>
    );
}
