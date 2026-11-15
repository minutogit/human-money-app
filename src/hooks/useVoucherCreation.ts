// src/hooks/useVoucherCreation.ts
import { useState, useEffect, useCallback } from "react";
import { standardsService } from "../services/standardsService";
import { profileService } from "../services/profileService";
import { marketService } from "../services/marketService";
import { logger } from "../utils/log";
import { stringifyError } from "../utils/errorHelper";
import { 
    VoucherStandardInfo, 
    VoucherStandardDefinition, 
    CreatorData, 
    CollateralData, 
    NewVoucherData
} from "../types";
import { normalizeCoordinates } from "../utils/geoUtils";

/**
 * Checks whether a voucher standard's immutable custom CEL rules require the creator's own gender.
 * Guarantor-only rules (`s.role != 'creator'`) are handled at countersignature time and return false.
 */
export function standardRequiresCreatorGender(
    parsedStandard?: VoucherStandardDefinition | null
): boolean {
    const customRules = parsedStandard?.immutable?.customRules;
    if (!customRules) return false;
    return Object.values(customRules).some((rule) => {
        const expr = rule.expression;
        if (!expr || !expr.includes("gender")) return false;
        if (expr.includes("creator.gender")) return true;
        return !/role\s*!=\s*['"]creator['"]/.test(expr);
    });
}

/**
 * Formats a concise localized one-sentence summary for a selected voucher standard.
 */
export function getStandardShortSummary(
    parsedStandard: VoucherStandardDefinition | null | undefined,
    language: string,
    amount: string,
    fallback: (name: string, unit: string) => string
): string | null {
    if (!parsedStandard) return null;
    const descriptions = parsedStandard.mutable?.i18n?.descriptions || {};
    const langCode = language.split(/[-_]/)[0]?.toLowerCase() || "";
    const rawTemplate =
        (langCode ? descriptions[langCode] : undefined) ||
        descriptions.de ||
        descriptions.en ||
        Object.values(descriptions).find((val) => typeof val === "string" && val.trim().length > 0);
    if (rawTemplate && rawTemplate.trim().length > 0) {
        return rawTemplate.replace(/\{\{amount\}\}/g, amount.trim() || "1");
    }
    return fallback(
        parsedStandard.immutable.identity.name,
        parsedStandard.immutable.blueprint.unit
    );
}

export function useVoucherCreation() {
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string>("");
    const [parsedStandard, setParsedStandard] = useState<VoucherStandardDefinition | null>(null);
    const [amount, setAmount] = useState("");
    const [validityValue, setValidityValue] = useState<number>(3);
    const [validityUnit, setValidityUnit] = useState<"Y" | "M" | "D">("Y");
    const [nonRedeemable, setNonRedeemable] = useState(true);

    const [identity, setIdentity] = useState<CreatorData>({
        firstName: "",
        lastName: "",
        address: { street: "", houseNumber: "", zipCode: "", city: "", country: "" },
        gender: "0",
        coordinates: "",
    });

    const [collateral, setCollateral] = useState<CollateralData>({
        amount: "",
        unit: "",
        abbreviation: "",
    });

    const [isLoading, setIsLoading] = useState(true);
    const [coordWarning, setCoordWarning] = useState("");
    const [errors, setErrors] = useState<Record<string, boolean>>({});

    useEffect(() => {
        let active = true;
        async function fetchStandards() {
            try {
                const fetchedStandards = await standardsService.getStandards();
                if (!active) return;
                setStandards(fetchedStandards);
            } catch (e) {
                if (!active) return;
                logger.error(`Failed to fetch standards: ${stringifyError(e)}`);
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchStandards();
        return () => { active = false; };
    }, []);

    const handleStandardChange = useCallback(async (newId: string) => {
        setSelectedStandardId(newId);
        setErrors(prev => ({ ...prev, standard: false }));
        
        const selectedStd = standards.find(s => s.id === newId);
        if (selectedStd) {
            try {
                const parsed = await standardsService.parseStandard(selectedStd.content);
                setParsedStandard(parsed);
                
                if (parsed.mutable.appConfig.defaultValidityDuration) {
                    const duration = parsed.mutable.appConfig.defaultValidityDuration;
                    const match = duration.match(/P(\d+)([YMD])/);
                    if (match) {
                        setValidityValue(parseInt(match[1], 10));
                        setValidityUnit(match[2] as "Y" | "M" | "D");
                    }
                }
            } catch (e) {
                logger.error(`Failed to parse standard: ${stringifyError(e)}`);
                setParsedStandard(null);
            }
        } else {
            setParsedStandard(null);
        }
    }, [standards]);

    const handleLoadProfile = useCallback(async () => {
        try {
            const profile = await profileService.getProfile();
            let serviceOffer = profile.serviceOffer;
            if (!serviceOffer || serviceOffer.trim() === '') {
                try {
                    const myMarket = await marketService.getMyMarketProfile();
                    if (myMarket.skills && myMarket.skills.length > 0) {
                        serviceOffer = myMarket.skills.map(s => s.title.trim()).filter(Boolean).join(', ');
                    }
                } catch {
                    // Ignore if market profile cannot be loaded
                }
            }

            setIdentity(prev => ({
                ...prev,
                firstName: profile.firstName || prev.firstName,
                lastName: profile.lastName || prev.lastName,
                organization: profile.organization || prev.organization,
                community: profile.community || prev.community,
                gender: profile.gender || prev.gender,
                email: profile.email || prev.email,
                phone: profile.phone || prev.phone,
                url: profile.url || prev.url,
                coordinates: profile.coordinates || prev.coordinates,
                serviceOffer: serviceOffer || prev.serviceOffer,
                needs: profile.needs || prev.needs,
                pictureUrl: profile.pictureUrl || prev.pictureUrl,
                address: profile.address ? {
                    street: profile.address.street || prev.address.street,
                    houseNumber: profile.address.houseNumber || prev.address.houseNumber,
                    zipCode: profile.address.zipCode || prev.address.zipCode,
                    city: profile.address.city || prev.address.city,
                    country: profile.address.country || prev.address.country,
                } : prev.address
            }));
            logger.info("CreateVoucher: Profile data synchronized.");
        } catch (e) {
            logger.error(`Failed to load profile: ${stringifyError(e)}`);
        }
    }, []);

    useEffect(() => {
        let active = true;
        (async () => {
            try {
                const profile = await profileService.getProfile();
                if (!active) return;
                let serviceOffer = profile.serviceOffer;
                if (!serviceOffer || serviceOffer.trim() === '') {
                    try {
                        const myMarket = await marketService.getMyMarketProfile();
                        if (!active) return;
                        if (myMarket.skills && myMarket.skills.length > 0) {
                            serviceOffer = myMarket.skills.map(s => s.title.trim()).filter(Boolean).join(', ');
                        }
                    } catch {
                        // Ignore if market profile cannot be loaded
                    }
                }

                if (!active) return;
                setIdentity(prev => ({
                    ...prev,
                    firstName: profile.firstName || prev.firstName,
                    lastName: profile.lastName || prev.lastName,
                    organization: profile.organization || prev.organization,
                    community: profile.community || prev.community,
                    gender: profile.gender || prev.gender,
                    email: profile.email || prev.email,
                    phone: profile.phone || prev.phone,
                    url: profile.url || prev.url,
                    coordinates: profile.coordinates || prev.coordinates,
                    serviceOffer: serviceOffer || prev.serviceOffer,
                    needs: profile.needs || prev.needs,
                    pictureUrl: profile.pictureUrl || prev.pictureUrl,
                    address: profile.address ? {
                        street: profile.address.street || prev.address.street,
                        houseNumber: profile.address.houseNumber || prev.address.houseNumber,
                        zipCode: profile.address.zipCode || prev.address.zipCode,
                        city: profile.address.city || prev.address.city,
                        country: profile.address.country || prev.address.country,
                    } : prev.address
                }));
                logger.info("CreateVoucher: Profile data synchronized.");
            } catch (e) {
                if (!active) return;
                logger.error(`Failed to load profile: ${stringifyError(e)}`);
            }
        })();
        return () => { active = false; };
    }, []);

    const handleCoordBlur = useCallback(() => {
        if (!identity.coordinates) {
            setCoordWarning("");
            return;
        }
        const normalized = normalizeCoordinates(identity.coordinates);
        if (normalized) {
            setIdentity(prev => ({ ...prev, coordinates: normalized }));
            setCoordWarning("");
        } else {
            setCoordWarning("Invalid GPS coordinate format.");
        }
    }, [identity.coordinates]);

    useEffect(() => {
        if (amount) {
            setErrors(prev => prev.amount ? { ...prev, amount: false } : prev);
        }
    }, [amount]);

    useEffect(() => {
        if (identity.firstName) {
            setErrors(prev => prev.firstName ? { ...prev, firstName: false } : prev);
        }
    }, [identity.firstName]);

    useEffect(() => {
        if (identity.lastName) {
            setErrors(prev => prev.lastName ? { ...prev, lastName: false } : prev);
        }
    }, [identity.lastName]);

    useEffect(() => {
        if (!standardRequiresCreatorGender(parsedStandard) || identity.gender === "1" || identity.gender === "2") {
            setErrors(prev => prev.gender ? { ...prev, gender: false } : prev);
        }
    }, [parsedStandard, identity.gender]);

    const validate = useCallback(() => {
        const newErrors: Record<string, boolean> = {};
        if (!selectedStandardId) newErrors.standard = true;
        if (!amount) newErrors.amount = true;
        if (!identity.firstName) newErrors.firstName = true;
        if (!identity.lastName) newErrors.lastName = true;
        if (standardRequiresCreatorGender(parsedStandard) && identity.gender !== "1" && identity.gender !== "2") {
            newErrors.gender = true;
        }
        
        setErrors(newErrors);
        return newErrors;
    }, [selectedStandardId, amount, identity, parsedStandard]);

    const prepareVoucherData = useCallback((): NewVoucherData => {
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const { street, houseNumber, zipCode, city, country } = identity.address;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();
        const creatorGender = standardRequiresCreatorGender(parsedStandard) ? identity.gender : "0";
        
        return {
            validityDuration,
            nonRedeemableTestVoucher: nonRedeemable,
            nominalValue: { amount, unit: parsedStandard?.immutable.blueprint.unit || "Units" },
            collateral,
            creator: {
                ...identity,
                gender: creatorGender,
                address: { ...identity.address, fullAddress }
            },
        };
    }, [identity, collateral, amount, nonRedeemable, parsedStandard, validityValue, validityUnit]);

    return {
        standards,
        selectedStandardId,
        parsedStandard,
        amount,
        validityValue,
        validityUnit,
        nonRedeemable,
        identity,
        collateral,
        isLoading,
        coordWarning,
        errors,
        setAmount,
        setValidityValue,
        setValidityUnit,
        setNonRedeemable,
        setIdentity,
        setCollateral,
        setErrors,
        handleStandardChange,
        handleLoadProfile,
        handleCoordBlur,
        validate,
        prepareVoucherData,
        setIsLoading
    };
}
