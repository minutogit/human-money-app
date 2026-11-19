// src/hooks/useVoucherCreation.ts
import { useState, useEffect, useCallback, useRef } from "react";
import { standardsService } from "../services/standardsService";
import { profileService } from "../services/profileService";
import { marketService } from "../services/marketService";
import { settingsService } from "../services/settingsService";
import { logger } from "../utils/log";
import { stringifyError } from "../utils/errorHelper";
import { 
    VoucherStandardInfo, 
    VoucherStandardDefinition, 
    CreatorData, 
    CollateralData, 
    NewVoucherData,
    PublicProfile,
    StoredAnchorTemplateMeta
} from "../types";
import { normalizeCoordinates } from "../utils/geoUtils";
import { estimateIsoDurationMs, quarterEndUtcMs, formatQuarter } from "../utils/retentionQuarter";
import { APP_EVENTS, useAppEvent } from "../utils/appEvents";

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

function mergeProfileIntoCreatorIdentity(
    prev: CreatorData,
    profile: PublicProfile,
    serviceOffer?: string
): CreatorData {
    return {
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
        address: profile.address
            ? {
                  street: profile.address.street || prev.address.street,
                  houseNumber: profile.address.houseNumber || prev.address.houseNumber,
                  zipCode: profile.address.zipCode || prev.address.zipCode,
                  city: profile.address.city || prev.address.city,
                  country: profile.address.country || prev.address.country,
              }
            : prev.address,
    };
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
    const [anchorTemplates, setAnchorTemplates] = useState<StoredAnchorTemplateMeta[]>([]);
    const [selectedAnchorId, setSelectedAnchorId] = useState<string | null>(null);
    const hasUserSelectedAnchorRef = useRef(false);
    const isMountedRef = useRef(true);
    const standardParseSeqRef = useRef(0);

    const handleSelectAnchor = useCallback((anchorId: string | null) => {
        hasUserSelectedAnchorRef.current = true;
        setSelectedAnchorId(anchorId);
    }, []);

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
        };
    }, []);

    useEffect(() => {
        let active = true;
        settingsService
            .getConfig()
            .then((cfg) => {
                if (!active || !cfg?.lifecycle?.l2AnchorTemplatesMeta) return;
                setAnchorTemplates(cfg.lifecycle.l2AnchorTemplatesMeta);
            })
            .catch(() => {
                // Keep offline-only default if config is unavailable in isolated tests
            });
        return () => { active = false; };
    }, []);

    const refreshStandards = useCallback(async () => {
        try {
            const fetchedStandards = await standardsService.getStandards();
            if (!isMountedRef.current) return;
            setStandards(fetchedStandards);
        } catch (e) {
            if (!isMountedRef.current) return;
            logger.error(`Failed to fetch standards: ${stringifyError(e)}`);
        } finally {
            if (isMountedRef.current) setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        void refreshStandards();
    }, [refreshStandards]);

    useAppEvent(APP_EVENTS.STANDARDS_UPDATED, () => {
        void refreshStandards();
    });

    const handleStandardChange = useCallback(async (newId: string) => {
        const seq = ++standardParseSeqRef.current;
        setSelectedStandardId(newId);
        setErrors(prev => ({ ...prev, standard: false }));

        const selectedStd = standards.find(s => s.id === newId);
        if (selectedStd) {
            try {
                const parsed = await standardsService.parseStandard(selectedStd.content);
                if (!isMountedRef.current || standardParseSeqRef.current !== seq) return;
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
                if (!isMountedRef.current || standardParseSeqRef.current !== seq) return;
                logger.error(`Failed to parse standard: ${stringifyError(e)}`);
                setParsedStandard(null);
            }
        } else {
            setParsedStandard(null);
        }
    }, [standards]);

    const syncProfileIdentity = useCallback(async (isActive: () => boolean) => {
        try {
            const profile = await profileService.getProfile();
            if (!isActive()) return;

            let serviceOffer = profile.serviceOffer;
            if (!serviceOffer || serviceOffer.trim() === '') {
                try {
                    const myMarket = await marketService.getMyMarketProfile();
                    if (!isActive()) return;
                    if (myMarket.skills && myMarket.skills.length > 0) {
                        serviceOffer = myMarket.skills.map(s => s.title.trim()).filter(Boolean).join(', ');
                    }
                } catch {
                    // Ignore if market profile cannot be loaded
                }
            }

            if (!isActive()) return;
            setIdentity(prev => mergeProfileIntoCreatorIdentity(prev, profile, serviceOffer));
            logger.info("CreateVoucher: Profile data synchronized.");
        } catch (e) {
            if (!isActive()) return;
            logger.error(`Failed to load profile: ${stringifyError(e)}`);
        }
    }, []);

    const handleLoadProfile = useCallback(async () => {
        await syncProfileIdentity(() => isMountedRef.current);
    }, [syncProfileIdentity]);

    useEffect(() => {
        let active = true;
        void syncProfileIdentity(() => active);
        return () => { active = false; };
    }, [syncProfileIdentity]);

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

    const validityDurationStr = validityValue > 0 ? `P${validityValue}${validityUnit}` : parsedStandard?.mutable?.appConfig?.defaultValidityDuration;
    const historyRetentionIso = parsedStandard?.mutable?.appConfig?.serverHistoryRetention || "P1Y";
    const estimatedDeletableAtMs =
        Date.now() +
        estimateIsoDurationMs(validityDurationStr) +
        estimateIsoDurationMs(historyRetentionIso);

    useEffect(() => {
        if (hasUserSelectedAnchorRef.current) return;
        if (anchorTemplates.length === 0) {
            setSelectedAnchorId(null);
            return;
        }
        const compatible = anchorTemplates.find(
            (tpl) => (quarterEndUtcMs(tpl.retainUntilQuarter) ?? 0) >= estimatedDeletableAtMs
        );
        setSelectedAnchorId(compatible ? compatible.anchorId : null);
    }, [anchorTemplates, estimatedDeletableAtMs]);

    const selectedAnchorTemplate =
        anchorTemplates.find((tpl) => tpl.anchorId === selectedAnchorId) || null;
    const retentionInsufficient = Boolean(
        selectedAnchorTemplate &&
            (quarterEndUtcMs(selectedAnchorTemplate.retainUntilQuarter) ?? 0) < estimatedDeletableAtMs
    );
    const selectedAnchorQuarterLabel = selectedAnchorTemplate
        ? formatQuarter(selectedAnchorTemplate.retainUntilQuarter)
        : null;

    const prepareVoucherData = useCallback((): NewVoucherData => {
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;
        const { street, houseNumber, zipCode, city, country } = identity.address;
        const fullAddress = `${street} ${houseNumber}, ${zipCode} ${city}, ${country}`.trim();
        const creatorGender = standardRequiresCreatorGender(parsedStandard) ? identity.gender : "0";
        const normalizedAmount = amount.trim().replace(',', '.');
        const normalizedCollateral = collateral.amount.trim().replace(',', '.');
        
        return {
            validityDuration,
            nonRedeemableTestVoucher: nonRedeemable,
            nominalValue: { amount: normalizedAmount, unit: parsedStandard?.immutable.blueprint.unit || "Units" },
            collateral: { ...collateral, amount: normalizedCollateral },
            creator: {
                ...identity,
                gender: creatorGender,
                address: { ...identity.address, fullAddress }
            },
            layer2Anchor: selectedAnchorId || null,
        };
    }, [identity, collateral, amount, nonRedeemable, parsedStandard, validityValue, validityUnit, selectedAnchorId]);

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
        anchorTemplates,
        selectedAnchorId,
        selectedAnchorTemplate,
        selectedAnchorQuarterLabel,
        retentionInsufficient,
        setAmount,
        setValidityValue,
        setValidityUnit,
        setNonRedeemable,
        setIdentity,
        setCollateral,
        setErrors,
        setSelectedAnchorId: handleSelectAnchor,
        handleStandardChange,
        handleLoadProfile,
        handleCoordBlur,
        validate,
        prepareVoucherData,
        setIsLoading
    };
}
