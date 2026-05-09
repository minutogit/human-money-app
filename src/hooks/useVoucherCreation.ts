// src/hooks/useVoucherCreation.ts
import { useState, useEffect, useCallback } from "react";
import { standardsService } from "../services/standardsService";
import { profileService } from "../services/profileService";
import { logger } from "../utils/log";
import { 
    VoucherStandardInfo, 
    VoucherStandardDefinition, 
    CreatorData, 
    CollateralData, 
    NewVoucherData
} from "../types";
import { normalizeCoordinates } from "../utils/geoUtils";

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
        async function fetchStandards() {
            try {
                const fetchedStandards = await standardsService.getStandards();
                setStandards(fetchedStandards);
            } catch (e) {
                logger.error(`Failed to fetch standards: ${e}`);
            } finally {
                setIsLoading(false);
            }
        }
        fetchStandards();
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
                logger.error(`Failed to parse standard: ${e}`);
                setParsedStandard(null);
            }
        } else {
            setParsedStandard(null);
        }
    }, [standards]);

    const handleLoadProfile = useCallback(async () => {
        try {
            const profile = await profileService.getProfile();
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
                serviceOffer: profile.serviceOffer || prev.serviceOffer,
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
            logger.error(`Failed to load profile: ${e}`);
        }
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

    const validate = useCallback(() => {
        const newErrors: Record<string, boolean> = {};
        if (!selectedStandardId) newErrors.standard = true;
        if (!amount) newErrors.amount = true;
        if (!identity.firstName) newErrors.firstName = true;
        if (!identity.lastName) newErrors.lastName = true;
        
        setErrors(newErrors);
        return newErrors;
    }, [selectedStandardId, amount, identity]);

    const prepareVoucherData = useCallback((): NewVoucherData => {
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const { street, houseNumber, zipCode, city, country } = identity.address;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();
        
        return {
            validityDuration,
            nonRedeemableTestVoucher: nonRedeemable,
            nominalValue: { amount, unit: parsedStandard?.immutable.blueprint.unit || "Units" },
            collateral,
            creator: {
                ...identity,
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
