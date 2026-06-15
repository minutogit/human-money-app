import { useState, useEffect, useMemo, FormEvent, useReducer, useRef } from "react";
import { useTranslation } from 'react-i18next';
import { voucherService } from "../services/voucherService";
import { transferService, CreateBundleResult } from "../services/transferService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
import { logger } from "../utils/log";
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
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";
import { Card } from "./ui/Card";
import { HelpIcon } from "./ui/HelpIcon";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { PageLayout } from "./ui/PageLayout";
import { formatAmount } from "../utils/format";
import { 
    Shield, 
    BookOpen,
    AlertTriangle,
    UserCheck
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
        case 'SET_RECIPIENT': return { ...state, recipientId: action.id, recipientError: false };
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

export function SendView() {
    const { t } = useTranslation();
    const { protectAction, profileName } = useSession();
    const { navigate, goBack } = useNavigation();
    const [state, dispatch] = useReducer(reducer, initialState);
    
    const navigationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
    const [standardsDict] = useState<Record<string, VoucherStandardDefinition>>({});

    const { selectedMap, toggleVoucher, setVoucherAmount, selectAmount, selectionStats } = useVoucherSelection(availableVouchers);
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

                for (const s of standards) {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        newIdMap.set(s.id, uuid);
                        newPrecisionMap.set(uuid, getPrecision(s.content));
                    }
                }
                
                setStandardIdToUuidMap(newIdMap);
                setUuidToPrecisionMap(newPrecisionMap);
                setAvailableVouchers(activeVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                setAppSettings(settings);
                setContacts(fetchedContacts);
                setActiveAssetClasses(activeClasses);
            } catch (e) {
                logger.error(`Failed to fetch data for SendView: ${e}`);
                dispatch({ type: 'SET_FEEDBACK', msg: `Error: ${e}` });
            }
        }
        fetchData();
    }, [profileName]);

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
                logger.error(`Reputation check failed: ${e}`);
            }
        }, 500);
        return () => clearTimeout(timer);
    }, [state.recipientId]);

    const filteredVouchers = useMemo(() => {
        if (!state.selectedStandardId) return availableVouchers;
        const selectedStandardUuid = standardIdToUuidMap.get(state.selectedStandardId);
        if (!selectedStandardUuid) return [];
        return availableVouchers.filter(v => {
            if (v.voucherStandardUuid !== selectedStandardUuid) return false;
            if (state.selectedIsTest !== null && v.isTestVoucher !== state.selectedIsTest) return false;
            return true;
        });
    }, [availableVouchers, state.selectedStandardId, state.selectedIsTest, standardIdToUuidMap]);

    const handleTargetAmountChange = (valStr: string) => {
        dispatch({ type: 'SET_TARGET_AMOUNT', amount: valStr });
        const val = parseFloat(valStr);
        if (isNaN(val) || val <= 0) return;
        
        const voucher = filteredVouchers[0];
        const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4) : 4;
        selectAmount(val, voucher?.displayCurrency, precision);
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
            const senderName = state.sendProfileName ? state.customSenderName : null;
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
                    senderProfileName: senderName || t('transfer.anonymous'),
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
            navigationTimerRef.current = setTimeout(() => navigate({ view: 'transfer_success', bundleData: bundleResult.bundleData, recipientId: state.recipientId, summary: summaryString }), 1500);
        } catch (e) {
            dispatch({ type: 'SET_FEEDBACK', msg: t('transfer.transferFailed', { error: `${e}` }) });
            dispatch({ type: 'SET_PROCESSING', value: false });
            dispatch({ type: 'TOGGLE_CONFIRM', value: false });
        }
    };

    return (
        <PageLayout title={t('transfer.title')} description={t('transfer.description')} onBack={goBack}>
            <div className="max-w-5xl mx-auto space-y-8 pb-32">
                {state.feedbackMsg && (
                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in slide-in-from-top-2">
                        <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                        <p className="text-sm font-bold text-rose-800 leading-tight">{state.feedbackMsg}</p>
                    </div>
                )}

                <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
                    <div className="lg:col-span-7 space-y-6">
                        {/* Absender (From) */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <UserCheck size={18} className="text-theme-primary"/>
                                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.from')}</span>
                            </div>
                        }>
                            <div className="space-y-6">
                                <div className="flex items-center justify-between p-4 bg-slate-50 border border-slate-200/60 rounded-3xl gap-4">
                                    <div className="flex items-center gap-4 min-w-0">
                                        <div className="relative shrink-0">
                                            {state.sendProfileName ? (
                                                <Avatar size={40} name={ownUserId || "anonymous"} variant="beam" />
                                            ) : (
                                                <div className="w-10 h-10 rounded-full bg-slate-200 flex items-center justify-center text-slate-400">
                                                    <Shield size={20} />
                                                </div>
                                            )}
                                        </div>
                                        <div className="min-w-0">
                                            <h4 className="text-sm font-black text-slate-800 truncate">
                                                {state.sendProfileName ? (state.customSenderName || t('transfer.anonymous')) : t('transfer.anonymous')}
                                            </h4>
                                            <p className="text-[9px] font-mono text-slate-400 truncate tracking-tight">{ownUserId || t('common.loading')}</p>
                                        </div>
                                    </div>
                                    <div className="shrink-0 flex items-center gap-2">
                                        <label className="flex items-center gap-3 cursor-pointer group">
                                            <div className={`w-10 h-6 rounded-full transition-all relative ${state.sendProfileName ? 'bg-theme-primary shadow-inner' : 'bg-slate-300'}`}>
                                                <div className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-all shadow-md ${state.sendProfileName ? 'left-5' : 'left-1'}`}></div>
                                                <input type="checkbox" className="hidden" checked={state.sendProfileName} onChange={(e) => dispatch({ type: 'TOGGLE_PROFILE_NAME', value: e.target.checked })} />
                                            </div>
                                        </label>
                                        <HelpIcon topic="senderName" />
                                    </div>
                                </div>

                                {state.sendProfileName && (
                                    <div className="space-y-2 animate-in slide-in-from-top-2 duration-200">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.displayName')}</label>
                                        <Input 
                                            value={state.customSenderName} 
                                            onChange={(e) => dispatch({ type: 'SET_SENDER_NAME', name: e.target.value })} 
                                            placeholder={t('transfer.displayNamePlaceholder')} 
                                            className="bg-white border-slate-200 text-sm font-bold rounded-2xl py-3 px-4" 
                                        />
                                    </div>
                                )}
                            </div>
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
                            onRecipientChange={(e) => dispatch({ type: 'SET_RECIPIENT', id: e.target.value })}
                            onSelectContact={(c) => dispatch({ type: 'SET_RECIPIENT', id: c.did })}
                            contacts={contacts}
                            trustStatus={trustStatus}
                            recipientError={state.recipientError}
                            onShowContactPicker={() => {}} // TODO
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

                        {selectedMap.size > 0 && (
                            <Card className="p-5 bg-slate-50 border border-slate-200/60 rounded-[32px] space-y-4 animate-in fade-in duration-300">
                                <h4 className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.confirmTitle')} - Vorschau</h4>
                                <div className="divide-y divide-slate-100 text-xs font-bold text-slate-700">
                                    <div className="flex justify-between py-2.5">
                                        <span className="text-slate-400">{t('transfer.from')}</span>
                                        <span className="text-slate-900">{state.sendProfileName ? (state.customSenderName || t('transfer.anonymous')) : t('transfer.anonymous')}</span>
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
        </PageLayout>
    );
}
