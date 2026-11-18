import { useState, useEffect, useMemo, FormEvent, useReducer, useRef, useCallback } from "react";
import { useTranslation } from 'react-i18next';
import { voucherService } from "../services/voucherService";
import { transferService, CreateBundleResult } from "../services/transferService";
import { settingsService } from "../services/settingsService";
import { backupService } from "../services/backupService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
import { postService } from "../services/postService";
import { marketService } from "../services/marketService";
import { runPostSyncPass } from "../utils/postSync";
import { logger } from "../utils/log";
import { translateError, stringifyError } from "../utils/errorHelper";
import { 
    VoucherSummary, 
    VoucherStandardInfo, 
    SourceTransfer, 
    TransactionRecord, 
    Contact, 
    TrustStatus,
    InvolvedVoucherInfo,
    AppSettings,
    AssetClassSummary,
    VoucherStandardDefinition
} from "../types";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { HelpIcon } from "./ui/HelpIcon";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { PageLayout } from "./ui/PageLayout";
import { AlertBanner } from "./ui/AlertBanner";
import { Advanced } from "./ui/Advanced";
import { Disclosure } from "./ui/Disclosure";
import { TechId } from "./ui/TechId";
import { formatAmount } from "../utils/format";
import { readFromClipboard } from "../utils/clipboard";
import { parseCardUri, parseContactCard, parsePaymentRequest } from "../utils/humocoUri";
import {
    Shield,
    Edit3,
    Clipboard,
    AlertTriangle,
    RefreshCw,
    Send
} from "lucide-react";
import { UserAvatar } from "./ui/UserAvatar";

// Modular Components
import { RecipientSelector } from "./send/RecipientSelector";
import { PrivacyToggle } from "./send/PrivacyToggle";
import { AssetInventory } from "./send/AssetInventory";

// Custom Hooks
import { useVoucherSelection } from "../hooks/useVoucherSelection";
import { usePrivacyDetection } from "../hooks/usePrivacyDetection";

import { useNavigation } from "../context/NavigationContext";

type SendState = {
    recipientId: string;
    notes: string;
    sendProfileName: boolean;
    customSenderName: string;
    targetAmountStr: string;
    selectedStandardId: string | null;
    selectedIsTest: boolean | null;
    privacyMode: 'public' | 'stealth' | null;
    isProcessing: boolean;
    feedbackMsg: string;
    showConfirm: boolean;
    recipientError: boolean;
    privacyError: boolean;
};

type SendAction = 
    | { type: 'SET_RECIPIENT'; id: string }
    | { type: 'SET_NOTES'; notes: string }
    | { type: 'TOGGLE_PROFILE_NAME'; value: boolean }
    | { type: 'SET_SENDER_NAME'; name: string }
    | { type: 'SET_TARGET_AMOUNT'; amount: string }
    | { type: 'SET_STANDARD'; id: string | null; isTest: boolean | null; preserveAmount?: boolean }
    | { type: 'SET_PRIVACY'; mode: 'public' | 'stealth' | null }
    | { type: 'SET_PROCESSING'; value: boolean }
    | { type: 'SET_FEEDBACK'; msg: string }
    | { type: 'TOGGLE_CONFIRM'; value: boolean }
    | { type: 'SET_RECIPIENT_ERROR'; value: boolean }
    | { type: 'SET_PRIVACY_ERROR'; value: boolean };

const initialState: SendState = {
    recipientId: "",
    notes: "",
    sendProfileName: true,
    customSenderName: "",
    targetAmountStr: "",
    selectedStandardId: null,
    selectedIsTest: null,
    privacyMode: null,
    isProcessing: false,
    feedbackMsg: "",
    showConfirm: false,
    recipientError: false,
    privacyError: false,
};

function reducer(state: SendState, action: SendAction): SendState {
    switch (action.type) {
        case 'SET_RECIPIENT': return { ...state, recipientId: action.id.replace(/\s+/g, ''), recipientError: false };
        case 'SET_NOTES': return { ...state, notes: action.notes };
        case 'TOGGLE_PROFILE_NAME': return { ...state, sendProfileName: action.value };
        case 'SET_SENDER_NAME': return { ...state, customSenderName: action.name };
        case 'SET_TARGET_AMOUNT': return { ...state, targetAmountStr: action.amount };
        case 'SET_STANDARD': return { ...state, selectedStandardId: action.id, selectedIsTest: action.isTest, targetAmountStr: action.preserveAmount ? state.targetAmountStr : "" };
        case 'SET_PRIVACY': return { ...state, privacyMode: action.mode, privacyError: false };
        case 'SET_PROCESSING': return { ...state, isProcessing: action.value };
        case 'SET_FEEDBACK': return { ...state, feedbackMsg: action.msg };
        case 'TOGGLE_CONFIRM': return { ...state, showConfirm: action.value };
        case 'SET_RECIPIENT_ERROR': return { ...state, recipientError: action.value };
        case 'SET_PRIVACY_ERROR': return { ...state, privacyError: action.value };
        default: return state;
    }
}

function getPrecision(content: string): number {
    const match = content.match(/amount_decimal_places\s*=\s*(\d+)/i);
    return match && match[1] ? parseInt(match[1], 10) : 4;
}

function formatProfileDisplayName(profile?: {
    firstName?: string;
    lastName?: string;
    organization?: string;
    community?: string;
} | null): string {
    if (!profile) return "";
    const fullName = [profile.firstName, profile.lastName].filter(Boolean).join(" ").trim();
    let name = fullName || profile.organization || "";
    if (profile.community) {
        name = name ? `${name} (${profile.community})` : profile.community;
    }
    return name.startsWith("did:key:") ? "" : name;
}

export interface SendViewProps {
    initialRecipientId?: string;
    initialVoucherId?: string;
    initialNote?: string;
    initialPaymentUri?: string;
}

export function SendView({ initialRecipientId, initialVoucherId, initialNote, initialPaymentUri }: SendViewProps = {}) {
    const { t } = useTranslation();
    const { protectAction, profileName, accessMode = 'full', requireFullAccess } = useSession();
    const { navigate, goBack } = useNavigation();
    const [state, dispatch] = useReducer(reducer, initialState);
    const [paymentRequestSuccessMsg, setPaymentRequestSuccessMsg] = useState<string | null>(null);
    const [isDataLoaded, setIsDataLoaded] = useState(false);
    const [isEditingSender, setIsEditingSender] = useState(false);

    const navigationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const appliedInitialUriRef = useRef<string | null>(null);
    const preserveInitialVoucherRef = useRef<boolean>(Boolean(initialVoucherId));

    useEffect(() => {
        if (initialRecipientId) {
            dispatch({ type: 'SET_RECIPIENT', id: initialRecipientId });
        }
        if (initialNote) {
            dispatch({ type: 'SET_NOTES', notes: initialNote });
        }
    }, [initialRecipientId, initialNote]);

    useEffect(() => {
        void settingsService.prewarmL2Checkout?.().catch(() => undefined);
    }, []);

    useEffect(() => {
        return () => {
            if (navigationTimerRef.current) {
                clearTimeout(navigationTimerRef.current);
            }
        };
    }, []);
    
    const [availableVouchers, setAvailableVouchers] = useState<VoucherSummary[]>([]);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [activeAssetClasses, setActiveAssetClasses] = useState<AssetClassSummary[]>([]);
    const [ownUserId, setOwnUserId] = useState("");
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());
    const [uuidToPrecisionMap, setUuidToPrecisionMap] = useState<Map<string, number>>(new Map());
    const [standardsDict, setStandardsDict] = useState<Record<string, VoucherStandardDefinition>>({});
    const [isReconcilingL2, setIsReconcilingL2] = useState(false);
    const [hasPublishedCard, setHasPublishedCard] = useState(false);
    const [sendCardOverride, setSendCardOverride] = useState<boolean | null>(null);

    const { selectedMap, toggleVoucher, setVoucherAmount, selectAmount, clearSelection, selectionStats, currentPlan, autoSelectGravitational } = useVoucherSelection(availableVouchers);
    const detectedPrivacy = usePrivacyDetection(
        Array.from(selectedMap.keys()),
        availableVouchers,
        standardsDict
    );

    useEffect(() => {
        let active = true;
        async function fetchData() {
            try {
                const [summaries, userId, profile, standards, settings, fetchedContacts, activeClasses, marketProfile] = await Promise.all([
                    voucherService.getSummaries(),
                    profileService.getUserId(),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards(),
                    settingsService.getSettings(),
                    contactService.getContacts(),
                    transferService.getActiveAssetClasses(),
                    marketService.getMyMarketProfile().catch(() => null)
                ]);

                if (!active) return;

                setHasPublishedCard(Boolean(marketProfile?.published));

                const activeVouchers = summaries.filter((v: VoucherSummary) => {
                    const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
                    return statusName.toLowerCase() === 'active';
                });

                const defaultName =
                    formatProfileDisplayName(marketProfile?.profile) ||
                    formatProfileDisplayName(profile);

                if (!active) return;
                dispatch({ type: 'SET_SENDER_NAME', name: defaultName });

                const newIdMap = new Map<string, string>();
                const newPrecisionMap = new Map<string, number>();
                const parsedDict: Record<string, VoucherStandardDefinition> = {};

                for (const s of standards) {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        newIdMap.set(s.id, uuid);
                        newPrecisionMap.set(uuid, getPrecision(s.content));
                    }
                }

                await Promise.all(
                    standards.map(async (s) => {
                        try {
                            const def = await standardsService.parseStandard(s.content);
                            if (def) {
                                const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                                const uuid = def.immutable?.identity?.uuid || (uuidMatch && uuidMatch[1]) || s.id;
                                parsedDict[uuid] = def;
                            }
                        } catch (err) {
                            logger.warn(`Failed to parse standard definition ${s.id}: ${stringifyError(err)}`);
                        }
                    })
                );
                
                if (!active) return;
                const resolvedVouchers = activeVouchers.map((v: VoucherSummary) => {
                    const stdDef = parsedDict[v.voucherStandardUuid];
                    const allowPartial = stdDef?.immutable?.features?.allowPartialTransfers;
                    return typeof allowPartial === 'boolean' ? { ...v, allowPartialTransfers: allowPartial } : v;
                });
                setStandardsDict(parsedDict);
                setStandardIdToUuidMap(newIdMap);
                setUuidToPrecisionMap(newPrecisionMap);
                setAvailableVouchers(resolvedVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                setAppSettings(settings);
                setContacts(fetchedContacts);
                setActiveAssetClasses(activeClasses || []);
                if (initialVoucherId) {
                    const targetVoucher = resolvedVouchers.find((v: VoucherSummary) => v.localInstanceId === initialVoucherId);
                    if (targetVoucher) {
                        const stdId = [...newIdMap.entries()].find(([, uuid]) => uuid === targetVoucher.voucherStandardUuid)?.[0] || targetVoucher.voucherStandardUuid;
                        dispatch({ type: 'SET_STANDARD', id: stdId, isTest: targetVoucher.isTestVoucher });
                        dispatch({ type: 'SET_TARGET_AMOUNT', amount: targetVoucher.currentAmount });
                        toggleVoucher(targetVoucher.localInstanceId, targetVoucher.currentAmount);
                    }
                } else if (activeClasses && activeClasses.length > 0) {
                    const firstClass = activeClasses.find((c: AssetClassSummary) => !c.isTestVoucher) || activeClasses[0];
                    const stdId = [...newIdMap.entries()].find(([, uuid]) => uuid === firstClass.standardUuid)?.[0] || firstClass.standardUuid;
                    dispatch({ type: 'SET_STANDARD', id: stdId, isTest: firstClass.isTestVoucher, preserveAmount: true });
                }
                setIsDataLoaded(true);
            } catch (e) {
                if (!active) return;
                logger.error(`Failed to fetch data for SendView: ${stringifyError(e)}`);
                dispatch({ type: 'SET_FEEDBACK', msg: `Error: ${translateError(e, t)}` });
            }
        }
        fetchData();
        return () => { active = false; };
    }, [profileName, t, initialVoucherId, toggleVoucher]);

    useEffect(() => {
        if (detectedPrivacy === 'stealth') dispatch({ type: 'SET_PRIVACY', mode: 'stealth' });
        else if (detectedPrivacy === 'public') dispatch({ type: 'SET_PRIVACY', mode: 'public' });
        else if (detectedPrivacy === 'flexible' && state.privacyMode === null && appSettings) {
            const defaultMode = appSettings.privacyDefault === 'stealth' ? 'stealth' : (appSettings.privacyDefault === 'public' ? 'public' : null);
            dispatch({ type: 'SET_PRIVACY', mode: defaultMode });
        }
    }, [detectedPrivacy, appSettings, state.privacyMode]);

    useEffect(() => {
        let active = true;
        if (state.recipientId.length < 10) {
            setTrustStatus("clean");
            return;
        }
        const timer = setTimeout(async () => {
            try {
                const status = await voucherService.checkReputation(state.recipientId);
                if (active) setTrustStatus(status);
            } catch (e) {
                logger.error(`Reputation check failed: ${stringifyError(e)}`);
            }
        }, 500);
        return () => {
            active = false;
            clearTimeout(timer);
        };
    }, [state.recipientId]);

    const filteredVouchers = useMemo(() => {
        if (!state.selectedStandardId) return availableVouchers;
        const selectedStandardUuid = standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
        if (!selectedStandardUuid) return [];
        return availableVouchers.filter(v => {
            if (v.voucherStandardUuid !== selectedStandardUuid) return false;
            if (state.selectedIsTest !== null && v.isTestVoucher !== state.selectedIsTest) return false;
            return true;
        });
    }, [availableVouchers, state.selectedStandardId, state.selectedIsTest, standardIdToUuidMap]);

    // Gravitational auto-selection: bei gültigem Betrag optimale Auswahl (inkl. WoT-Scoring) automatisch berechnen
    useEffect(() => {
        if (preserveInitialVoucherRef.current) return;
        let active = true;
        const amountStr = state.targetAmountStr.trim().replace(',', '.');
        const recipient = state.recipientId.trim();
        const amt = parseFloat(amountStr);
        if (isNaN(amt) || amt <= 0) return;
        if (!state.selectedStandardId) return;
        const standardUuid = standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
        if (!standardUuid) return;
        const voucher = filteredVouchers[0];
        const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4) : 4;
        const timer = setTimeout(() => {
            autoSelectGravitational(amountStr, recipient, standardUuid, precision, state.selectedIsTest).catch((e) => {
                if (!active) return;
                logger.error(`Gravitational auto-select failed: ${stringifyError(e)}`);
                // Fallback to legacy greedy selection on error
                const val = parseFloat(amountStr);
                if (!isNaN(val) && val > 0 && voucher) {
                    selectAmount(val, voucher.displayCurrency, precision, state.selectedIsTest, standardUuid);
                }
            });
        }, 400);
        return () => {
            active = false;
            clearTimeout(timer);
        };
    }, [state.targetAmountStr, state.recipientId, state.selectedStandardId, state.selectedIsTest, filteredVouchers, standardIdToUuidMap, uuidToPrecisionMap, autoSelectGravitational, selectAmount]);

    const handleTargetAmountChange = (valStr: string) => {
        preserveInitialVoucherRef.current = false;
        dispatch({ type: 'SET_TARGET_AMOUNT', amount: valStr });
        let effectiveStandardId = state.selectedStandardId;
        let effectiveIsTest = state.selectedIsTest;

        if (!effectiveStandardId && activeAssetClasses.length > 0) {
            const preferredClass = activeAssetClasses.find((c: AssetClassSummary) => !c.isTestVoucher) || activeAssetClasses[0];
            effectiveStandardId = [...standardIdToUuidMap.entries()].find(([, uuid]) => uuid === preferredClass.standardUuid)?.[0] || preferredClass.standardUuid;
            effectiveIsTest = preferredClass.isTestVoucher;
            dispatch({ type: 'SET_STANDARD', id: effectiveStandardId, isTest: effectiveIsTest, preserveAmount: true });
        }

        // Auto-selection triggers via effect when recipient is entered;
        // fallback for missing/incomplete recipient: immediate local selection keeping Test/Real isolated
        if (state.recipientId.trim().length < 10) {
            const val = parseFloat(valStr.replace(',', '.'));
            const effectiveStandardUuid = effectiveStandardId
                ? (standardIdToUuidMap.get(effectiveStandardId) || effectiveStandardId)
                : null;
            const matchingVoucher = availableVouchers.find(v =>
                (!effectiveStandardUuid || v.voucherStandardUuid === effectiveStandardUuid) &&
                (effectiveIsTest === null || v.isTestVoucher === effectiveIsTest)
            );
            if (!isNaN(val) && val > 0 && matchingVoucher) {
                const precision = uuidToPrecisionMap.get(matchingVoucher.voucherStandardUuid) ?? 4;
                selectAmount(val, matchingVoucher.displayCurrency, precision, effectiveIsTest, effectiveStandardUuid);
            }
        }
    };

    const applyPaymentRequest = useCallback((rawText: string): boolean => {
        const parsed = parsePaymentRequest(rawText);
        if (!parsed) return false;

        dispatch({ type: 'SET_FEEDBACK', msg: '' });
        dispatch({ type: 'SET_RECIPIENT', id: parsed.to });

        if (parsed.note) {
            dispatch({ type: 'SET_NOTES', notes: parsed.note });
        }

        let matchedStandardId: string | null = null;
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
        if (parsed.standardUuid) {
            for (const [id, uuid] of standardIdToUuidMap.entries()) {
                if (uuid === parsed.standardUuid || id === parsed.standardUuid) {
                    matchedStandardId = id;
                    break;
                }
            }
            if (!matchedStandardId) {
                const isStrictUuid = uuidPattern.test(parsed.standardUuid);
                const foundByIdOrName = voucherStandards.find(
                    s =>
                        s.id === parsed.standardUuid ||
                        (!isStrictUuid &&
                            s.displayName?.toLowerCase() === parsed.standardUuid?.toLowerCase())
                );
                if (foundByIdOrName) matchedStandardId = foundByIdOrName.id;
            }
        } else if (parsed.standardName) {
            const found = voucherStandards.find(s => s.displayName?.toLowerCase() === parsed.standardName?.toLowerCase());
            if (found) matchedStandardId = found.id;
        }

        const requestedStandard = Boolean(parsed.standardUuid || parsed.standardName);
        if (matchedStandardId) {
            dispatch({ type: 'SET_STANDARD', id: matchedStandardId, isTest: parsed.isTest ?? false });
        } else if (requestedStandard) {
            setPaymentRequestSuccessMsg(null);
            dispatch({
                type: 'SET_FEEDBACK',
                msg: t('transfer.paymentRequestStandardMissing', {
                    name: parsed.standardName || parsed.standardUuid,
                }),
            });
            return true;
        } else if (parsed.isTest !== undefined && state.selectedStandardId) {
            dispatch({ type: 'SET_STANDARD', id: state.selectedStandardId, isTest: parsed.isTest });
        }

        if (parsed.amount) {
            dispatch({ type: 'SET_TARGET_AMOUNT', amount: parsed.amount });
        }

        setPaymentRequestSuccessMsg(t('transfer.paymentRequestApplied'));
        setTimeout(() => setPaymentRequestSuccessMsg(null), 3500);
        return true;
    }, [standardIdToUuidMap, voucherStandards, state.selectedStandardId, t]);

    useEffect(() => {
        if (!isDataLoaded || !initialPaymentUri) return;
        if (appliedInitialUriRef.current === initialPaymentUri) return;
        appliedInitialUriRef.current = initialPaymentUri;
        applyPaymentRequest(initialPaymentUri);
    }, [isDataLoaded, initialPaymentUri, applyPaymentRequest]);

    const handlePastePaymentRequest = async () => {
        try {
            const { text, error } = await readFromClipboard();
            if (error) {
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.clipboardReadFailed') });
                return;
            }
            if (!text.trim()) {
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.emptyClipboard') });
                return;
            }
            handleApplyRawCode(text);
        } catch (e) {
            logger.error(`Failed to read clipboard for payment request: ${stringifyError(e)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.clipboardReadFailed') });
        }
    };

    const handleApplyRawCode = (rawText: string): boolean => {
        const trimmed = rawText.trim();
        if (!trimmed) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.emptyClipboard') });
            return false;
        }
        if (trimmed.toLowerCase().includes('humoco://pay')) {
            const ok = applyPaymentRequest(trimmed);
            if (!ok) {
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
            }
            return ok;
        }
        const parsedCard = parseCardUri(trimmed);
        if (parsedCard?.envelopeJson) {
            try {
                const env = JSON.parse(parsedCard.envelopeJson) as Record<string, unknown>;
                let cardDid = typeof env.did === 'string' ? env.did : '';
                if (!cardDid && typeof env.card_wire === 'string') {
                    const seg = env.card_wire.split('.')[1];
                    if (seg) {
                        const b64 = seg.replace(/-/g, '+').replace(/_/g, '/');
                        const payload = JSON.parse(window.atob(b64)) as { did?: string };
                        if (typeof payload.did === 'string') cardDid = payload.did;
                    }
                }
                if (cardDid) {
                    dispatch({ type: 'SET_FEEDBACK', msg: '' });
                    dispatch({ type: 'SET_RECIPIENT', id: cardDid.replace(/\s+/g, '') });
                    return true;
                }
            } catch {
                // Fall through to parseContactCard
            }
        }
        const parsedContact = parseContactCard(trimmed);
        if (parsedContact?.did) {
            dispatch({ type: 'SET_FEEDBACK', msg: '' });
            dispatch({ type: 'SET_RECIPIENT', id: parsedContact.did.replace(/\s+/g, '') });
            return true;
        }
        dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
        return false;
    };

    const handleVoucherToggle = (v: VoucherSummary) => {
        preserveInitialVoucherRef.current = false;
        toggleVoucher(v.localInstanceId, v.currentAmount);
    };

    const handleAmountChange = (id: string, amt: string) => {
        preserveInitialVoucherRef.current = false;
        setVoucherAmount(id, amt);
    };

    const handlePrepareTransfer = (e: FormEvent) => {
        e.preventDefault();
        if (accessMode === 'receiveOnly') {
            if (requireFullAccess) {
                void requireFullAccess(async () => {}).catch(() => undefined);
            } else {
                dispatch({ type: 'SET_FEEDBACK', msg: t('auth.cashierModeRestrictedDesc') });
            }
            return;
        }
        if (appSettings?.pendingRestoreL2Reconciliation) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.pendingL2ReconciliationBlocked', 'Ausgaben sind vorübergehend gesperrt, bis der Online-Sicherheitsabgleich durchgeführt wurde.') });
            return;
        }
        if (!state.recipientId) {
            dispatch({ type: 'SET_RECIPIENT_ERROR', value: true });
            const targetEl =
                document.getElementById('recipientId') ||
                document.querySelector<HTMLElement>('[data-testid="recipient-search-input"]');
            targetEl?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        if (selectedMap.size === 0) { 
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.assetSelectionRequired') }); 
            return; 
        }
        if (detectedPrivacy === 'incompatible') {
            dispatch({ type: 'SET_PRIVACY_ERROR', value: true });
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.privacyConflict') });
            document.getElementById('privacyMode')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        if (detectedPrivacy === 'flexible' && state.privacyMode === null) {
            dispatch({ type: 'SET_PRIVACY_ERROR', value: true });
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.privacyRequired') });
            document.getElementById('privacyMode')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        dispatch({ type: 'TOGGLE_CONFIRM', value: true });
    };

    const executeTransfer = async () => {
        dispatch({ type: 'SET_PROCESSING', value: true });
        try {
            const trimmedName = state.customSenderName.trim();
            const senderName = state.sendProfileName && trimmedName.length > 0 ? trimmedName : null;
            const sources: SourceTransfer[] = Array.from(selectedMap.entries()).map(([id, amount]) => ({
                localInstanceId: id,
                amountToSend: amount,
            }));
            
            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(s => {
                const uuid = standardIdToUuidMap.get(s.id) || s.id;
                standardDefinitionsToml[uuid] = s.content;
            });

            const summableAmounts: Record<string, string> = {};
            const countableItems: Record<string, number> = {};

            Object.entries(selectionStats).forEach(([displayCurrency, stat]) => {
                const voucher = availableVouchers.find(v => v.displayCurrency === displayCurrency);
                const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                summableAmounts[displayCurrency] = stat.total.toFixed(precision);
            });

            const txOutcome = await protectAction(async (password) => {
                const bundleResult: CreateBundleResult = await transferService.createBundle({
                    recipientId: state.recipientId,
                    sources,
                    notes: state.notes || null,
                    senderProfileName: senderName,
                    standardDefinitionsToml,
                    usePrivacyMode: state.privacyMode === 'stealth',
                    sendCardOverride: sendCardOverride ?? undefined,
                    password
                });

                const record: TransactionRecord = {
                    id: crypto.randomUUID(),
                    direction: 'sent',
                    recipientId: state.recipientId,
                    senderId: ownUserId,
                    timestamp: new Date().toISOString(),
                    summableAmounts,
                    countableItems,
                    involvedVouchers: (bundleResult.involvedSourcesDetails as InvolvedVoucherInfo[]).map((d: InvolvedVoucherInfo) => d.localInstanceId),
                    involvedSourcesDetails: bundleResult.involvedSourcesDetails as InvolvedVoucherInfo[],
                    bundleData: bundleResult.bundleData,
                    bundleId: bundleResult.bundleId,
                    notes: state.notes || undefined,
                    senderProfileName: senderName || undefined,
                };

                try {
                    await transferService.saveTransactionRecord(record, password || undefined);
                } catch (saveErr) {
                    logger.error(`Failed to persist transaction record after transfer commit: ${stringifyError(saveErr)}`);
                }

                let depositedTo = 0;
                let depositError: string | undefined = undefined;
                try {
                    const depositRes = await postService.depositTransferToPost(bundleResult.bundleId, password || undefined);
                    depositedTo = depositRes.depositedTo;
                    if (depositRes.errors && depositRes.errors.length > 0) {
                        depositError = depositRes.errors.join('; ');
                        logger.warn(`Post deposit completed with ${depositRes.errors.length} error(s)`);
                    }
                    // Point G: Fast/prioritized sync after transaction send
                    runPostSyncPass().catch(() => {});
                } catch (err) {
                    depositError = translateError(err, t);
                    logger.warn(`Post deposit failed after transfer commit: ${stringifyError(err)}`);
                }

                return { bundleResult, depositedTo, depositError };
            });

            if (!txOutcome) { return; }
            const { bundleResult, depositedTo, depositError } = txOutcome;

            const summaryString = Object.entries(selectionStats)
                .map(([displayCurrency, stat]) => `${formatAmount(stat.total.toString())} ${displayCurrency}`)
                .join(', ');

            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferSuccess') });
            navigationTimerRef.current = setTimeout(() => navigate({
                view: 'transfer_success',
                bundleData: bundleResult.bundleData,
                recipientId: state.recipientId,
                summary: summaryString,
                l2Dispatch: bundleResult.l2Dispatch,
                bundleId: bundleResult.bundleId,
                depositedTo,
                depositError,
            }), 1500);
        } catch (e) {
            logger.error(`Transfer failed: ${stringifyError(e)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferFailed', { error: translateError(e, t) }) });
            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
        } finally {
            dispatch({ type: 'SET_PROCESSING', value: false });
        }
    };

    return (
        <PageLayout 
            title={t('transfer.title')} 
            description={t('transfer.description')} 
            onBack={goBack}
            advancedActions={
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={handlePastePaymentRequest}
                    title={t('transfer.pastePaymentRequestHint')}
                    className="gap-2 text-xs shadow-sm"
                >
                    <Clipboard size={16} className="shrink-0" />
                    <span>{t('transfer.pastePaymentRequest')}</span>
                </Button>
            }
        >
            <div className="max-w-xl mx-auto space-y-5 pb-16">
                {paymentRequestSuccessMsg && (
                    <AlertBanner variant="success">
                        {paymentRequestSuccessMsg}
                    </AlertBanner>
                )}

                {accessMode === 'receiveOnly' && (
                    <AlertBanner variant="warning" title={t('auth.cashierModeRestrictedTitle')}>
                        <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
                            <span>{t('auth.cashierModeRestrictedDesc')}</span>
                            {requireFullAccess && (
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="xs"
                                    onClick={() => {
                                        void requireFullAccess(async () => {}).catch(() => undefined);
                                    }}
                                >
                                    {t('auth.unlockFullAccess')}
                                </Button>
                            )}
                        </div>
                    </AlertBanner>
                )}

                {state.feedbackMsg && (
                    <AlertBanner variant="error">
                        {state.feedbackMsg}
                    </AlertBanner>
                )}

                {appSettings?.pendingRestoreL2Reconciliation && (
                    <div className="p-4 rounded-2xl bg-amber-50 border border-amber-200 text-amber-900 text-xs space-y-2 mb-4">
                        <div className="flex items-center gap-2 font-bold text-amber-800">
                            <AlertTriangle size={16} className="shrink-0" />
                            <span>{t('transfer.pendingL2ReconciliationTitle', 'Online-Sicherheitsabgleich erforderlich')}</span>
                        </div>
                        <p className="leading-relaxed">
                            {t('transfer.pendingL2ReconciliationDesc', 'Nach einer Wiederherstellung aus der Sicherung ist vor dem Ausgeben von Gutscheinen ein einmaliger Online-Sicherheitsabgleich erforderlich, um SST-V4-Konflikte auszuschließen.')}
                        </p>
                        <Button
                            variant="secondary"
                            size="sm"
                            disabled={isReconcilingL2}
                            onClick={async () => {
                                setIsReconcilingL2(true);
                                try {
                                    await protectAction(async (password) => {
                                        const res = await backupService.reconcileRestoredWalletL2(password || undefined);
                                        if (res.l2ReconciliationStatus === 'reconciled') {
                                            setAppSettings((prev) => prev ? { ...prev, pendingRestoreL2Reconciliation: false } : prev);
                                        }
                                    });
                                } catch (err) {
                                    logger.error(`Reconcile failed: ${stringifyError(err)}`);
                                    dispatch({ type: 'SET_FEEDBACK', msg: translateError(err, t) });
                                } finally {
                                    setIsReconcilingL2(false);
                                }
                            }}
                            className="w-full mt-1"
                        >
                            <RefreshCw size={14} className={isReconcilingL2 ? "animate-spin mr-2" : "mr-2"} />
                            {isReconcilingL2 ? t('profile.backupRestoreReconcilingL2', 'Gleiche mit Layer 2 ab...') : t('profile.backupRestoreReconcileL2Btn', 'Jetzt online abgleichen')}
                        </Button>
                    </div>
                )}

                <div className="space-y-5">
                    {/* Absender (kompakt aus Profil/Visitenkarte, erst per Stift editierbar) */}
                    {!isEditingSender ? (
                        <div className="flex items-center justify-between gap-3 bg-white border border-theme-subtle rounded-2xl px-4 py-3 shadow-xs">
                            <div className="flex items-center gap-3 min-w-0">
                                <UserAvatar
                                    size={28}
                                    name={state.sendProfileName ? (state.customSenderName || ownUserId || "sender") : "anonymous"}
                                />
                                <div className="min-w-0 flex items-baseline gap-2 flex-wrap">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-light shrink-0">
                                        {t('transfer.senderNameLabel')}:
                                    </span>
                                    <span className="text-sm font-bold text-theme-secondary truncate">
                                        {state.sendProfileName
                                            ? (state.customSenderName.trim() || t('transfer.noSenderNameProvided'))
                                            : t('transfer.anonymous')}
                                    </span>
                                </div>
                            </div>
                            <div className="flex items-center gap-1 shrink-0">
                                <HelpIcon topic="senderName" size={14} />
                                <button
                                    type="button"
                                    data-testid="edit-sender-name-btn"
                                    onClick={() => setIsEditingSender(true)}
                                    aria-label={t('common.edit')}
                                    title={t('common.edit')}
                                    className="p-1.5 text-theme-light hover:text-theme-primary hover:bg-theme-primary/5 rounded-xl transition-colors cursor-pointer"
                                >
                                    <Edit3 size={15} />
                                </button>
                            </div>
                        </div>
                    ) : (
                        <div className="bg-white border border-theme-primary/30 rounded-2xl p-4 shadow-xs space-y-3">
                            <div className="flex items-center justify-between gap-2 flex-wrap">
                                <div className="flex items-center gap-1.5">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-primary">
                                        {t('transfer.senderNameLabel')}
                                    </span>
                                    <HelpIcon topic="senderName" size={14} />
                                </div>
                                <div className="flex items-center gap-3">
                                    <label className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-theme-light hover:text-theme-secondary cursor-pointer select-none">
                                        <input
                                            type="checkbox"
                                            checked={!state.sendProfileName}
                                            onChange={(e) => dispatch({ type: 'TOGGLE_PROFILE_NAME', value: !e.target.checked })}
                                            className="rounded border-slate-300 accent-theme-primary h-3.5 w-3.5 cursor-pointer"
                                        />
                                        <span>{t('transfer.sendAnonymously')}</span>
                                    </label>
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() => setIsEditingSender(false)}
                                        data-testid="save-sender-name-btn"
                                    >
                                        {t('common.done')}
                                    </Button>
                                </div>
                            </div>
                            {state.sendProfileName ? (
                                <div className="flex items-center gap-3">
                                    <UserAvatar size={30} name={state.customSenderName || ownUserId || "sender"} />
                                    <Input
                                        value={state.customSenderName}
                                        onChange={(e) => dispatch({ type: 'SET_SENDER_NAME', name: e.target.value })}
                                        placeholder={t('transfer.senderNamePlaceholder')}
                                        className="bg-white border-slate-200 text-sm font-bold rounded-xl py-2 px-3"
                                    />
                                </div>
                            ) : (
                                <div className="flex items-center gap-2.5 py-1.5 px-3 bg-slate-50 border border-slate-200/60 rounded-xl text-xs text-slate-500">
                                    <Shield size={15} className="text-slate-400 shrink-0" />
                                    <span>{t('transfer.anonymousHint')}</span>
                                </div>
                            )}
                            <Advanced>
                                {ownUserId && <TechId value={ownUserId} kind="did" />}
                            </Advanced>
                        </div>
                    )}

                    {/* Empfänger (To) */}
                    <RecipientSelector
                        recipientId={state.recipientId}
                        onRecipientChange={(e) => {
                            const rawVal = e.target.value;
                            if (rawVal.toLowerCase().includes('humoco://pay')) {
                                const ok = applyPaymentRequest(rawVal);
                                if (!ok) {
                                    dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
                                }
                                return;
                            }
                            const val = rawVal.replace(/\s+/g, '');
                            e.target.value = val;
                            dispatch({ type: 'SET_RECIPIENT', id: val });
                        }}
                        onApplyRawCode={handleApplyRawCode}
                        onSelectContact={(c) => dispatch({ type: 'SET_RECIPIENT', id: c.did })}
                        contacts={contacts}
                        trustStatus={trustStatus}
                        recipientError={state.recipientError}
                        onClearRecipient={() => dispatch({ type: 'SET_RECIPIENT', id: "" })}
                    />

                    {/* Betrag, Währung & Auto-Auswahl */}
                    <AssetInventory
                        filteredVouchers={filteredVouchers}
                        activeAssetClasses={activeAssetClasses}
                        selectedStandardId={state.selectedStandardId}
                        selectedIsTest={state.selectedIsTest}
                        onStandardSelect={(id, isTest) => {
                            preserveInitialVoucherRef.current = false;
                            if (isTest !== null && selectedMap.size > 0) {
                                const hasOppositeRealm = Array.from(selectedMap.keys()).some(vId => {
                                    const found = availableVouchers.find(v => v.localInstanceId === vId);
                                    return found && Boolean(found.isTestVoucher) !== Boolean(isTest);
                                });
                                if (hasOppositeRealm) {
                                    clearSelection();
                                }
                            }
                            dispatch({ type: 'SET_STANDARD', id, isTest });
                        }}
                        targetAmountStr={state.targetAmountStr}
                        onTargetAmountChange={handleTargetAmountChange}
                        selection={selectedMap}
                        onVoucherToggle={handleVoucherToggle}
                        onAmountChange={handleAmountChange}
                        uuidToPrecisionMap={uuidToPrecisionMap}
                        standardIdToUuidMap={standardIdToUuidMap}
                        currentPlan={currentPlan}
                        recipientSelected={state.recipientId.trim().length >= 10}
                        onCreateVoucher={() => navigate({ view: 'create_voucher' })}
                        onReceiveVoucher={() => navigate({ view: 'receive_bundle' })}
                    />

                    {/* Verwendungszweck (kompakt) */}
                    <div className="bg-white border border-theme-subtle rounded-2xl px-4 py-3 shadow-xs space-y-1.5">
                        <label htmlFor="transfer-note-input" className="text-[10px] font-black text-theme-light uppercase tracking-widest block">
                            {t('transfer.transactionNote')}
                        </label>
                        <Input
                            id="transfer-note-input"
                            value={state.notes}
                            onChange={(e) => dispatch({ type: 'SET_NOTES', notes: e.target.value })}
                            placeholder={t('transfer.notePlaceholder')}
                            aria-label={t('transfer.transactionNote')}
                            className="rounded-xl text-sm"
                        />
                    </div>

                    {/* Weitere Optionen (auch im Experten-Modus initial eingeklappt) */}
                    <Disclosure
                        defaultOpen={false}
                        open={state.privacyError ? true : undefined}
                        title={t('transfer.moreOptionsTitle')}
                        indicatorDot={sendCardOverride !== null}
                        toggleTestId="send-more-options-toggle"
                        className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50"
                        buttonClassName="w-full flex items-center justify-between gap-3 px-4 py-3 bg-slate-50/70 hover:bg-slate-100/70 transition-colors text-left cursor-pointer text-xs font-bold text-theme-secondary"
                        contentClassName="p-4 border-t border-theme-subtle bg-white"
                    >
                        <div className="space-y-4">
                            <PrivacyToggle
                                privacyMode={state.privacyMode}
                                onPrivacyChange={(mode) => dispatch({ type: 'SET_PRIVACY', mode })}
                                privacyRules={{
                                    mode: detectedPrivacy,
                                    forced: detectedPrivacy === 'stealth' || detectedPrivacy === 'public',
                                }}
                                privacyError={state.privacyError}
                            />

                            {hasPublishedCard ? (
                                <div className="flex items-start gap-3 p-3.5 bg-white border border-theme-subtle rounded-2xl">
                                    <input
                                        type="checkbox"
                                        id="sendCardToggle"
                                        checked={sendCardOverride !== null ? sendCardOverride : (appSettings?.marketSendCardOnPayment ?? false)}
                                        onChange={(e) => setSendCardOverride(e.target.checked)}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div className="flex-1">
                                        <label htmlFor="sendCardToggle" className="text-xs font-bold text-theme-secondary block cursor-pointer">
                                            {t('transfer.sendCardTitle')}
                                        </label>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('transfer.sendCardDesc')}
                                        </p>
                                    </div>
                                </div>
                            ) : (
                                <div className="p-3 bg-slate-50 border border-theme-subtle/60 rounded-2xl text-[11px] text-theme-light flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                                    <span>{t('transfer.sendCardNoCardNotice')}</span>
                                    <button
                                        type="button"
                                        onClick={() => navigate({ view: 'marketplace', initialTab: 'mine', initialOpenEditor: true })}
                                        className="text-xs text-theme-primary hover:underline shrink-0 self-start sm:self-auto p-0 h-auto font-medium"
                                    >
                                        {t('transfer.openMarketplace')}
                                    </button>
                                </div>
                            )}
                        </div>
                    </Disclosure>

                    {/* Schlanke Senden-Leiste */}
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 bg-white border border-theme-subtle rounded-3xl p-4 shadow-sm">
                        <div className="min-w-0">
                            <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                                {t('transfer.disbursementSummary')}
                            </p>
                            <div className="text-xl font-black tracking-tighter text-theme-primary flex flex-wrap gap-x-3 gap-y-0.5">
                                {selectedMap.size > 0 ? (
                                    Object.entries(selectionStats).map(([unit, stat]) => {
                                        const voucher = availableVouchers.find(v => v.displayCurrency === unit);
                                        const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                                        return (
                                            <span key={unit} className="inline-flex items-baseline gap-1">
                                                {formatAmount(stat.total.toString(), precision)}
                                                <span className="text-xs font-bold uppercase tracking-widest text-theme-light">{unit}</span>
                                            </span>
                                        );
                                    })
                                ) : (
                                    <span className="text-theme-light">0.00</span>
                                )}
                            </div>
                        </div>
                        <Button
                            type="button"
                            variant="primary"
                            onClick={handlePrepareTransfer}
                            disabled={state.isProcessing}
                            className="gap-2 rounded-2xl px-6 py-3 font-black text-xs uppercase tracking-widest shrink-0"
                        >
                            <Send size={15} />
                            <span>{state.isProcessing ? t('transfer.sending') : t('transfer.sendVoucher')}</span>
                        </Button>
                    </div>
                </div>
            </div>

            <ConfirmationModal 
                isOpen={state.showConfirm}
                title={t('transfer.confirmTitle')}
                description={(() => {
                    const matched = contacts.find(c => c.did === state.recipientId);
                    const matchedName = matched
                        ? [`${matched.profile.firstName || ''}`, `${matched.profile.lastName || ''}`].join(' ').trim() || matched.profile.organization
                        : undefined;
                    return (
                        <div className="space-y-4 pt-2 text-left">
                            <div className="flex items-center gap-3 p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                                <UserAvatar size={40} pictureUrl={matched?.profile.pictureUrl} name={matchedName || state.recipientId} />
                                <div className="min-w-0 flex-1 space-y-1">
                                    <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.confirmRecipient')}</p>
                                    {matchedName ? (
                                        <>
                                            <p className="text-sm font-black text-theme-secondary truncate">{matchedName}</p>
                                            <Advanced>
                                                <TechId value={state.recipientId} kind="did" />
                                            </Advanced>
                                        </>
                                    ) : (
                                        <TechId value={state.recipientId} kind="did" />
                                    )}
                                </div>
                            </div>
                            <div className="p-4 bg-slate-50 rounded-2xl border border-slate-100">
                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mb-2">{t('transfer.confirmAssets')}</p>
                                <div className="text-lg font-black tracking-tighter text-theme-primary">
                                    {Object.entries(selectionStats).map(([displayCurrency, stat]) => `${formatAmount(stat.total.toString())} ${displayCurrency}`).join(', ')}
                                </div>
                            </div>
                            <Advanced>
                                <div className="flex items-center gap-2 px-1">
                                    <Shield size={14} className={state.privacyMode === 'stealth' ? "text-purple-500" : "text-blue-500"}/>
                                    <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                                        {t('transfer.confirmPrivacy')} <span className={state.privacyMode === 'stealth' ? "text-purple-600" : "text-blue-600"}>{state.privacyMode === 'stealth' ? t('transfer.stealth') : t('transfer.public')}</span>
                                    </p>
                                </div>
                            </Advanced>
                        </div>
                    );
                })()}
                confirmText={t('transfer.confirmExecute')}
                onConfirm={executeTransfer}
                onCancel={() => dispatch({ type: 'TOGGLE_CONFIRM', value: false })}
            />
        </PageLayout>
    );
}
