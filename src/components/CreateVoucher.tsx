// src/components/CreateVoucher.tsx
import { useState, useEffect, FormEvent, ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error, info } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { NewVoucherData, VoucherStandardInfo, PublicProfile } from "../types";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { normalizeCoordinates } from "../utils/geoUtils";
import { PageLayout } from "./ui/PageLayout";

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

// Hilfs-Komponente zur Strukturierung des Formulars
const Fieldset: React.FC<{ legend: string; children: React.ReactNode }> = ({ legend, children }) => (
    <fieldset className="rounded-lg border border-theme-subtle p-5 mb-6">
        <legend className="px-2 text-lg font-semibold text-theme-primary">{legend}</legend>
        <div className="space-y-4 pt-2">
            {children}
        </div>
    </fieldset>
);

// Helper: Extrahiert Metadaten aus dem TOML-Content
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
    const [coordWarning, setCoordWarning] = useState("");

    const [collateralAmount, setCollateralAmount] = useState("");
    const [collateralUnit, setCollateralUnit] = useState("");
    const [collateralAbbreviation, setCollateralAbbreviation] = useState("");

    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);
    const [showConfirm, setShowConfirm] = useState(false);
    const [showTestVoucherWarning, setShowTestVoucherWarning] = useState(false);
    const [highlightStandardSelect, setHighlightStandardSelect] = useState(false);
    const [highlightAmount, setHighlightAmount] = useState(false);
    const [highlightFirstName, setHighlightFirstName] = useState(false);
    const [highlightLastName, setHighlightLastName] = useState(false);

    useEffect(() => {
        logger.info("CreateVoucher component displayed");
        async function fetchStandards() {
            try {
                info("CreateVoucher: Fetching voucher standards.");
                const fetchedStandards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setStandards(fetchedStandards);
                info(`CreateVoucher: Loaded ${fetchedStandards.length} standards.`);
            } catch (e) {
                const msg = `Failed to fetch voucher standards: ${e}`;
                error(msg);
                setFeedback({ type: 'error', msg });
            } finally {
                setIsLoading(false);
            }
        }
        fetchStandards();
    }, []);

    const handleStandardChange = (e: ChangeEvent<HTMLSelectElement>) => {
        const newId = e.target.value;
        setSelectedStandardId(newId);
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
        setFeedback(null);
        
        // Reset all highlights
        setHighlightStandardSelect(false);
        setHighlightAmount(false);
        setHighlightFirstName(false);
        setHighlightLastName(false);
        
        if (!selectedStandardId) {
            setHighlightStandardSelect(true);
            document.getElementById('standard-select')?.focus();
            document.getElementById('standard-select')?.scrollIntoView?.({ behavior: 'smooth', block: 'center' });
            return;
        }
        
        if (!amount) {
            setHighlightAmount(true);
            document.getElementById('amount-input')?.focus();
            document.getElementById('amount-input')?.scrollIntoView?.({ behavior: 'smooth', block: 'center' });
            return;
        }
        
        if (!firstName) {
            setHighlightFirstName(true);
            document.getElementById('first-name-input')?.focus();
            document.getElementById('first-name-input')?.scrollIntoView?.({ behavior: 'smooth', block: 'center' });
            return;
        }
        
        if (!lastName) {
            setHighlightLastName(true);
            document.getElementById('last-name-input')?.focus();
            document.getElementById('last-name-input')?.scrollIntoView?.({ behavior: 'smooth', block: 'center' });
            return;
        }
        
        const selectedStandard = standards.find(s => s.id === selectedStandardId);
        if (!selectedStandard) {
            setFeedback({ type: 'error', msg: "Selected standard not found." });
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
            if (profile.address) {
                if (profile.address.street) setStreet(profile.address.street);
                if (profile.address.house_number) setHouseNumber(profile.address.house_number);
                if (profile.address.zip_code) setZipCode(profile.address.zip_code);
                if (profile.address.city) setCity(profile.address.city);
                if (profile.address.country) setCountry(profile.address.country);
            }
            logger.info("CreateVoucher: Profile data loaded successfully.");
        } catch (e) {
            logger.error(`Failed to load profile for voucher creation: ${e}`);
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
            setCoordWarning("Invalid coordinate format.");
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
            },
        };
        try {
            await protectAction(async (password) => {
                await invoke("create_new_voucher", { standardTomlContent: selectedStandard.content, data: voucherData, password });
            });
            setFeedback({type: 'success', msg: "Voucher created successfully! Redirecting..."});
            setTimeout(onVoucherCreated, 2000);
        } catch (e) {
            const msg = `Failed to create voucher: ${e}`;
            setFeedback({type: 'error', msg});
            setIsLoading(false);
            setShowConfirm(false);
        }
    }

    const inputClass = "block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50";

    return (
        <PageLayout 
            title="Create New Voucher" 
            onBack={onCancel}
        >
            <div className="rounded-lg border border-theme-subtle bg-bg-card-alternate p-6 shadow-lg">
                <form onSubmit={handleCreateClick}>
                    <Fieldset legend="Basic Information">
                        <div>
                            <label htmlFor="standard-select" className="block text-sm font-medium text-theme-secondary mb-1">Voucher Type</label>
                            <select id="standard-select" value={selectedStandardId} onChange={(e) => { handleStandardChange(e); setHighlightStandardSelect(false); }} disabled={isLoading || standards.length === 0} className={`${inputClass} ${highlightStandardSelect ? 'border-4 border-red-500 ring-4 ring-red-200' : ''}`}>
                                <option value="">-- Please select a voucher standard --</option>
                                {standards.map(s => {
                                    const { name, issuer } = parseStandardInfo(s.content);
                                    let label = name ? name : s.id;
                                    if (issuer) {
                                        const displayIssuer = issuer.length > 25 ? issuer.substring(0, 22) + "..." : issuer;
                                        label = `${label} (${displayIssuer})`;
                                    }
                                    return <option key={s.id} value={s.id}>{label}</option>;
                                })}
                            </select>
                        </div>
                        <div>
                            <label htmlFor="amount-input" className="block text-sm font-medium text-theme-secondary mb-1">Amount (e.g., 60)</label>
                            <input id="amount-input" type="number" value={amount} onChange={(e) => { setAmount(e.target.value); setHighlightAmount(false); }} disabled={isLoading} className={`${inputClass} ${highlightAmount ? 'border-4 border-red-500 ring-4 ring-red-200' : ''}`}/>
                        </div>
                        <div>
                            <label htmlFor="validityValue" className="block text-sm font-medium text-theme-secondary mb-1">Validity</label>
                            <div className="flex items-center gap-2">
                                <input id="validityValue" type="number" value={validityValue} onChange={(e) => setValidityValue(parseInt(e.target.value, 10) || 0)} disabled={isLoading} className="block w-2/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"/>
                                <select id="validityUnit" value={validityUnit} onChange={(e) => setValidityUnit(e.target.value as "Y" | "M" | "D")} disabled={isLoading} className="block w-1/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50">
                                    <option value="Y">Years</option>
                                    <option value="M">Months</option>
                                    <option value="D">Days</option>
                                </select>
                            </div>
                        </div>
                        <div className="flex items-center gap-2">
                            <input id="nonRedeemable" type="checkbox" checked={nonRedeemable} onChange={(e) => {
                                if (e.target.checked) {
                                    setNonRedeemable(true);
                                } else {
                                    setShowTestVoucherWarning(true);
                                }
                            }} disabled={isLoading} className="h-4 w-4 rounded border-gray-300 text-theme-accent focus:ring-theme-accent"/>
                            <label htmlFor="nonRedeemable" className="block text-sm font-medium text-theme-secondary">Non-redeemable Test Voucher</label>
                        </div>
                    </Fieldset>

                    <Fieldset legend="Creator Details">
                        <div className="flex justify-end mb-2">
                             <Button type="button" variant="secondary" size="sm" onClick={handleLoadProfile} disabled={isLoading}>
                                 Load Profile Data
                             </Button>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="community" className="block text-sm font-medium text-theme-secondary mb-1">Community (Optional)</label>
                                <input id="community" type="text" value={community} onChange={(e) => setCommunity(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="organization" className="block text-sm font-medium text-theme-secondary mb-1">Organization (Optional)</label>
                                <input id="organization" type="text" value={organization} onChange={(e) => setOrganization(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="first-name-input" className="block text-sm font-medium text-theme-secondary mb-1">First Name (Required)</label>
                                <input id="first-name-input" type="text" value={firstName} onChange={(e) => { setFirstName(e.target.value); setHighlightFirstName(false); }} disabled={isLoading} className={`${inputClass} ${highlightFirstName ? 'border-4 border-red-500 ring-4 ring-red-200' : ''}`}/>
                            </div>
                            <div>
                                <label htmlFor="last-name-input" className="block text-sm font-medium text-theme-secondary mb-1">Last Name (Required)</label>
                                <input id="last-name-input" type="text" value={lastName} onChange={(e) => { setLastName(e.target.value); setHighlightLastName(false); }} disabled={isLoading} className={`${inputClass} ${highlightLastName ? 'border-4 border-red-500 ring-4 ring-red-200' : ''}`}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div className="col-span-1">
                                <label htmlFor="street" className="block text-sm font-medium text-theme-secondary mb-1">Street</label>
                                <input id="street" type="text" value={street} onChange={(e) => setStreet(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div className="col-span-2">
                                <label htmlFor="houseNumber" className="block text-sm font-medium text-theme-secondary mb-1">House No.</label>
                                <input id="houseNumber" type="text" value={houseNumber} onChange={(e) => setHouseNumber(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div className="col-span-1">
                                <label htmlFor="zipCode" className="block text-sm font-medium text-theme-secondary mb-1">ZIP Code</label>
                                <input id="zipCode" type="text" value={zipCode} onChange={(e) => setZipCode(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div className="col-span-2">
                                <label htmlFor="city" className="block text-sm font-medium text-theme-secondary mb-1">City</label>
                                <input id="city" type="text" value={city} onChange={(e) => setCity(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div>
                            <label htmlFor="country" className="block text-sm font-medium text-theme-secondary mb-1">Country</label>
                            <input id="country" type="text" value={country} onChange={(e) => setCountry(e.target.value)} disabled={isLoading} className={inputClass}/>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="gender" className="block text-sm font-medium text-theme-secondary mb-1">Gender</label>
                                <select id="gender" value={gender} onChange={(e) => setGender(e.target.value)} required disabled={isLoading} className={inputClass}>
                                    <option value="1">Male</option>
                                    <option value="2">Female</option>
                                    <option value="0">Not Known</option>
                                    <option value="9">Not Applicable</option>
                                </select>
                            </div>
                            <div>
                                <label htmlFor="coordinates" className="block text-sm font-medium text-theme-secondary mb-1">Coordinates (Optional)</label>
                                <input id="coordinates" type="text" value={coordinates} placeholder="e.g. 51.16, 10.45" onChange={(e) => { setCoordinates(e.target.value); if(coordWarning) setCoordWarning("");}} onBlur={handleCoordBlur} disabled={isLoading} className={`${inputClass} ${coordWarning ? 'border-4 border-red-500 ring-4 ring-red-200' : ''}`}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="email" className="block text-sm font-medium text-theme-secondary mb-1">Email (Optional)</label>
                                <input id="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="phone" className="block text-sm font-medium text-theme-secondary mb-1">Phone (Optional)</label>
                                <input id="phone" type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div>
                            <label htmlFor="url" className="block text-sm font-medium text-theme-secondary mb-1">Website (Optional)</label>
                            <input id="url" type="text" value={url} onChange={(e) => setUrl(e.target.value)} disabled={isLoading} className={inputClass}/>
                        </div>
                        <div>
                            <label htmlFor="serviceOffer" className="block text-sm font-medium text-theme-secondary mb-1">Service Offer (Optional)</label>
                            <textarea id="serviceOffer" value={serviceOffer} onChange={(e) => setServiceOffer(e.target.value)} disabled={isLoading} className={inputClass} rows={3}></textarea>
                        </div>
                        <div>
                            <label htmlFor="needs" className="block text-sm font-medium text-theme-secondary mb-1">Needs (Optional)</label>
                            <textarea id="needs" value={needs} onChange={(e) => setNeeds(e.target.value)} disabled={isLoading} className={inputClass} rows={3}></textarea>
                        </div>
                    </Fieldset>

                    <Fieldset legend="Collateral (if applicable for standard)">
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div>
                                <label htmlFor="collateralAmount" className="block text-sm font-medium text-theme-secondary mb-1">Amount</label>
                                <input id="collateralAmount" type="number" value={collateralAmount} onChange={(e) => setCollateralAmount(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="collateralUnit" className="block text-sm font-medium text-theme-secondary mb-1">Unit</label>
                                <input id="collateralUnit" type="text" value={collateralUnit} onChange={(e) => setCollateralUnit(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="collateralAbbreviation" className="block text-sm font-medium text-theme-secondary mb-1">Abbreviation</label>
                                <input id="collateralAbbreviation" type="text" value={collateralAbbreviation} onChange={(e) => setCollateralAbbreviation(e.target.value)} disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                    </Fieldset>

                    {feedback && (
                        <p className={`text-center text-sm font-semibold ${feedback.type === 'error' ? 'text-theme-error' : 'text-theme-success'}`}>
                            {feedback.msg}
                        </p>
                    )}

                    {/* Public Data Notice */}
                    <div className="bg-amber-50 border-l-4 border-amber-500 p-4 rounded-md shadow-sm">
                        <div className="flex items-start gap-3">
                            <div className="flex-shrink-0 text-amber-600 text-2xl">⚠️</div>
                            <div>
                                <p className="text-sm font-semibold text-amber-800">
                                    Public Data Notice
                                </p>
                                <p className="text-xs text-amber-700 mt-1">
                                    The information you enter into this voucher is crucial for building trust in this voucher. Please be aware that all details stored within this voucher will be readable by anyone who receives or verifies it in the future.
                                </p>
                            </div>
                        </div>
                    </div>

                    <div className="flex justify-end gap-4 pt-6">
                        <Button type="submit" disabled={isLoading}>{isLoading ? "Creating..." : "Create Voucher"}</Button>
                    </div>
                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title="Create Voucher?"
                description={<p>Do you really want to create a new <strong>{(() => {
                    const selected = standards.find(s => s.id === selectedStandardId);
                    if (!selected) return `${amount}`;
                    const { name, abbreviation, unit } = parseStandardInfo(selected.content);
                    let displayName = name || selected.id;
                    let displayUnit = abbreviation || unit || 'Units';

                    // BFF Pattern: Add TEST- prefix manually because the voucher doesn't exist yet,
                    // so the core query logic (format_bff_name) hasn't processed it.
                    if (nonRedeemable) {
                        if (!displayName.startsWith("TEST-")) displayName = `TEST-${displayName}`;
                        if (!displayUnit.startsWith("TEST-")) displayUnit = `TEST-${displayUnit}`;
                    }

                    return `${amount} ${displayUnit} of ${displayName}`;
                })()}</strong> voucher?<br/><br/>This action will sign the voucher with your private key.</p>}
                confirmText="Yes, Create"
                onConfirm={executeCreation}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isLoading}
            />

            <ConfirmationModal
                isOpen={showTestVoucherWarning}
                title="Early Phase Warning"
                description={<p>In this early phase, we recommend creating only <strong>non-redeemable test vouchers</strong> without real value. This allows you to test the system without financial risk.<br/><br/>Do you want to proceed with creating a real, redeemable voucher?</p>}
                confirmText="I still want to create a real voucher"
                cancelText="OK, I'll create only a test voucher"
                onConfirm={() => {
                    setNonRedeemable(false);
                    setShowTestVoucherWarning(false);
                }}
                onCancel={() => {
                    setNonRedeemable(true);
                    setShowTestVoucherWarning(false);
                }}
                confirmVariant="danger"
            />
        </PageLayout>
    );
}
