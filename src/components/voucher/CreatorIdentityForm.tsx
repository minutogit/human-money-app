// src/components/voucher/CreatorIdentityForm.tsx

import { UserCircle, Mail, Phone, Globe, Briefcase, Heart } from "lucide-react";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { CreatorData } from "../../types";

interface CreatorIdentityFormProps {
    identity: CreatorData;
    onIdentityChange: (identity: CreatorData) => void;
    onLoadProfile: () => void;
    onCoordBlur: () => void;
    coordWarning: string;
    errors: Record<string, boolean>;
    firstNameRef: React.RefObject<HTMLInputElement | null>;
    lastNameRef: React.RefObject<HTMLInputElement | null>;
}

export function CreatorIdentityForm({
    identity,
    onIdentityChange,
    onLoadProfile,
    onCoordBlur,
    coordWarning,
    errors,
    firstNameRef,
    lastNameRef
}: CreatorIdentityFormProps) {
    const handleChange = (field: string, value: any) => {
        onIdentityChange({ ...identity, [field]: value });
    };

    const handleAddressChange = (field: string, value: string) => {
        onIdentityChange({
            ...identity,
            address: { ...identity.address, [field]: value }
        });
    };

    return (
        <Card header={
            <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                    <UserCircle size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Your Details</span>
                </div>
                <Button type="button" variant="secondary" size="sm" onClick={onLoadProfile} className="rounded-xl px-3 h-8 text-[9px] uppercase tracking-widest">
                    Sync from Profile
                </Button>
            </div>
        }>
            <div className="space-y-8">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div className="space-y-4">
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label htmlFor="creator-first-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">First Name (Required)</label>
                                <Input id="creator-first-name" ref={firstNameRef} value={identity.firstName} onChange={(e) => handleChange('firstName', e.target.value)} className={errors.firstName ? 'border-rose-500' : ''} />
                            </div>
                            <div className="space-y-2">
                                <label htmlFor="creator-last-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Last Name (Required)</label>
                                <Input id="creator-last-name" ref={lastNameRef} value={identity.lastName} onChange={(e) => handleChange('lastName', e.target.value)} className={errors.lastName ? 'border-rose-500' : ''} />
                            </div>
                        </div>
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Community</label>
                                <Input value={identity.community || ""} onChange={(e) => handleChange('community', e.target.value)} placeholder="Optional" />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Organization / Company</label>
                                <Input value={identity.organization || ""} onChange={(e) => handleChange('organization', e.target.value)} placeholder="Optional" />
                            </div>
                        </div>
                        <div className="grid grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Gender</label>
                                <select value={identity.gender} onChange={(e) => handleChange('gender', e.target.value)} className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-3 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none">
                                    <option value="1">Male</option>
                                    <option value="2">Female</option>
                                    <option value="0">Not Known</option>
                                    <option value="9">Not Applicable</option>
                                </select>
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Coordinates</label>
                                <Input value={identity.coordinates} placeholder="51.16, 10.45" onChange={(e) => handleChange('coordinates', e.target.value)} onBlur={onCoordBlur} className={coordWarning ? 'border-rose-500' : ''} />
                            </div>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="grid grid-cols-4 gap-4">
                            <div className="col-span-3 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Street</label>
                                <Input value={identity.address.street || ""} onChange={(e) => handleAddressChange('street', e.target.value)} />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Nr.</label>
                                <Input value={identity.address.houseNumber || ""} onChange={(e) => handleAddressChange('houseNumber', e.target.value)} />
                            </div>
                        </div>
                        <div className="grid grid-cols-3 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">ZIP</label>
                                <Input value={identity.address.zipCode || ""} onChange={(e) => handleAddressChange('zipCode', e.target.value)} />
                            </div>
                            <div className="col-span-2 space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">City</label>
                                <Input value={identity.address.city || ""} onChange={(e) => handleAddressChange('city', e.target.value)} />
                            </div>
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Country</label>
                            <Input value={identity.address.country || ""} onChange={(e) => handleAddressChange('country', e.target.value)} />
                        </div>
                    </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-6 pt-4 border-t border-theme-subtle/40">
                    <div className="space-y-6">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Mail size={10}/> Public Email</label>
                            <Input type="email" value={identity.email || ""} onChange={(e) => handleChange('email', e.target.value)} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Phone size={10}/> Public Phone</label>
                            <Input type="tel" value={identity.phone || ""} onChange={(e) => handleChange('phone', e.target.value)} />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Globe size={10}/> Website</label>
                            <Input value={identity.url || ""} onChange={(e) => handleChange('url', e.target.value)} placeholder="https://..." />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><UserCircle size={10}/> Profile Picture URL</label>
                            <Input value={identity.pictureUrl || ""} onChange={(e) => handleChange('pictureUrl', e.target.value)} placeholder="https://domain.com/avatar.jpg" />
                        </div>
                    </div>
                    <div className="space-y-6">
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Briefcase size={10}/> Service Offer (I can help with)</label>
                            <textarea value={identity.serviceOffer || ""} onChange={(e) => handleChange('serviceOffer', e.target.value)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[80px]" placeholder="What do you offer?" />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Heart size={10}/> Needs (I'm looking for)</label>
                            <textarea value={identity.needs || ""} onChange={(e) => handleChange('needs', e.target.value)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[80px]" placeholder="What do you need?" />
                        </div>
                    </div>
                </div>
            </div>
        </Card>
    );
}
