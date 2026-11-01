import { useState, useEffect, useMemo, FormEvent, useReducer, useRef } from "react";
import { useTranslation } from 'react-i18next';
import { voucherService } from "../services/voucherService";
import { transferService, CreateBundleResult } from "../services/transferService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
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
import { Textarea } from "./ui/Textarea";
import { Card } from "./ui/Card";
import { HelpIcon } from "./ui/HelpIcon";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { PageLayout } from "./ui/PageLayout";
import { AlertBanner } from "./ui/AlertBanner";
import { ModalShell } from "./ui/ModalShell";
import { formatAmount } from "../utils/format";
import { readFromClipboard } from "../utils/clipboard";
import { parsePaymentRequest } from "../utils/humocoUri";
import { 
    Shield, 
    BookOpen,
    UserCheck,
    Search,
    X,
    Clipboard
} from "lucide-react";
import Avatar from "boring-avatars";

// Modular Components
import { RecipientSelector } from "./send/RecipientSelector";
import { PrivacyToggle } from "./send/PrivacyToggle";
import { AssetInventory } from "./send/AssetInventory";
import { TransferSummaryBar } from "./send/TransferSummaryBar";

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
    | { type: 'SET_STANDARD'; id: string | null; isTest: boolean | null }
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
        case 'SET_STANDARD': return { ...state, selectedStandardId: action.id, selectedIsTest: action.isTest, targetAmountStr: "" };
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

export interface SendViewProps {
    initialRecipientId?: string;
    initialVoucherId?: string;
    initialNote?: string;
}

export function SendView({ initialRecipientId, initialVoucherId, initialNote }: SendViewProps = {}) {
    const { t } = useTranslation();
    const { protectAction, profileName } = useSession();
    const { navigate, goBack } = useNavigation();
    const [state, dispatch] = useReducer(reducer, initialState);
    const [paymentRequestSuccessMsg, setPaymentRequestSuccessMsg] = useState<string | null>(null);
    
    const navigationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        if (initialRecipientId) {
            dispatch({ type: 'SET_RECIPIENT', id: initialRecipientId });
        }
        if (initialNote) {
            dispatch({ type: 'SET_NOTES', notes: initialNote });
        }
    }, [initialRecipientId, initialNote]);

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
    const [showContactPicker, setShowContactPicker] = useState(false);
    const [contactSearchQuery, setContactSearchQuery] = useState("");
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());
    const [uuidToPrecisionMap, setUuidToPrecisionMap] = useState<Map<string, number>>(new Map());
    const [standardsDict, setStandardsDict] = useState<Record<string, VoucherStandardDefinition>>({});

    const { selectedMap, toggleVoucher, setVoucherAmount, selectAmount, selectionStats, currentPlan, autoSelectGravitational } = useVoucherSelection(availableVouchers);
    const detectedPrivacy = usePrivacyDetection(
        Array.from(selectedMap.keys()),
        availableVouchers,
        standardsDict
    );

    useEffect(() => {
        async function fetchData() {
            try {
                const [summaries, userId, profile, standards, settings, fetchedContacts, activeClasses] = await Promise.all([
                    voucherService.getSummaries(),
                    profileService.getUserId(),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards(),
                    settingsService.getSettings(),
                    contactService.getContacts(),
                    transferService.getActiveAssetClasses()
                ]);

                const activeVouchers = summaries.filter((v: VoucherSummary) => {
                    const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
                    return statusName.toLowerCase() === 'active';
                });

                let defaultName = "";
                if (profile) {
                    const nameParts = [];
                    if (profile.firstName) nameParts.push(profile.firstName);
                    if (profile.lastName) nameParts.push(profile.lastName);
                    const fullName = nameParts.join(" ").trim();

                    if (fullName) {
                        defaultName = fullName;
                    } else if (profile.organization) {
                        defaultName = profile.organization;
                    }

                    if (profile.community) {
                        if (defaultName) {
                            defaultName = `${defaultName} (${profile.community})`;
                        } else {
                            defaultName = profile.community;
                        }
                    }
                }

                if (defaultName.startsWith("did:key:")) {
                    defaultName = "";
                }

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
                            const parseToml = (voucherService as unknown as { parseStandardToml?: (toml: string) => Promise<VoucherStandardDefinition> }).parseStandardToml
                                ? (voucherService as unknown as { parseStandardToml: (toml: string) => Promise<VoucherStandardDefinition> }).parseStandardToml
                                : standardsService.parseStandard;
                            const def = await parseToml(s.content);
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
                
                setStandardsDict(parsedDict);
                setStandardIdToUuidMap(newIdMap);
                setUuidToPrecisionMap(newPrecisionMap);
                setAvailableVouchers(activeVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                setAppSettings(settings);
                setContacts(fetchedContacts);
                setActiveAssetClasses(activeClasses);
                if (initialVoucherId) {
                    const targetVoucher = activeVouchers.find((v: VoucherSummary) => v.localInstanceId === initialVoucherId);
                    if (targetVoucher) {
                        const stdId = [...newIdMap.entries()].find(([, uuid]) => uuid === targetVoucher.voucherStandardUuid)?.[0] || targetVoucher.voucherStandardUuid;
                        dispatch({ type: 'SET_STANDARD', id: stdId, isTest: targetVoucher.isTestVoucher });
                        dispatch({ type: 'SET_TARGET_AMOUNT', amount: targetVoucher.currentAmount });
                        toggleVoucher(targetVoucher.localInstanceId, targetVoucher.currentAmount);
                    }
                } else if (activeClasses.length > 0) {
                    const firstClass = activeClasses[0];
                    const stdId = [...newIdMap.entries()].find(([, uuid]) => uuid === firstClass.standardUuid)?.[0] || firstClass.standardUuid;
                    dispatch({ type: 'SET_STANDARD', id: stdId, isTest: firstClass.isTestVoucher });
                }
            } catch (e) {
                logger.error(`Failed to fetch data for SendView: ${stringifyError(e)}`);
                dispatch({ type: 'SET_FEEDBACK', msg: `Error: ${translateError(e, t)}` });
            }
        }
        fetchData();
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
        if (state.recipientId.length < 10) {
            setTrustStatus("clean");
            return;
        }
        const timer = setTimeout(async () => {
            try {
                const status = await voucherService.checkReputation(state.recipientId);
                setTrustStatus(status);
            } catch (e) {
                logger.error(`Reputation check failed: ${stringifyError(e)}`);
            }
        }, 500);
        return () => clearTimeout(timer);
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

    // Gravitational auto-selection: bei gültigem Empfänger und Betrag optimale Auswahl automatisch vornehmen
    useEffect(() => {
        const amountStr = state.targetAmountStr.trim();
        const recipient = state.recipientId.trim();
        const amt = parseFloat(amountStr);
        if (!recipient || recipient.length < 10 || isNaN(amt) || amt <= 0) return;
        if (!state.selectedStandardId) return;
        const standardUuid = standardIdToUuidMap.get(state.selectedStandardId) || state.selectedStandardId;
        if (!standardUuid) return;
        const voucher = filteredVouchers[0];
        const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4) : 4;
        const timer = setTimeout(() => {
            autoSelectGravitational(amountStr, recipient, standardUuid, precision).catch((e) => {
                logger.error(`Gravitational auto-select failed: ${stringifyError(e)}`);
                // Fallback to legacy greedy selection on error
                const val = parseFloat(amountStr);
                if (!isNaN(val) && val > 0 && voucher) {
                    selectAmount(val, voucher.displayCurrency, precision);
                }
            });
        }, 400);
        return () => clearTimeout(timer);
    }, [state.targetAmountStr, state.recipientId, state.selectedStandardId, state.selectedIsTest, filteredVouchers, standardIdToUuidMap, uuidToPrecisionMap, autoSelectGravitational, selectAmount]);

    const handleTargetAmountChange = (valStr: string) => {
        dispatch({ type: 'SET_TARGET_AMOUNT', amount: valStr });
        // Auto-selection übernimmt die Auswahl via Effekt; kein direkter selectAmount-Aufruf nötig.
        // Fallback für fehlenden Empfänger: lokale Greedy-Auswahl sofort, damit UI nicht leer wirkt
        if (state.recipientId.trim().length < 10) {
            const val = parseFloat(valStr);
            if (!isNaN(val) && val > 0 && filteredVouchers[0]) {
                const voucher = filteredVouchers[0];
                const precision = uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4;
                selectAmount(val, voucher.displayCurrency, precision);
            }
        }
    };

    const applyPaymentRequest = (rawText: string): boolean => {
        const parsed = parsePaymentRequest(rawText);
        if (!parsed) return false;

        dispatch({ type: 'SET_FEEDBACK', msg: '' });
        dispatch({ type: 'SET_RECIPIENT', id: parsed.to });

        if (parsed.note) {
            dispatch({ type: 'SET_NOTES', notes: parsed.note });
        }

        let matchedStandardId: string | null = null;
        if (parsed.standardUuid) {
            for (const [id, uuid] of standardIdToUuidMap.entries()) {
                if (uuid === parsed.standardUuid || id === parsed.standardUuid) {
                    matchedStandardId = id;
                    break;
                }
            }
        } else if (parsed.standardName) {
            const found = voucherStandards.find(s => s.displayName?.toLowerCase() === parsed.standardName?.toLowerCase());
            if (found) matchedStandardId = found.id;
        }

        if (matchedStandardId) {
            dispatch({ type: 'SET_STANDARD', id: matchedStandardId, isTest: parsed.isTest ?? false });
        } else if (parsed.isTest !== undefined && state.selectedStandardId) {
            dispatch({ type: 'SET_STANDARD', id: state.selectedStandardId, isTest: parsed.isTest });
        }

        if (parsed.amount) {
            dispatch({ type: 'SET_TARGET_AMOUNT', amount: parsed.amount });
        }

        setPaymentRequestSuccessMsg(t('transfer.paymentRequestApplied'));
        setTimeout(() => setPaymentRequestSuccessMsg(null), 3500);
        return true;
    };

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
            const success = applyPaymentRequest(text);
            if (!success) {
                dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.invalidPaymentRequest') });
            }
        } catch (e) {
            logger.error(`Failed to read clipboard for payment request: ${stringifyError(e)}`);
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.clipboardReadFailed') });
        }
    };

    const handleVoucherToggle = (v: VoucherSummary) => {
        toggleVoucher(v.localInstanceId, v.currentAmount);
    };

    const handleAmountChange = (id: string, amt: string) => {
        setVoucherAmount(id, amt);
    };

    const handlePrepareTransfer = (e: FormEvent) => {
        e.preventDefault();
        if (!state.recipientId) { 
            dispatch({ type: 'SET_RECIPIENT_ERROR', value: true }); 
            document.getElementById('recipientId')?.scrollIntoView({ behavior: 'smooth', block: 'center' }); 
            return; 
        }
        if (selectedMap.size === 0) { 
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.assetSelectionRequired') }); 
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

            const bundleResult = await protectAction(async (password) => {
                return await transferService.createBundle({
                    recipientId: state.recipientId,
                    sources,
                    notes: state.notes || null,
                    senderProfileName: senderName,
                    standardDefinitionsToml,
                    usePrivacyMode: state.privacyMode === 'stealth',
                    password
                });
            }) as CreateBundleResult;

            if (!bundleResult) { dispatch({ type: 'SET_PROCESSING', value: false }); return; }

            const summableAmounts: Record<string, string> = {};
            const countableItems: Record<string, number> = {};
            
            Object.entries(selectionStats).forEach(([displayCurrency, stat]) => {
                const voucher = availableVouchers.find(v => v.displayCurrency === displayCurrency);
                const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                summableAmounts[displayCurrency] = stat.total.toFixed(precision);
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

            await protectAction(async (password) => {
                 await transferService.saveTransactionRecord(record, password || undefined);
            });

            const summaryString = Object.entries(selectionStats)
                .map(([displayCurrency, stat]) => `${formatAmount(stat.total.toString())} ${displayCurrency}`)
                .join(', ');

            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferSuccess') });
            navigationTimerRef.current = setTimeout(() => navigate({ view: 'transfer_success', bundleData: bundleResult.bundleData, recipientId: state.recipientId, summary: summaryString, l2Dispatch: bundleResult.l2Dispatch }), 1500);
        } catch (e) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferFailed', { error: translateError(e, t) }) });
            dispatch({ type: 'SET_PROCESSING', value: false });
            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
        }
    };

    return (
        <PageLayout 
            title={t('transfer.title')} 
            description={t('transfer.description')} 
            onBack={goBack}
            actions={
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
            <div className="max-w-5xl mx-auto space-y-8 pb-32">
                {paymentRequestSuccessMsg && (
                    <AlertBanner variant="success">
                        {paymentRequestSuccessMsg}
                    </AlertBanner>
                )}

                {state.feedbackMsg && (
                    <AlertBanner variant="error">
                        {state.feedbackMsg}
                    </AlertBanner>
                )}

                <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
                    <div className="lg:col-span-7 space-y-6">
                        {/* Absender (From) */}
                        <Card header={
                            <div className="flex items-center justify-between gap-2 w-full">
                                <div className="flex items-center gap-2">
                                    <UserCheck size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.senderNameLabel')}</span>
                                    <HelpIcon topic="senderName" size={14} />
                                </div>
                                <label className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-theme-light hover:text-theme-secondary cursor-pointer select-none">
                                    <input
                                        type="checkbox"
                                        checked={!state.sendProfileName}
                                        onChange={(e) => dispatch({ type: 'TOGGLE_PROFILE_NAME', value: !e.target.checked })}
                                        className="rounded border-slate-300 accent-theme-primary h-3.5 w-3.5 cursor-pointer"
                                    />
                                    <span>{t('transfer.sendAnonymously')}</span>
                                </label>
                            </div>
                        }>
                            {state.sendProfileName ? (
                                <div className="space-y-1.5">
                                    <div className="flex items-center gap-3">
                                        <div className="shrink-0">
                                            <Avatar size={32} name={ownUserId || "sender"} variant="beam" />
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <Input 
                                                value={state.customSenderName} 
                                                onChange={(e) => dispatch({ type: 'SET_SENDER_NAME', name: e.target.value })} 
                                                placeholder={t('transfer.senderNamePlaceholder')} 
                                                className="bg-white border-slate-200 text-sm font-bold rounded-xl py-2 px-3" 
                                            />
                                        </div>
                                    </div>
                                    {ownUserId && (
                                        <p className="text-[10px] font-mono text-slate-400 truncate pl-11">
                                            {ownUserId}
                                        </p>
                                    )}
                                </div>
                            ) : (
                                <div className="flex items-center gap-2.5 py-1.5 px-3 bg-slate-50 border border-slate-200/60 rounded-xl text-xs text-slate-500">
                                    <Shield size={15} className="text-slate-400 shrink-0" />
                                    <span>{t('transfer.anonymousHint')}</span>
                                </div>
                            )}
                        </Card>

                        {/* Visual Flow Connector */}
                        <div className="flex justify-center -my-3 relative z-10">
                            <div className="w-0.5 h-6 border-l-2 border-dashed border-slate-300 dark:border-slate-700 flex items-center justify-center">
                                <div className="bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-400 rounded-full p-1 shadow-sm">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><polyline points="19 12 12 19 5 12"></polyline></svg>
                                </div>
                            </div>
                        </div>

                        {/* Empfänger (To) */}
                        <RecipientSelector 
                            recipientId={state.recipientId}
                            onRecipientChange={(e) => {
                                const rawVal = e.target.value;
                                if (rawVal.includes('humoco://pay')) {
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
                            onSelectContact={(c) => dispatch({ type: 'SET_RECIPIENT', id: c.did })}
                            contacts={contacts}
                            trustStatus={trustStatus}
                            recipientError={state.recipientError}
                            onShowContactPicker={() => setShowContactPicker(true)}
                            onClearRecipient={() => dispatch({ type: 'SET_RECIPIENT', id: "" })}
                        />

                        {/* Verwendungszweck (Note) */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <BookOpen size={18} className="text-theme-primary"/>
                                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.context')}</span>
                            </div>
                        }>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">{t('transfer.transactionNote')}</label>
                                <Textarea 
                                    value={state.notes} 
                                    onChange={(e) => dispatch({ type: 'SET_NOTES', notes: e.target.value })} 
                                    placeholder={t('transfer.notePlaceholder')} 
                                    className="rounded-2xl min-h-[80px]" 
                                />
                            </div>
                        </Card>

                        {/* Privatsphäre & Sicherheit */}
                        <PrivacyToggle 
                            privacyMode={state.privacyMode}
                            onPrivacyChange={(mode) => dispatch({ type: 'SET_PRIVACY', mode })}
                            privacyRules={{ mode: detectedPrivacy, forced: detectedPrivacy !== 'flexible' }}
                            privacyError={state.privacyError}
                        />
                    </div>

                    <div className="lg:col-span-5 space-y-8">
                        <AssetInventory 
                            filteredVouchers={filteredVouchers}
                            activeAssetClasses={activeAssetClasses}
                            selectedStandardId={state.selectedStandardId}
                            selectedIsTest={state.selectedIsTest}
                            onStandardSelect={(id, isTest) => dispatch({ type: 'SET_STANDARD', id, isTest })}
                            targetAmountStr={state.targetAmountStr}
                            onTargetAmountChange={handleTargetAmountChange}
                            selection={selectedMap}
                            onVoucherToggle={handleVoucherToggle}
                            onAmountChange={handleAmountChange}
                            uuidToPrecisionMap={uuidToPrecisionMap}
                            standardIdToUuidMap={standardIdToUuidMap}
                        />

                        {currentPlan && (
                            <Card className="p-5 bg-amber-50/50 border border-amber-200/60 rounded-[32px] space-y-3 animate-in fade-in duration-300">
                                <div className="flex items-center justify-between">
                                    <h4 className="text-[10px] font-black text-amber-700 uppercase tracking-widest">{t('transfer.planTitle')}</h4>
                                    {currentPlan.needsSplit && <span className="text-[10px] font-bold text-amber-600 bg-amber-100 px-2 py-1 rounded-full">{t('transfer.planSplitRequired')}</span>}
                                </div>
                                <div className="grid grid-cols-3 gap-2 text-xs">
                                    <div className="bg-white rounded-xl p-2 border border-amber-100">
                                        <p className="text-[9px] font-black uppercase tracking-widest text-slate-400">Ziel</p>
                                        <p className="font-black text-slate-800">{formatAmount(currentPlan.target)} {filteredVouchers[0]?.displayCurrency ?? ''}</p>
                                    </div>
                                    <div className="bg-white rounded-xl p-2 border border-amber-100">
                                        <p className="text-[9px] font-black uppercase tracking-widest text-slate-400">Ausgewählt</p>
                                        <p className="font-black text-slate-800">{formatAmount(currentPlan.totalSelected)} {filteredVouchers[0]?.displayCurrency ?? ''}</p>
                                    </div>
                                    <div className={`rounded-xl p-2 border ${currentPlan.needsSplit ? 'bg-white border-amber-200' : 'bg-slate-50 border-slate-100'}`}>
                                        <p className="text-[9px] font-black uppercase tracking-widest text-slate-400">Wechselgeld</p>
                                        <p className={`font-black ${currentPlan.needsSplit ? 'text-amber-700' : 'text-slate-400'}`}>
                                            {(() => {
                                                const total = parseFloat(currentPlan.totalSelected);
                                                const target = parseFloat(currentPlan.target);
                                                const diff = total - target;
                                                const precision = filteredVouchers[0] ? (uuidToPrecisionMap.get(filteredVouchers[0].voucherStandardUuid) ?? 4) : 4;
                                                return formatAmount(diff.toFixed(precision));
                                            })()} {filteredVouchers[0]?.displayCurrency ?? ''}
                                        </p>
                                    </div>
                                </div>
                                <div className="space-y-1">
                                    <p className="text-[9px] font-black uppercase tracking-widest text-slate-400">{t('transfer.planInvolvedVouchers')}</p>
                                    <div className="space-y-1 max-h-32 overflow-auto pr-1">
                                        {currentPlan.selected.map((rv) => {
                                            const voucher = filteredVouchers.find(v => v.localInstanceId === rv.localInstanceId);
                                            const label = voucher?.displayStandardName ? `${voucher.displayStandardName} (${rv.localInstanceId.slice(0, 6)}…)` : `${rv.localInstanceId.slice(0, 8)}…`;
                                            const currency = voucher?.displayCurrency || filteredVouchers[0]?.displayCurrency || '';
                                            return (
                                                <div key={rv.localInstanceId} className="flex justify-between items-center bg-white rounded-lg px-2 py-1 border border-slate-100 text-[11px]">
                                                    <span className="font-mono text-slate-600 truncate max-w-[160px]">{label}</span>
                                                    <span className="font-bold text-slate-700">{formatAmount(rv.amount)} {currency}</span>
                                                </div>
                                            );
                                        })}
                                    </div>
                                </div>
                                {currentPlan.needsSplit && (
                                    <p className="text-[10px] font-medium text-amber-800 bg-amber-100/70 p-2.5 rounded-xl border border-amber-200/50 leading-relaxed">
                                        {t('transfer.splitReassurance')}
                                    </p>
                                )}
                            </Card>
                        )}

                        {selectedMap.size > 0 && (
                            <Card className="p-5 bg-slate-50 border border-slate-200/60 rounded-[32px] space-y-4 animate-in fade-in duration-300">
                                <h4 className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.confirmTitle')} - {t('common.preview')}</h4>
                                <div className="divide-y divide-slate-100 text-xs font-bold text-slate-700">
                                    <div className="flex justify-between py-2.5">
                                        <span className="text-slate-400">{t('transfer.senderNameLabel')}</span>
                                        <span className="text-slate-900">{state.sendProfileName ? (state.customSenderName.trim() || t('transfer.noSenderNameProvided')) : t('transfer.anonymous')}</span>
                                    </div>
                                    <div className="flex justify-between py-2.5">
                                        <span className="text-slate-400">{t('transfer.recipient')}</span>
                                        <span className="font-mono text-[10px] text-slate-900 truncate max-w-[200px]">
                                            {state.recipientId ? (
                                                contacts.find(c => c.did === state.recipientId)
                                                    ? `${contacts.find(c => c.did === state.recipientId)?.profile.firstName || ''} ${contacts.find(c => c.did === state.recipientId)?.profile.lastName || ''}`.trim() || contacts.find(c => c.did === state.recipientId)?.profile.organization || state.recipientId.slice(0, 8) + "..." + state.recipientId.slice(-8)
                                                    : state.recipientId.slice(0, 8) + "..." + state.recipientId.slice(-8)
                                            ) : '-'}
                                        </span>
                                    </div>
                                    <div className="flex justify-between py-2.5">
                                        <span className="text-slate-400">{t('transfer.securityPrivacy')}</span>
                                        <span className={state.privacyMode === 'stealth' ? 'text-purple-600' : 'text-blue-600'}>
                                            {state.privacyMode === 'stealth' ? t('transfer.stealth') : (state.privacyMode === 'public' ? t('transfer.public') : '-')}
                                        </span>
                                    </div>
                                </div>
                            </Card>
                        )}

                        <TransferSummaryBar 
                            checkoutSummary={{
                                count: selectedMap.size,
                                summableTotals: Object.fromEntries(Object.entries(selectionStats).map(([u, s]) => [u, s.total])),
                                countableTotals: {}
                            }}
                            selectionSize={selectedMap.size}
                            isProcessing={state.isProcessing}
                            onPrepareTransfer={handlePrepareTransfer}
                            availableVouchers={availableVouchers}
                            uuidToPrecisionMap={uuidToPrecisionMap}
                        />
                    </div>
                </div>
            </div>

            <ConfirmationModal 
                isOpen={state.showConfirm}
                title={t('transfer.confirmTitle')}
                description={
                    <div className="space-y-4 pt-2 text-left">
                        <div className="flex items-center gap-3 p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                            <Avatar size={40} name={state.recipientId} variant="beam" />
                            <div className="min-w-0">
                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.confirmRecipient')}</p>
                                <p className="text-sm font-black text-theme-secondary truncate">{state.recipientId}</p>
                            </div>
                        </div>
                        <div className="p-4 bg-slate-50 rounded-2xl border border-slate-100">
                            <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mb-2">{t('transfer.confirmAssets')}</p>
                            <div className="text-lg font-black tracking-tighter text-theme-primary">
                                {Object.entries(selectionStats).map(([displayCurrency, stat]) => `${formatAmount(stat.total.toString())} ${displayCurrency}`).join(', ')}
                            </div>
                        </div>
                        <div className="flex items-center gap-2 px-1">
                            <Shield size={14} className={state.privacyMode === 'stealth' ? "text-purple-500" : "text-blue-500"}/>
                            <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                                {t('transfer.confirmPrivacy')} <span className={state.privacyMode === 'stealth' ? "text-purple-600" : "text-blue-600"}>{state.privacyMode === 'stealth' ? t('transfer.stealth') : t('transfer.public')}</span>
                            </p>
                        </div>
                    </div>
                }
                confirmText={t('transfer.confirmExecute')}
                onConfirm={executeTransfer}
                onCancel={() => dispatch({ type: 'TOGGLE_CONFIRM', value: false })}
            />

            {/* Contact Picker Modal */}
            <ModalShell
                isOpen={showContactPicker}
                onClose={() => { setShowContactPicker(false); setContactSearchQuery(""); }}
                maxWidth="md"
                title={
                    <div className="flex items-center justify-between pb-2 border-b border-theme-subtle">
                        <h3 className="text-base font-black text-theme-secondary uppercase tracking-wider">
                            {t('contacts.selectContactTitle')}
                        </h3>
                        <button
                            type="button"
                            onClick={() => { setShowContactPicker(false); setContactSearchQuery(""); }}
                            title={t('common.close')}
                            aria-label={t('common.close')}
                            className="p-2 hover:bg-slate-100 rounded-xl text-slate-400 hover:text-slate-600 transition-colors cursor-pointer"
                        >
                            <X size={20} />
                        </button>
                    </div>
                }
            >
                <div className="py-2">
                    <div className="relative">
                        <Input
                            value={contactSearchQuery}
                            onChange={(e) => setContactSearchQuery(e.target.value)}
                            placeholder={t('contacts.searchPlaceholder', 'Search contacts...')}
                            className="pl-10 text-xs rounded-2xl"
                            autoFocus
                        />
                        <Search size={16} className="absolute left-3.5 top-1/2 -translate-y-1/2 text-theme-light" />
                    </div>
                </div>

                <div className="space-y-2 pr-1">
                    {(() => {
                        const q = contactSearchQuery.toLowerCase().trim();
                        const filtered = contacts.filter(c =>
                            !q ||
                            c.did.toLowerCase().includes(q) ||
                            (c.profile.firstName || '').toLowerCase().includes(q) ||
                            (c.profile.lastName || '').toLowerCase().includes(q) ||
                            (c.profile.organization || '').toLowerCase().includes(q)
                        );
                        if (filtered.length === 0) {
                            return (
                                <div className="text-center py-8 text-theme-light text-xs font-medium">
                                    {contacts.length === 0 ? t('contacts.emptyContacts') : t('contacts.noContactsFound')}
                                </div>
                            );
                        }
                        return filtered.map(c => {
                            const name = [(c.profile.firstName || ''), (c.profile.lastName || '')].join(' ').trim() || c.profile.organization || t('transfer.anonymous');
                            return (
                                <button
                                    key={c.did}
                                    type="button"
                                    onClick={() => {
                                        dispatch({ type: 'SET_RECIPIENT', id: c.did });
                                        setShowContactPicker(false);
                                        setContactSearchQuery("");
                                    }}
                                    className="w-full flex items-center gap-3.5 p-3 hover:bg-theme-primary/5 rounded-2xl text-left border border-transparent hover:border-theme-primary/20 transition-all cursor-pointer"
                                >
                                    <Avatar size={36} name={c.did} variant="beam" />
                                    <div className="min-w-0 flex-1">
                                        <p className="text-sm font-bold text-theme-secondary truncate">{name}</p>
                                        <p className="text-[10px] font-mono text-theme-light truncate">{c.did}</p>
                                    </div>
                                </button>
                            );
                        });
                    })()}
                </div>
            </ModalShell>
        </PageLayout>
    );
}
