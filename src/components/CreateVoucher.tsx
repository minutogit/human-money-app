// src/components/CreateVoucher.tsx
import { useState, useEffect, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error, info } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { NewVoucherData, VoucherStandardInfo } from "../types";


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

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    // Basic Voucher State
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string>("");
    const [amount, setAmount] = useState("");
    const [validityValue, setValidityValue] = useState<number>(3);
    const [validityUnit, setValidityUnit] = useState<"Y" | "M" | "D">("Y");
    const [nonRedeemable, setNonRedeemable] = useState(false);

    // Creator State
    const [firstName, setFirstName] = useState("");
    const [lastName, setLastName] = useState("");
    const [street, setStreet] = useState("");
    const [houseNumber, setHouseNumber] = useState("");
    const [zipCode, setZipCode] = useState("");
    const [city, setCity] = useState("");
    const [country, setCountry] = useState("");
    const [gender, setGender] = useState("0"); // ISO 5218
    const [coordinates, setCoordinates] = useState(""); // "lat, lon"
    const [email, setEmail] = useState("");
    const [phone, setPhone] = useState("");
    const [url, setUrl] = useState("");
    const [organization, setOrganization] = useState("");
    const [community, setCommunity] = useState("");
    const [serviceOffer, setServiceOffer] = useState("");
    const [needs, setNeeds] = useState("");

    // Collateral State
    const [collateralAmount, setCollateralAmount] = useState("");
    const [collateralUnit, setCollateralUnit] = useState("");
    const [collateralAbbreviation, setCollateralAbbreviation] = useState("");

    // General Component State
    const [password, setPassword] = useState("");
    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);

    useEffect(() => {
        logger.info("CreateVoucher component displayed");
        async function fetchStandards() {
            try {
                info("CreateVoucher: Fetching voucher standards.");
                const fetchedStandards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setStandards(fetchedStandards);
                if (fetchedStandards.length > 0) {
                    setSelectedStandardId(fetchedStandards[0].id);
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

    async function handleSubmit(event: FormEvent) {
        event.preventDefault();
        // Guard Clause: Prevent multiple submissions if already loading.
        if (isLoading) {
            return;
        }

        setFeedback(null);
        setIsLoading(true);

        const selectedStandard = standards.find(s => s.id === selectedStandardId);
        if (!selectedStandard) {
            setFeedback({ type: 'error', msg: "Selected standard not found." });
            setIsLoading(false);
            return;
        }

        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();

        const voucherData: NewVoucherData = {
            validity_duration: validityDuration,
            non_redeemable_test_voucher: nonRedeemable,
            nominal_value: {
                amount,
                unit: "Minuto", // Hardcoded for prototype as per library design
            },
            collateral: {
                amount: collateralAmount,
                unit: collateralUnit,
                abbreviation: collateralAbbreviation,
            },
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
                // Hinzugefügt, um die neuen Felder zu übergeben
            },
        };

        try {
            await invoke("create_new_voucher", {
                standardTomlContent: selectedStandard.content,
                data: voucherData,
                password,
            });
            setFeedback({type: 'success', msg: "Voucher created successfully! Redirecting..."});
            setTimeout(onVoucherCreated, 2000);
        } catch (e) {
            const msg = `Failed to create voucher: ${e}`;
            setFeedback({type: 'error', msg});
            // ONLY set loading to false on error, so the user can try again.
            // On success, the form should remain disabled until navigation.
            setIsLoading(false);
        }
    }

    const inputClass = "block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50";

    return (
        <div className="mx-auto max-w-2xl">
            <h1 className="text-3xl font-bold text-center mb-6 text-theme-primary">Create New Voucher</h1>
            <div className="rounded-lg border border-theme-subtle bg-bg-card-alternate p-6 shadow-lg">
                <form onSubmit={handleSubmit}>
                    <Fieldset legend="Basic Information">
                        <div>
                            <label htmlFor="standard" className="block text-sm font-medium text-theme-secondary mb-1">Voucher
                                Type</label>
                            <select id="standard" value={selectedStandardId}
                                    onChange={(e) => setSelectedStandardId(e.target.value)}
                                    disabled={isLoading || standards.length === 0} className={inputClass}>
                                {standards.map(s => <option key={s.id} value={s.id}>{s.id}</option>)}
                            </select>
                        </div>
                        <div>
                            <label htmlFor="amount" className="block text-sm font-medium text-theme-secondary mb-1">Amount
                                (e.g., 60)</label>
                            <input id="amount" type="number" value={amount} onChange={(e) => setAmount(e.target.value)}
                                   required disabled={isLoading} className={inputClass}
                                   title="Please enter an amount."/>
                        </div>
                        <div>
                            <label htmlFor="validityValue"
                                   className="block text-sm font-medium text-theme-secondary mb-1">Validity</label>
                            <div className="flex items-center gap-2">
                                <input id="validityValue" type="number" value={validityValue}
                                       onChange={(e) => setValidityValue(parseInt(e.target.value, 10) || 0)}
                                       disabled={isLoading}
                                       className="block w-2/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"/>
                                <select id="validityUnit" value={validityUnit}
                                        onChange={(e) => setValidityUnit(e.target.value as "Y" | "M" | "D")}
                                        disabled={isLoading}
                                        className="block w-1/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50">
                                    <option value="Y">Years</option>
                                    <option value="M">Months</option>
                                    <option value="D">Days</option>
                                </select>
                            </div>
                        </div>
                        <div className="flex items-center gap-2">
                            <input id="nonRedeemable" type="checkbox" checked={nonRedeemable}
                                   onChange={(e) => setNonRedeemable(e.target.checked)} disabled={isLoading}
                                   className="h-4 w-4 rounded border-gray-300 text-theme-accent focus:ring-theme-accent"/>
                            <label htmlFor="nonRedeemable" className="block text-sm font-medium text-theme-secondary">Non-redeemable
                                Test Voucher</label>
                        </div>
                    </Fieldset>

                    <Fieldset legend="Creator Details">
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="community"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Community
                                    (Optional)</label>
                                <input id="community" type="text" value={community}
                                       onChange={(e) => setCommunity(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="organization"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Organization
                                    (Optional)</label>
                                <input id="organization" type="text" value={organization}
                                       onChange={(e) => setOrganization(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="firstName"
                                       className="block text-sm font-medium text-theme-secondary mb-1">First Name
                                    (Required)</label>
                                <input id="firstName" type="text" value={firstName}
                                       onChange={(e) => setFirstName(e.target.value)} required disabled={isLoading}
                                       className={inputClass} title="Please enter a first name."/>
                            </div>
                            <div>
                                <label htmlFor="lastName"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Last Name
                                    (Required)</label>
                                <input id="lastName" type="text" value={lastName}
                                       onChange={(e) => setLastName(e.target.value)} required disabled={isLoading}
                                       className={inputClass} title="Please enter a last name."/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div>
                                <label htmlFor="street"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Street</label>
                                <input id="street" type="text" value={street}
                                       onChange={(e) => setStreet(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                            <div className="col-span-2">
                                <label htmlFor="houseNumber"
                                       className="block text-sm font-medium text-theme-secondary mb-1">House No.</label>
                                <input id="houseNumber" type="text" value={houseNumber}
                                       onChange={(e) => setHouseNumber(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div>
                                <label htmlFor="zipCode"
                                       className="block text-sm font-medium text-theme-secondary mb-1">ZIP Code</label>
                                <input id="zipCode" type="text" value={zipCode}
                                       onChange={(e) => setZipCode(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                            <div className="col-span-2">
                                <label htmlFor="city"
                                       className="block text-sm font-medium text-theme-secondary mb-1">City</label>
                                <input id="city" type="text" value={city} onChange={(e) => setCity(e.target.value)}
                                       disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div>
                            <label htmlFor="country"
                                   className="block text-sm font-medium text-theme-secondary mb-1">Country</label>
                            <input id="country" type="text" value={country} onChange={(e) => setCountry(e.target.value)}
                                   disabled={isLoading} className={inputClass}/>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="gender"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Gender</label>
                                <select id="gender" value={gender} onChange={(e) => setGender(e.target.value)} required
                                        disabled={isLoading} className={inputClass}>
                                    <option value="1">Male</option>
                                    <option value="2">Female</option>
                                    <option value="0">Not Known</option>
                                    <option value="9">Not Applicable</option>
                                </select>
                            </div>
                            <div>
                                <label htmlFor="coordinates"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Coordinates
                                    (Optional)</label>
                                <input id="coordinates" type="text" value={coordinates} placeholder="e.g. 51.16, 10.45"
                                       onChange={(e) => setCoordinates(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                        </div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <div>
                                <label htmlFor="email" className="block text-sm font-medium text-theme-secondary mb-1">Email
                                    (Optional)</label>
                                <input id="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)}
                                       disabled={isLoading} className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="phone" className="block text-sm font-medium text-theme-secondary mb-1">Phone
                                    (Optional)</label>
                                <input id="phone" type="tel" value={phone} onChange={(e) => setPhone(e.target.value)}
                                       disabled={isLoading} className={inputClass}/>
                            </div>
                        </div>
                        <div>
                            <label htmlFor="url" className="block text-sm font-medium text-theme-secondary mb-1">Website
                                (Optional)</label>
                            <input id="url" type="text" value={url} onChange={(e) => setUrl(e.target.value)}
                                   disabled={isLoading}
                                   className={inputClass}/>
                        </div>
                        <div>
                            <label htmlFor="serviceOffer"
                                   className="block text-sm font-medium text-theme-secondary mb-1">Service Offer
                                (Optional)</label>
                            <textarea id="serviceOffer" value={serviceOffer}
                                      onChange={(e) => setServiceOffer(e.target.value)} disabled={isLoading}
                                      className={inputClass} rows={3}></textarea>
                        </div>
                        <div>
                            <label htmlFor="needs" className="block text-sm font-medium text-theme-secondary mb-1">Needs
                                (Optional)</label>
                            <textarea id="needs" value={needs} onChange={(e) => setNeeds(e.target.value)}
                                      disabled={isLoading} className={inputClass} rows={3}></textarea>
                        </div>
                    </Fieldset>

                    <Fieldset legend="Collateral (if applicable for standard)">
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <div>
                                <label htmlFor="collateralAmount"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Amount</label>
                                <input id="collateralAmount" type="number" value={collateralAmount}
                                       onChange={(e) => setCollateralAmount(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="collateralUnit"
                                       className="block text-sm font-medium text-theme-secondary mb-1">Unit</label>
                                <input id="collateralUnit" type="text" value={collateralUnit}
                                       onChange={(e) => setCollateralUnit(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                            <div>
                                <label htmlFor="collateralAbbreviation" className="block text-sm font-medium text-theme-secondary mb-1">Abbreviation</label>
                                <input id="collateralAbbreviation" type="text" value={collateralAbbreviation}
                                       onChange={(e) => setCollateralAbbreviation(e.target.value)} disabled={isLoading}
                                       className={inputClass}/>
                            </div>
                        </div>
                    </Fieldset>

                    <div className="mt-8 rounded-lg border border-theme-accent/50 bg-bg-card-alternate p-4">
                        <p className="text-center text-sm text-theme-light mb-4">
                            To authorize and sign the new voucher, please enter your wallet password.
                            <br/>
                            This password is only used for this action and is not stored in the voucher.
                        </p>
                        <input id="password" type="password" value={password}
                               onChange={(e) => setPassword(e.target.value)} required disabled={isLoading}
                               autoComplete="current-password" className={inputClass}
                               placeholder="Your Wallet Password (Required)"
                               title="Please enter your wallet password."/>
                    </div>

                    {feedback && (
                        <p className={`text-center text-sm font-semibold ${feedback.type === 'error' ? 'text-theme-error' : 'text-theme-success'}`}>
                            {feedback.msg}
                        </p>
                    )}

                    <div className="flex justify-end gap-4 pt-6">
                        <Button type="button" onClick={onCancel} variant="secondary" disabled={isLoading}>
                            Cancel
                        </Button>
                        <Button type="submit" disabled={isLoading}>
                            {isLoading ? "Creating..." : "Create Voucher"}
                        </Button>
                    </div>
                </form>
            </div>
        </div>
    );
}