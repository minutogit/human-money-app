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

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

// Hilfs-Komponente zur Strukturierung des Formulars
const Fieldset: React.FC<{ legend: string; children: React.ReactNode }> = ({ legend, children }) => (
    <fieldset className="rounded-lg border border-theme-subtle p-4 mb-6">
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
    const durationMatch = tomlContent.match(/default_validity_duration\s*=\s*"P(\d+)([YMD])"/);
    let validity = null;
    if (durationMatch) {
        validity = { value: parseInt(durationMatch[1], 10), unit: durationMatch[2] as "Y" | "M" | "D" };
    }
    return { name, issuer, validity };
}

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    const { protectAction } = useSession();
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string>("");
    const [amount, setAmount] = useState("");
    const [validityValue, setValidityValue] = useState<number>(3);
    const [validityUnit, setValidityUnit] = useState<"Y" | "M" | "D">("Y");
    const [nonRedeemable, setNonRedeemable] = useState(false);

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

    useEffect(() => {
        logger.info("CreateVoucher component displayed");
        async function fetchStandards() {
            try {
                info("CreateVoucher: Fetching voucher standards.");
                const fetchedStandards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setStandards(fetchedStandards);
                if (fetchedStandards.length > 0) {
                    setSelectedStandardId(fetchedStandards[0].id);
                    const { validity } = parseStandardInfo(fetchedStandards[0].content);
                    if (validity) {
                        setValidityValue(validity.value);
                        setValidityUnit(validity.unit);
                        info(`CreateVoucher: Applied default validity P${validity.value}${validity.unit}`);
                    }
                }
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
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();
        const voucherData: NewVoucherData = {
            validity_duration: validityDuration,
            non_redeemable_test_voucher: nonRedeemable,
            nominal_value: { amount, unit: "Minuto" },
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
        <div className="mx-auto max-w-2xl">
            <h1 className="text-3xl font-bold text-center mb-6 text-theme-primary">Create New Voucher</h1>
            <div className="rounded-lg border border-theme-subtle bg-bg-card-alternate p-6 shadow-lg">
                <form onSubmit={handleCreateClick}>
                    <Fieldset legend="Basic Information">
                        <div>
                            <label htmlFor="standard" className="block text-sm font-medium text-theme-secondary mb-1">Voucher Type</label>
                            <select id="standard" value={selectedStandardId} onChange={handleStandardChange} disabled={isLoading || standards.length === 0} className={inputClass}>
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
                            <label htmlFor="amount" className="block text-sm font-medium text-theme-secondary mb-1">Amount (e.g., 60)</label>
                            <input id="amount" type="number" value={amount} onChange={(e) => setAmount(e.target.value)} required disabled={isLoading} className={inputClass} title="Please enter an amount."/>
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
                            <input id="nonRedeemable" type="checkbox" checked={nonRedeemable} onChange={(e) => setNonRedeemable(e.target.checked)} disabled={isLoading} className="h-4 w-4 rounded border-gray-300 text-theme-accent focus:ring-theme-accent"/>
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
                                <label htmlFor="firstName" className="block text-sm font-medium text-theme-secondary mb-1">First Name (Required)</label>
                                <input id="firstName" type="text" value={firstName} onChange={(e) => setFirstName(e.target.value)} required disabled={isLoading} className={inputClass} title="Please enter a first name."/>
                            </div>
                            <div>
                                <label htmlFor="lastName" className="block text-sm font-medium text-theme-secondary mb-1">Last Name (Required)</label>
                                <input id="lastName" type="text" value={lastName} onChange={(e) => setLastName(e.target.value)} required disabled={isLoading} className={inputClass} title="Please enter a last name."/>
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
                                <input id="coordinates" type="text" value={coordinates} placeholder="e.g. 51.16, 10.45" onChange={(e) => { setCoordinates(e.target.value); if(coordWarning) setCoordWarning("");}} onBlur={handleCoordBlur} disabled={isLoading} className={`${inputClass} ${coordWarning ? 'border-red-500 ring-red-500' : ''}`}/>
                                {coordWarning && <p className="text-[10px] text-red-500 mt-1 font-medium">{coordWarning}</p>}
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

                    <div className="flex justify-end gap-4 pt-6">
                        <Button type="button" onClick={onCancel} variant="secondary" disabled={isLoading}>Cancel</Button>
                        <Button type="submit" disabled={isLoading}>{isLoading ? "Creating..." : "Create Voucher"}</Button>
                    </div>
                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title="Create Voucher?"
                description={<p>Do you really want to create a new <strong>{amount} {standards.find(s=>s.id===selectedStandardId)?.id || 'Minuto'}</strong> voucher?<br/><br/>This action will sign the voucher with your private key.</p>}
                confirmText="Yes, Create"
                onConfirm={executeCreation}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isLoading}
            />
        </div>
    );
}