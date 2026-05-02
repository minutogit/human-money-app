// src/components/CreateVoucher.tsx
import { useState, useEffect, FormEvent, ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { Input } from "./ui/Input";
import { NewVoucherData, VoucherStandardInfo, PublicProfile } from "../types";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { normalizeCoordinates } from "../utils/geoUtils";
import { PageLayout } from "./ui/PageLayout";
import { 
    Coins, 
    ShieldAlert, 
    Briefcase, 
    Info, 
    UserCircle,
    CheckCircle2,
    Clock,
    AlertCircle,
    Lock, 
    Mail, 
    Phone, 
    PlusCircle,
    Heart,
    Globe
} from "lucide-react";

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

function parseStandardInfo(tomlContent: string) {
    const nameMatch = tomlContent.match(/\bname\s*=\s*"([^"]+)"/);
    const name = nameMatch ? nameMatch[1] : null;
    const issuerMatch = tomlContent.match(/issuer_name\s*=\s*"([^"]+)"/);
    const issuer = issuerMatch ? issuerMatch[1] : null;
    const unitMatch = tomlContent.match(/unit\s*=\s*"([^"]+)"/);
    const unit = unitMatch ? unitMatch[1] : null;
    const abbreviationMatch = tomlContent.match(/abbreviation\s*=\s*"([^"]+)"/);
    const abbreviation = abbreviationMatch ? abbreviationMatch[1] : null;
    const durationMatch = tomlContent.match(/default_validity_duration\s*=\s*"P(\d+)([YMD])"/);
    let validity = null;
    if (durationMatch) {
        validity = { value: parseInt(durationMatch[1], 10), unit: durationMatch[2] as "Y" | "M" | "D" };
    }
    return { name, issuer, unit, abbreviation, validity };
}

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    const { protectAction } = useSession();
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string>("");
    const [amount, setAmount] = useState("");
    const [validityValue, setValidityValue] = useState<number>(3);
    const [validityUnit, setValidityUnit] = useState<"Y" | "M" | "D">("Y");
    const [nonRedeemable, setNonRedeemable] = useState(true);

    const [firstName, setFirstName] = useState("");
    const [lastName, setLastName] = useState("");
    const [street, setStreet] = useState("");
    const [houseNumber, setHouseNumber] = useState("");
    const [zipCode, setZipCode] = useState("");
    const [city, setCity] = useState("");
    const [country, setCountry] = useState("");
    const [gender, setGender] = useState("0");
    const [coordinates, setCoordinates] = useState("");
    const [email, setEmail] = useState("");
    const [phone, setPhone] = useState("");
    const [url, setUrl] = useState("");
    const [organization, setOrganization] = useState("");
    const [community, setCommunity] = useState("");
    const [serviceOffer, setServiceOffer] = useState("");
    const [needs, setNeeds] = useState("");
    const [pictureUrl, setPictureUrl] = useState("");
    const [coordWarning, setCoordWarning] = useState("");

    const [collateralAmount, setCollateralAmount] = useState("");
    const [collateralUnit, setCollateralUnit] = useState("");
    const [collateralAbbreviation, setCollateralAbbreviation] = useState("");

    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);
    const [showConfirm, setShowConfirm] = useState(false);
    const [showTestVoucherWarning, setShowTestVoucherWarning] = useState(false);
    
    // Validierungs-Status
    const [errors, setErrors] = useState<Record<string, boolean>>({});

    useEffect(() => {
        async function fetchStandards() {
            try {
                const fetchedStandards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setStandards(fetchedStandards);
            } catch (e) {
                setFeedback({ type: 'error', msg: `Failed to fetch standards: ${e}` });
            } finally {
                setIsLoading(false);
            }
        }
        fetchStandards();
    }, []);

    const handleStandardChange = (e: ChangeEvent<HTMLSelectElement>) => {
        const newId = e.target.value;
        setSelectedStandardId(newId);
        setErrors(prev => ({ ...prev, standard: false }));
        const selectedStd = standards.find(s => s.id === newId);
        if (selectedStd) {
            const { validity } = parseStandardInfo(selectedStd.content);
            if (validity) {
                setValidityValue(validity.value);
                setValidityUnit(validity.unit);
            }
        }
    };

    const handleCreateClick = (event: FormEvent) => {
        event.preventDefault();
        if (isLoading) return;
        
        const newErrors: Record<string, boolean> = {};
        if (!selectedStandardId) newErrors.standard = true;
        if (!amount) newErrors.amount = true;
        if (!firstName) newErrors.firstName = true;
        if (!lastName) newErrors.lastName = true;

        if (Object.keys(newErrors).length > 0) {
            setErrors(newErrors);
            return;
        }

        setShowConfirm(true);
    };

    const handleLoadProfile = async () => {
        try {
            const profile = await invoke<PublicProfile>('get_user_profile');
            if (profile.first_name) setFirstName(profile.first_name);
            if (profile.last_name) setLastName(profile.last_name);
            if (profile.organization) setOrganization(profile.organization);
            if (profile.community) setCommunity(profile.community);
            if (profile.gender) setGender(profile.gender);
            if (profile.email) setEmail(profile.email);
            if (profile.phone) setPhone(profile.phone);
            if (profile.url) setUrl(profile.url);
            if (profile.coordinates) setCoordinates(profile.coordinates);
            if (profile.service_offer) setServiceOffer(profile.service_offer);
            if (profile.needs) setNeeds(profile.needs);
            if (profile.picture_url) setPictureUrl(profile.picture_url);
            if (profile.address) {
                if (profile.address.street) setStreet(profile.address.street);
                if (profile.address.house_number) setHouseNumber(profile.address.house_number);
                if (profile.address.zip_code) setZipCode(profile.address.zip_code);
                if (profile.address.city) setCity(profile.address.city);
                if (profile.address.country) setCountry(profile.address.country);
            }
            logger.info("CreateVoucher: Profile data synchronized.");
        } catch (e) {
            logger.error(`Failed to load profile: ${e}`);
        }
    };

    const handleCoordBlur = () => {
        if (!coordinates) {
            setCoordWarning("");
            return;
        }
        const normalized = normalizeCoordinates(coordinates);
        if (normalized) {
            setCoordinates(normalized);
            setCoordWarning("");
        } else {
            setCoordWarning("Invalid GPS coordinate format.");
        }
    };

    async function executeCreation() {
        setIsLoading(true);
        const selectedStandard = standards.find(s => s.id === selectedStandardId)!;
        const { unit: standardUnit } = parseStandardInfo(selectedStandard.content);
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();
        const voucherData: NewVoucherData = {
            validity_duration: validityDuration,
            non_redeemable_test_voucher: nonRedeemable,
            nominal_value: { amount, unit: standardUnit || "Units" },
            collateral: { amount: collateralAmount, unit: collateralUnit, abbreviation: collateralAbbreviation },
            creator: {
                first_name: firstName,
                last_name: lastName,
                address: { street, house_number: houseNumber, zip_code: zipCode, city, country, full_address: fullAddress },
                gender,
                coordinates,
                email: email || undefined,
                phone: phone || undefined,
                url: url || undefined,
                organization: organization || undefined,
                community: community || undefined,
                service_offer: serviceOffer || undefined,
                needs: needs || undefined,
                picture_url: pictureUrl || undefined,
            },
        };
        try {
            await protectAction(async (password) => {
                await invoke("create_new_voucher", { standardTomlContent: selectedStandard.content, data: voucherData, password });
            });
            setFeedback({type: 'success', msg: "Voucher created! Synchronizing ledger..."});
            setTimeout(onVoucherCreated, 2000);
        } catch (e) {
            setFeedback({type: 'error', msg: `Creation Failed: ${e}`});
            setIsLoading(false);
            setShowConfirm(false);
        }
    }

    return (
        <PageLayout 
            title="Create Voucher" 
            description="Create a new voucher." 
            onBack={onCancel}
        >
            <div className="max-w-5xl mx-auto space-y-8 pb-20">
                {feedback && (
                    <div className={`p-5 rounded-3xl flex items-start gap-3 border shadow-sm animate-in zoom-in duration-300 ${feedback.type === 'error' ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                        {feedback.type === 'error' ? <ShieldAlert className="shrink-0" size={20}/> : <CheckCircle2 className="shrink-0" size={20}/>}
                        <p className="text-sm font-bold leading-tight">{feedback.msg}</p>
                    </div>
                )}

                <form onSubmit={handleCreateClick} className="space-y-8">
                    {/* Basic Info Card */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <Coins size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Voucher Details</span>
                        </div>
                    }>
                        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                            <div className="lg:col-span-2 space-y-6">
                                <div className="space-y-2">
                                    <label htmlFor="voucher-type" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Voucher Type</label>
                                    <select 
                                        id="voucher-type"
                                        value={selectedStandardId} 
                                        onChange={handleStandardChange} 
                                        disabled={isLoading || standards.length === 0} 
                                        className={`w-full bg-white border rounded-2xl px-4 py-3.5 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all ${errors.standard ? 'border-rose-500' : 'border-theme-subtle'}`}
                                    >
                                        <option value="">Select Voucher Type...</option>
                                        {standards.map(s => {
                                            const { name, issuer } = parseStandardInfo(s.content);
                                            return <option key={s.id} value={s.id}>{name || s.id} {issuer ? `(${issuer})` : ''}</option>;
                                        })}
                                    </select>
                                </div>
                                <div className="grid grid-cols-2 gap-4">
                                    <div className="space-y-2">
                                        <label htmlFor="voucher-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Amount (e.g., 60)</label>
                                        <Input 
                                            id="voucher-amount"
                                            type="number" 
                                            value={amount} 
                                            onChange={(e) => { setAmount(e.target.value); setErrors(prev => ({ ...prev, amount: false })); }} 
                                            placeholder="0.00"
                                            className={`font-black text-lg py-4 ${errors.amount ? 'border-rose-500' : ''}`}
                                        />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Validity Duration</label>
                                        <div className="flex gap-2">
                                            <Input 
                                                type="number" 
                                                value={validityValue} 
                                                onChange={(e) => setValidityValue(parseInt(e.target.value, 10) || 0)}
                                                className="w-1/2 font-bold"
                                            />
                                            <select 
                                                value={validityUnit} 
                                                onChange={(e) => setValidityUnit(e.target.value as "Y" | "M" | "D")}
                                                className="w-1/2 bg-white border border-theme-subtle rounded-xl px-3 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                            >
                                                <option value="Y">Years</option>
                                                <option value="M">Months</option>
                                                <option value="D">Days</option>
                                            </select>
                                        </div>
                                    </div>
                                </div>
                            </div>
                            
                            <div className="flex flex-col justify-center">
                                <label className={`relative flex flex-col p-6 rounded-[32px] border-2 transition-all cursor-pointer group shadow-sm ${nonRedeemable ? 'bg-emerald-500/5 border-emerald-500 shadow-emerald-100' : 'bg-rose-500/5 border-rose-500 shadow-rose-100'}`}>
                                    <input 
                                        type="checkbox" 
                                        checked={nonRedeemable} 
                                        onChange={(e) => {
                                            if (e.target.checked) setNonRedeemable(true);
                                            else setShowTestVoucherWarning(true);
                                        }}
                                        className="absolute opacity-0"
                                    />
                                    <div className={`mb-3 p-3 rounded-2xl w-fit ${nonRedeemable ? 'bg-emerald-500 text-white' : 'bg-rose-500 text-white'}`}>
                                        {nonRedeemable ? <CheckCircle2 size={24} /> : <AlertCircle size={24} />}
                                    </div>
                                    <span className={`text-lg font-black tracking-tight ${nonRedeemable ? 'text-emerald-900' : 'text-rose-900'}`}>
                                        {nonRedeemable ? 'Test Voucher' : 'Real Voucher'}
                                    </span>
                                    <span className={`text-[10px] font-bold mt-1 ${nonRedeemable ? 'text-emerald-700/60' : 'text-rose-700/60'}`}>
                                        {nonRedeemable ? 'Test Voucher' : 'Standard obligation'}
                                    </span>
                                </label>
                            </div>
                        </div>
                    </Card>

                    {/* Creator Identity Card */}
                    <Card header={
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                                <UserCircle size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Your Details</span>
                            </div>
                            <Button type="button" variant="secondary" size="sm" onClick={handleLoadProfile} className="rounded-xl px-3 h-8 text-[9px] uppercase tracking-widest">
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
                                            <Input id="creator-first-name" value={firstName} onChange={(e) => { setFirstName(e.target.value); setErrors(prev => ({ ...prev, firstName: false })); }} className={errors.firstName ? 'border-rose-500' : ''} />
                                        </div>
                                        <div className="space-y-2">
                                            <label htmlFor="creator-last-name" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Last Name (Required)</label>
                                            <Input id="creator-last-name" value={lastName} onChange={(e) => { setLastName(e.target.value); setErrors(prev => ({ ...prev, lastName: false })); }} className={errors.lastName ? 'border-rose-500' : ''} />
                                        </div>
                                    </div>
                                    <div className="grid grid-cols-2 gap-4">
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Community</label>
                                            <Input value={community} onChange={(e) => setCommunity(e.target.value)} placeholder="Optional" />
                                        </div>
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Organization / Company</label>
                                            <Input value={organization} onChange={(e) => setOrganization(e.target.value)} placeholder="Optional" />
                                        </div>
                                    </div>
                                    <div className="grid grid-cols-2 gap-4">
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Gender</label>
                                            <select value={gender} onChange={(e) => setGender(e.target.value)} className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-3 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none">
                                                <option value="1">Male</option>
                                                <option value="2">Female</option>
                                                <option value="0">Not Known</option>
                                                <option value="9">Not Applicable</option>
                                            </select>
                                        </div>
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Coordinates</label>
                                            <Input value={coordinates} placeholder="51.16, 10.45" onChange={(e) => { setCoordinates(e.target.value); if(coordWarning) setCoordWarning("");}} onBlur={handleCoordBlur} className={coordWarning ? 'border-rose-500' : ''} />
                                        </div>
                                    </div>
                                </div>

                                <div className="space-y-4">
                                    <div className="grid grid-cols-4 gap-4">
                                        <div className="col-span-3 space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Street</label>
                                            <Input value={street} onChange={(e) => setStreet(e.target.value)} />
                                        </div>
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Nr.</label>
                                            <Input value={houseNumber} onChange={(e) => setHouseNumber(e.target.value)} />
                                        </div>
                                    </div>
                                    <div className="grid grid-cols-3 gap-4">
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">ZIP</label>
                                            <Input value={zipCode} onChange={(e) => setZipCode(e.target.value)} />
                                        </div>
                                        <div className="col-span-2 space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">City</label>
                                            <Input value={city} onChange={(e) => setCity(e.target.value)} />
                                        </div>
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Country</label>
                                        <Input value={country} onChange={(e) => setCountry(e.target.value)} />
                                    </div>
                                </div>
                            </div>

                            <div className="grid grid-cols-1 md:grid-cols-2 gap-6 pt-4 border-t border-theme-subtle/40">
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Mail size={10}/> Public Email</label>
                                        <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Phone size={10}/> Public Phone</label>
                                        <Input type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Globe size={10}/> Website</label>
                                        <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://..." />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><UserCircle size={10}/> Profile Picture URL</label>
                                        <Input value={pictureUrl} onChange={(e) => setPictureUrl(e.target.value)} placeholder="https://domain.com/avatar.jpg" />
                                    </div>
                                </div>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Briefcase size={10}/> Service Offer (I can help with)</label>
                                        <textarea value={serviceOffer} onChange={(e) => setServiceOffer(e.target.value)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[80px]" placeholder="What do you offer?" />
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Heart size={10}/> Needs (I'm looking for)</label>
                                        <textarea value={needs} onChange={(e) => setNeeds(e.target.value)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-xs font-medium text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all min-h-[80px]" placeholder="What do you need?" />
                                    </div>
                                </div>
                            </div>
                        </div>
                    </Card>

                    {/* Collateral Card */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <ShieldAlert size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Collateral (Optional)</span>
                        </div>
                    }>
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Collateral Amount</label>
                                <Input type="number" value={collateralAmount} onChange={(e) => setCollateralAmount(e.target.value)} />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Unit</label>
                                <Input value={collateralUnit} onChange={(e) => setCollateralUnit(e.target.value)} placeholder="e.g. Euro" />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Abbr.</label>
                                <Input value={collateralAbbreviation} onChange={(e) => setCollateralAbbreviation(e.target.value)} placeholder="e.g. EUR" />
                            </div>
                        </div>
                    </Card>

                    {/* Privacy Guarantee */}
                    <div className="bg-amber-500/10 border border-amber-500/20 p-6 rounded-[32px] flex items-start gap-5 shadow-sm">
                        <div className="p-3 bg-white rounded-2xl shadow-sm text-amber-600">
                            <Info size={24} />
                        </div>
                        <div>
                            <h3 className="text-sm font-black text-amber-900 uppercase tracking-widest mb-1">Public Data Notice</h3>
                            <p className="text-xs text-amber-800/80 font-medium leading-relaxed">
                                The information embedded in this voucher serves as your cryptographic guarantee. All creator details will be permanently readable by anyone verifying this asset on the public ledger.
                            </p>
                        </div>
                    </div>

                    <div className="flex flex-col items-center gap-4 py-4">
                        <Button type="submit" disabled={isLoading} className="min-w-[320px] py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            {isLoading ? <Clock className="animate-spin" size={24} /> : <PlusCircle size={24} />}
                            {isLoading ? "Creating..." : "Create Voucher"}
                        </Button>
                            <p className="text-[10px] font-bold text-theme-light flex items-center gap-2">
                                <Lock size={12} /> You will sign this voucher in the next step.
                            </p>
                    </div>
                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title="Create Voucher?"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-6 bg-theme-primary/5 rounded-[32px] border border-theme-primary/20 text-center">
                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light mb-2">New Voucher</p>
                            <p className="text-3xl font-black text-theme-primary tracking-tighter">
                                {(() => {
                                    const selected = standards.find(s => s.id === selectedStandardId);
                                    if (!selected) return amount;
                                    const { abbreviation, unit } = parseStandardInfo(selected.content);
                                    let displayUnit = abbreviation || unit || 'Units';
                                    if (nonRedeemable && !displayUnit.startsWith("TEST-")) displayUnit = `TEST-${displayUnit}`;
                                    return `${amount} ${displayUnit}`;
                                })()}
                            </p>
                        </div>
                        <p className="text-sm font-medium text-theme-secondary leading-relaxed">
                            This action will cryptographically sign the asset with your private key. The voucher will be permanently attributed to your identity. Proceed?
                        </p>
                    </div>
                }
                confirmText="Create Voucher"
                onConfirm={executeCreation}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isLoading}
            />

            <ConfirmationModal
                isOpen={showTestVoucherWarning}
                title="Warning"
                confirmVariant="danger"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-5 bg-rose-50 border border-rose-100 rounded-3xl flex items-start gap-4">
                            <ShieldAlert size={24} className="text-rose-500 mt-1 shrink-0" />
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                You are about to issue a <strong>real redeemable voucher</strong>. In this early development phase, we recommend using Test Assets to prevent potential asset loss.
                            </p>
                        </div>
                        <p className="text-sm font-bold text-theme-secondary">Proceed with real cryptographic obligation?</p>
                    </div>
                }
                confirmText="Create Real Voucher"
                cancelText="Back to Safety"
                onConfirm={() => { setNonRedeemable(false); setShowTestVoucherWarning(false); }}
                onCancel={() => { setNonRedeemable(true); setShowTestVoucherWarning(false); }}
            />
        </PageLayout>
    );
}
