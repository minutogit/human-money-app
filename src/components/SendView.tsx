// src/components/SendView.tsx
import { useState, useEffect, useMemo, FormEvent, ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { 
    VoucherSummary, 
    VoucherStandardInfo, 
    SourceTransfer, 
    TransactionRecord, 
    Contact, 
    TrustStatus, 
    AppSettings,
    AssetClassSummary
} from "../types";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";
import { Card } from "./ui/Card";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { ContactBadge } from "./ui/ContactBadge";
import Avatar from "boring-avatars";
import { PageLayout } from "./ui/PageLayout";
import { 
    User, 
    Send, 
    Shield, 
    Lock, 
    Eye, 
    AlertTriangle, 
    CheckCircle2, 
    BookOpen, 
    Search, 
    Filter, 
    CreditCard, 
    Info, 
    X,
    UserPlus,
    ArrowRight,
    Coins
} from "lucide-react";

interface SendViewProps {
    onBack: () => void;
    onTransferPrepared: (bundleData: number[], recipientId: string, summary: string) => void;
    profileName: string | null;
}

function formatDate(isoString: string): string {
    if (!isoString) return 'N/A';
    return new Date(isoString).toLocaleDateString(undefined, {
        year: 'numeric', month: 'short', day: 'numeric'
    });
}

function getPrecision(content: string): number {
    const match = content.match(/amount_decimal_places\s*=\s*(\d+)/i);
    return match && match[1] ? parseInt(match[1], 10) : 4;
}

function formatAmount(amountStr: string, precision: number = 2): string {
    const num = parseFloat(amountStr);
    if (isNaN(num)) return amountStr;
    return num.toLocaleString(undefined, { 
        minimumFractionDigits: precision, 
        maximumFractionDigits: precision 
    });
}

export function SendView({ onBack, onTransferPrepared, profileName }: SendViewProps) {
    const { protectAction } = useSession();
    const [recipientId, setRecipientId] = useState("");
    const [notes, setNotes] = useState("");
    const [sendProfileName, setSendProfileName] = useState(true);
    const [customSenderName, setCustomSenderName] = useState("");
    const [ownUserId, setOwnUserId] = useState("");
    const [targetAmountStr, setTargetAmountStr] = useState("");
    const [availableVouchers, setAvailableVouchers] = useState<VoucherSummary[]>([]);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string | null>(null);
    const [selectedIsTest, setSelectedIsTest] = useState<boolean | null>(null);
    const [activeAssetClasses, setActiveAssetClasses] = useState<AssetClassSummary[]>([]);
    const [selection, setSelection] = useState<Map<string, string>>(new Map());
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isProcessing, setIsProcessing] = useState(false);
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());
    const [uuidToPrecisionMap, setUuidToPrecisionMap] = useState<Map<string, number>>(new Map());
    const [showConfirm, setShowConfirm] = useState(false);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");
    const [privacyMode, setPrivacyMode] = useState<'public' | 'stealth' | null>(null);
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [suggestions, setSuggestions] = useState<Contact[]>([]);
    const [showSuggestions, setShowSuggestions] = useState(false);
    const [showContactPicker, setShowContactPicker] = useState(false);
    const [recipientError, setRecipientError] = useState(false);
    const [privacyError, setPrivacyError] = useState(false);

    useEffect(() => {
        if (recipientError) {
            const timer = setTimeout(() => setRecipientError(false), 2000);
            return () => clearTimeout(timer);
        }
    }, [recipientError]);

    useEffect(() => {
        if (privacyError) {
            const timer = setTimeout(() => setPrivacyError(false), 2000);
            return () => clearTimeout(timer);
        }
    }, [privacyError]);

    useEffect(() => {
        if (privacyRules.mode === 'Flexible') {
            setPrivacyMode(null);
        }
    }, [selection]);

    useEffect(() => {
        if (recipientId.length < 10) {
            setTrustStatus("clean");
            return;
        }

        const timer = setTimeout(async () => {
            try {
                const status = await invoke<TrustStatus>("check_reputation", { offenderId: recipientId });
                setTrustStatus(status);
            } catch (e) {
                logger.error(`Reputation check failed: ${e}`);
            }
        }, 500);

        return () => clearTimeout(timer);
    }, [recipientId]);

    useEffect(() => {
        async function fetchData() {
            logger.info("SendView component displayed. Fetching initial data...");
            try {
                const allFetchedVouchers = await invoke<VoucherSummary[]>("get_voucher_summaries");
                const activeVouchers = allFetchedVouchers.filter(v => {
                    const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
                    return statusName.toLowerCase() === 'active';
                });

                const userId = await invoke<string>("get_user_id");
                
                try {
                    const userProfile = await invoke<any>("get_user_profile");
                    let defaultName = "";
                    if (userProfile.organization) {
                        defaultName = userProfile.organization;
                    } else if (userProfile.firstName || userProfile.lastName) {
                        defaultName = `${userProfile.firstName || ''} ${userProfile.lastName || ''}`.trim();
                    } else {
                        defaultName = profileName || "";
                    }
                    setCustomSenderName(defaultName);
                } catch (e) {
                    setCustomSenderName(profileName || "");
                }

                const standards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                const settings = await invoke<AppSettings>("get_app_settings");
                const newIdMap = new Map<string, string>();
                const newPrecisionMap = new Map<string, number>();
                standards.forEach(s => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        newIdMap.set(s.id, uuid);
                        newPrecisionMap.set(uuid, getPrecision(s.content));
                    }
                });
                setStandardIdToUuidMap(newIdMap);
                setUuidToPrecisionMap(newPrecisionMap);

                const enrichedVouchers = activeVouchers.map(v => ({ ...v, divisible: true }));

                setAvailableVouchers(enrichedVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                setAppSettings(settings);
                
                const fetchedContacts = await invoke<Contact[]>("get_contacts");
                setContacts(fetchedContacts);

                const activeClasses = await invoke<AssetClassSummary[]>("get_active_asset_classes");
                setActiveAssetClasses(activeClasses);
            } catch (e) {
                logger.error(`Failed to fetch data for SendView: ${e}`);
                setFeedbackMsg(`Error: ${e}`);
            }
        }
        fetchData();
    }, []);

    const filteredVouchers = useMemo(() => {
        if (!selectedStandardId) return availableVouchers;
        const selectedStandardUuid = standardIdToUuidMap.get(selectedStandardId);
        if (!selectedStandardUuid) return [];
        return availableVouchers.filter(v => {
            if (v.voucherStandardUuid !== selectedStandardUuid) return false;
            if (selectedIsTest !== null && v.isTestVoucher !== selectedIsTest) return false;
            return true;
        });
    }, [availableVouchers, selectedStandardId, selectedIsTest, standardIdToUuidMap]);


    const handleTargetAmountChange = (valStr: string) => {
        if (!selectedStandardId || !valStr) {
            setTargetAmountStr(valStr);
            setSelection(new Map());
            return;
        }

        let targetAmount = parseFloat(valStr);
        if (isNaN(targetAmount) || targetAmount <= 0) {
            setTargetAmountStr(valStr);
            setSelection(new Map());
            return;
        }

        const selectedStandardUuid = standardIdToUuidMap.get(selectedStandardId) || selectedStandardId;
        const precision = uuidToPrecisionMap.get(selectedStandardUuid) ?? 4;

        let maxPossibleAmount = 0;
        filteredVouchers.forEach(v => {
            const amt = parseFloat(v.currentAmount);
            if (!isNaN(amt)) maxPossibleAmount += amt;
        });
        
        maxPossibleAmount = parseFloat(maxPossibleAmount.toFixed(precision));

        let finalValStr = valStr;
        if (targetAmount > maxPossibleAmount && maxPossibleAmount > 0) {
            targetAmount = maxPossibleAmount;
            finalValStr = targetAmount.toString();
        }
        
        setTargetAmountStr(finalValStr);

        const newSelection = new Map<string, string>();
        let currentTotal = 0;

        const sortedVouchers = [...filteredVouchers].sort((a, b) => parseFloat(a.currentAmount) - parseFloat(b.currentAmount));
        for (const voucher of sortedVouchers) {
            const voucherAmount = parseFloat(voucher.currentAmount);
            if (currentTotal + voucherAmount <= targetAmount) {
                newSelection.set(voucher.localInstanceId, voucher.currentAmount);
                currentTotal += voucherAmount;
            } else {
                const neededAmount = targetAmount - currentTotal;
                // @ts-ignore
                if (voucher.divisible) {
                    newSelection.set(voucher.localInstanceId, neededAmount.toFixed(precision));
                    currentTotal += neededAmount;
                    break;
                }
            }
            if (currentTotal >= targetAmount) break;
        }
        
        if (currentTotal < targetAmount - 0.000001) {
            setFeedbackMsg("The entered amount cannot be met with the available vouchers.");
        } else {
            setFeedbackMsg("");
        }
        setSelection(newSelection);
    };

    const handleStandardSelect = (id: string | null, isTest: boolean | null = null) => {
        setSelectedStandardId(id);
        setSelectedIsTest(isTest);
        setTargetAmountStr("");
        setSelection(new Map());
        setFeedbackMsg("");
    };

    const handleVoucherAmountChange = (voucherId: string, newAmountStr: string, maxAmount: string) => {
        let val = parseFloat(newAmountStr);
        const max = parseFloat(maxAmount);
        
        if (!isNaN(val) && val > max) {
            newAmountStr = maxAmount.toString();
            val = max;
        }

        // Enforce precision
        const voucher = availableVouchers.find(v => v.localInstanceId === voucherId);
        if (voucher && !isNaN(val)) {
            const precision = uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4;
            const parts = newAmountStr.split('.');
            if (parts.length > 1 && parts[1].length > precision) {
                newAmountStr = parts[0] + '.' + parts[1].substring(0, precision);
            }
        }

        const newSelection = new Map(selection);
        newSelection.set(voucherId, newAmountStr);
        setSelection(newSelection);

        if (selectedStandardId) {
            let total = 0;
            newSelection.forEach(amount => {
                const parsed = parseFloat(amount);
                if (!isNaN(parsed)) total += parsed;
            });
            const selectedStandardUuid = standardIdToUuidMap.get(selectedStandardId) || selectedStandardId;
            const precision = uuidToPrecisionMap.get(selectedStandardUuid) ?? 4;
            setTargetAmountStr(total > 0 ? total.toFixed(precision) : "");
        }
    };

    const handleRecipientChange = (e: ChangeEvent<HTMLInputElement>) => {
        const value = e.target.value;
        setRecipientId(value);

        if (value.length > 0) {
            const filtered = contacts.filter(c =>
                c.did.toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.firstName || '').toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.lastName || '').toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.organization || '').toLowerCase().includes(value.toLowerCase())
            );
            setSuggestions(filtered.slice(0, 5));
            setShowSuggestions(filtered.length > 0);
        } else {
            setSuggestions([]);
            setShowSuggestions(false);
        }
    };

    const currentContact = contacts.find(c => c.did === recipientId);

    const selectContact = (contact: Contact) => {
        setRecipientId(contact.did);
        setShowSuggestions(false);
        setShowContactPicker(false);
    };

    const privacyRules = useMemo(() => {
        if (selection.size === 0) return { mode: 'Flexible', forced: false };
        const selectedVoucherIds = Array.from(selection.keys());
        const selectedStandards = new Set<string>();
        selectedVoucherIds.forEach(vid => {
            const v = availableVouchers.find(av => av.localInstanceId === vid);
            if (v) selectedStandards.add(v.voucherStandardUuid);
        });
        let hasPublic = false;
        let hasPrivate = false;
        selectedStandards.forEach(uuid => {
            const standard = voucherStandards.find(s => standardIdToUuidMap.get(s.id) === uuid);
            if (standard) {
                const match = standard.content.match(/privacyMode\s*=\s*"([^"]+)"/);
                const mode = match ? match[1] : "Public";
                if (mode === "Public" || mode === "public") hasPublic = true;
                if (mode === "Stealth" || mode === "stealth" || mode === "Private" || mode === "private") hasPrivate = true;
            }
        });
        if (hasPublic && hasPrivate) return { mode: 'Incompatible', forced: false };
        if (hasPrivate) return { mode: 'Stealth', forced: true };
        if (hasPublic) return { mode: 'Public', forced: true };
        return { mode: 'Flexible', forced: false };
    }, [selection, availableVouchers, voucherStandards, standardIdToUuidMap]);

    useEffect(() => {
        if (privacyRules.mode === 'Stealth') setPrivacyMode('stealth');
        else if (privacyRules.mode === 'Public') setPrivacyMode('public');
        else if (privacyRules.mode === 'Flexible' && privacyMode === null && appSettings) {
            if (appSettings.privacyDefault === 'stealth') setPrivacyMode('stealth');
            else if (appSettings.privacyDefault === 'public') setPrivacyMode('public');
            else setPrivacyMode(null);
        }
    }, [privacyRules, privacyMode, appSettings]);


    const handleManualVoucherSelect = (voucher: VoucherSummary) => {
        const newSelection = new Map(selection);
        if (newSelection.has(voucher.localInstanceId)) {
            newSelection.delete(voucher.localInstanceId);
        } else {
            const precision = uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 4;
            const amount = parseFloat(voucher.currentAmount);
            newSelection.set(voucher.localInstanceId, isNaN(amount) ? voucher.currentAmount : amount.toFixed(precision));
        }
        setSelection(newSelection);

        if (selectedStandardId) {
            let total = 0;
            newSelection.forEach(amount => {
                const parsed = parseFloat(amount);
                if (!isNaN(parsed)) total += parsed;
            });
            const selectedStandardUuid = standardIdToUuidMap.get(selectedStandardId) || selectedStandardId;
            const precision = uuidToPrecisionMap.get(selectedStandardUuid) ?? 4;
            setTargetAmountStr(total > 0 ? total.toFixed(precision) : "");
        } else {
            setTargetAmountStr("");
        }
    };

    const checkoutSummary = useMemo(() => {
        let count = 0;
        const summableTotals = {} as Record<string, number>;
        const countableTotals = {} as Record<string, number>;

        for (const [voucherId, amountStr] of selection.entries()) {
            const voucher = availableVouchers.find(v => v.localInstanceId === voucherId);
            if (voucher) {
                count++;
                const amount = parseFloat(amountStr);
                // @ts-ignore
                if (voucher.divisible) {
                    if (!summableTotals[voucher.displayCurrency]) summableTotals[voucher.displayCurrency] = 0;
                    summableTotals[voucher.displayCurrency] += amount;
                } else {
                    if (!countableTotals[voucher.displayCurrency]) countableTotals[voucher.displayCurrency] = 0;
                    countableTotals[voucher.displayCurrency] += 1;
                }
            }
        }
        return { count, summableTotals, countableTotals };
    }, [selection, availableVouchers]);

    const handlePrepareTransferClick = (event: FormEvent) => {
        event.preventDefault();
        if (!recipientId) { setRecipientError(true); document.getElementById('recipientId')?.scrollIntoView({ behavior: 'smooth', block: 'center' }); return; }
        if (selection.size === 0) { setFeedbackMsg("Asset selection required."); return; }
        if (privacyRules.mode === 'Flexible' && privacyMode === null) {
            setPrivacyError(true);
            setFeedbackMsg("Privacy configuration required.");
            document.getElementById('privacyMode')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        setFeedbackMsg("");
        setShowConfirm(true);
    };

    async function executeTransfer() {
        setIsProcessing(true);
        try {
            const senderProfileNameToSend = (sendProfileName && customSenderName.trim() !== "") ? customSenderName.trim() : null;
            const notesToSend = notes.trim() === "" ? null : notes.trim();

            const sources: SourceTransfer[] = Array.from(selection.entries()).map(([id, amount]) => ({
                localInstanceId: id,
                amountToSend: amount,
            }));
            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(standard => {
                const uuid = standardIdToUuidMap.get(standard.id) || standard.id;
                standardDefinitionsToml[uuid] = standard.content;
            });

            const bundleResult = await protectAction(async (password) => {
                return await invoke<any>("create_transfer_bundle", {
                    recipientId,
                    sources,
                    notes: notesToSend,
                    senderProfileName: senderProfileNameToSend,
                    standardDefinitionsToml,
                    usePrivacyMode: privacyMode === 'stealth',
                    password
                });
            });
            if (!bundleResult) { setIsProcessing(false); return; }
            
            const summableAmounts: Record<string, string> = {};
            Object.entries(checkoutSummary.summableTotals).forEach(([unit, total]) => {
                // Find precision for this unit
                const voucher = availableVouchers.find(v => v.displayCurrency === unit);
                const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                summableAmounts[unit] = total.toFixed(precision);
            });
            const countableItems: Record<string, number> = checkoutSummary.countableTotals;

            const record: Omit<TransactionRecord, 'bundleData'> & { bundleData: number[] } = {
                id: crypto.randomUUID(),
                direction: 'sent',
                recipientId: recipientId,
                senderId: ownUserId,
                timestamp: new Date().toISOString(),
                summableAmounts: summableAmounts,
                countableItems: countableItems,
                involvedVouchers: bundleResult.involvedSourcesDetails.map((d: any) => d.localInstanceId),
                involvedSourcesDetails: bundleResult.involvedSourcesDetails,
                bundleData: bundleResult.bundleData,
                bundleId: bundleResult.bundleId,
                notes: notesToSend ?? undefined,
                senderProfileName: senderProfileNameToSend ?? undefined,
            };

            await protectAction(async (password) => {
                 await invoke("save_transaction_record", { record: record as TransactionRecord, password });
            });

            const summableStrings = Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => {
                const voucher = availableVouchers.find(v => v.displayCurrency === unit);
                const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                return `${formatAmount(total.toString(), precision)} ${unit}`;
            });
            const countableStrings = Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);
            const summaryString = [...summableStrings, ...countableStrings].join(', ');

            setFeedbackMsg("Transaction successfully prepared");
            setTimeout(() => onTransferPrepared(bundleResult.bundleData, recipientId, summaryString), 1500);

        } catch (e) {
            setFeedbackMsg(`Preparation failed: ${e}`);
            setIsProcessing(false);
            setShowConfirm(false);
        }
    }

    return (
        <PageLayout 
            title="Create Transfer" 
            description="Prepare a cryptographic asset transfer." 
            onBack={onBack}
        >
            <div className="max-w-5xl mx-auto space-y-8 pb-32">
                {feedbackMsg && (
                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in slide-in-from-top-2">
                        <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                        <p className="text-sm font-bold text-rose-800 leading-tight">{feedbackMsg}</p>
                    </div>
                )}

                <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
                    {/* Left Column: Recipient & Config */}
                    <div className="lg:col-span-7 space-y-8">
                        <Card header={<div className="flex items-center gap-2"><User size={18} className="text-theme-primary"/><span className="font-black text-xs uppercase tracking-widest text-theme-primary">Recipient</span></div>}>
                            <div className="space-y-6">
                                <div className="space-y-3">
                                    <label htmlFor="recipientId" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
                                        User ID (DID)
                                        <button type="button" onClick={() => setShowContactPicker(true)} className="text-theme-primary hover:underline flex items-center gap-1"><UserPlus size={10}/> Address Book</button>
                                    </label>
                                    <div className="relative group">
                                        {currentContact ? (
                                            <div className="flex items-center justify-between p-4 bg-theme-primary/5 border-2 border-theme-primary rounded-3xl animate-in zoom-in duration-300 shadow-sm">
                                                <ContactBadge did={currentContact.did} contacts={contacts} size="lg" />
                                                <button type="button" onClick={() => setRecipientId('')} className="p-2 hover:bg-rose-50 text-theme-light hover:text-rose-500 rounded-xl transition-all"><X size={20}/></button>
                                            </div>
                                        ) : (
                                            <div className="relative">
                                                <Input
                                                    id="recipientId"
                                                    value={recipientId}
                                                    onChange={handleRecipientChange}
                                                    onFocus={() => recipientId.length > 0 && suggestions.length > 0 && setShowSuggestions(true)}
                                                    onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
                                                    placeholder="did:key:z..."
                                                    className={`py-5 px-6 rounded-3xl font-mono text-xs ${recipientError ? 'border-rose-500 shadow-rose-100' : ''}`}
                                                />
                                                <div className="absolute right-4 top-1/2 -translate-y-1/2 text-theme-light"><Search size={18}/></div>
                                                
                                                {showSuggestions && (
                                                    <div className="absolute z-30 w-full mt-2 bg-white/90 backdrop-blur-xl border border-theme-subtle rounded-3xl shadow-premium-lg overflow-hidden animate-in slide-in-from-top-2 duration-200">
                                                        {suggestions.map(contact => (
                                                            <button key={contact.did} type="button" onClick={() => selectContact(contact)} className="w-full flex items-center gap-4 p-4 hover:bg-theme-primary/5 text-left border-b border-theme-subtle/40 last:border-0 transition-all">
                                                                <Avatar size={32} name={contact.did} variant="beam" />
                                                                <div className="min-w-0">
                                                                    <p className="text-sm font-black text-theme-secondary truncate">{(contact.profile.firstName || contact.profile.lastName) ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim() : (contact.profile.organization || 'Anonymous')}</p>
                                                                    <p className="text-[9px] font-mono text-theme-light truncate">{contact.did}</p>
                                                                </div>
                                                            </button>
                                                        ))}
                                                    </div>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                    
                                    {typeof trustStatus === 'object' && 'KnownOffender' in trustStatus && (
                                        <div className="p-5 bg-rose-50 border border-rose-100 rounded-[32px] flex items-start gap-4 animate-in shake duration-500 shadow-sm">
                                            <AlertTriangle className="text-rose-500 shrink-0 mt-1" size={24} />
                                            <div>
                                                <h4 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">Reputation Alert</h4>
                                                <p className="text-xs text-rose-800 font-medium leading-relaxed">This identity is associated with past ledger conflicts. Exercise extreme caution.</p>
                                            </div>
                                        </div>
                                    )}
                                </div>

                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><BookOpen size={10}/> Note</label>
                                    <Textarea value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Transaction context (optional)..." className="rounded-2xl min-h-[80px]" />
                                </div>

                                <div className="p-4 bg-slate-50 border border-slate-200 rounded-3xl space-y-4">
                                    <div className="flex items-center justify-between">
                                        <label className="flex items-center gap-3 cursor-pointer group">
                                            <div className={`w-10 h-6 rounded-full transition-all relative ${sendProfileName ? 'bg-theme-primary shadow-inner' : 'bg-slate-300'}`}>
                                                <div className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-all shadow-md ${sendProfileName ? 'left-5' : 'left-1'}`}></div>
                                                <input type="checkbox" className="hidden" checked={sendProfileName} onChange={(e) => setSendProfileName(e.target.checked)} />
                                            </div>
                                            <span className="text-xs font-black text-slate-600 uppercase tracking-widest group-hover:text-theme-primary transition-colors">Disclose My Identity</span>
                                        </label>
                                    </div>
                                    {sendProfileName && (
                                        <div className="space-y-2 animate-in slide-in-from-top-2">
                                            <label className="text-[9px] font-black text-slate-400 uppercase tracking-widest">Display As</label>
                                            <Input value={customSenderName} onChange={(e) => setCustomSenderName(e.target.value)} placeholder="e.g. John Doe" className="bg-white border-slate-200 text-sm font-bold" />
                                            <p className="text-[9px] font-medium text-slate-400 leading-tight flex items-start gap-1.5 pt-1"><Info size={10} className="shrink-0"/> This is only visible to the direct recipient to establish trust.</p>
                                        </div>
                                    )}
                                </div>
                            </div>
                        </Card>

                        <Card header={<div id="privacyMode" className="flex items-center gap-2"><Shield size={18} className="text-theme-primary"/><span className="font-black text-xs uppercase tracking-widest text-theme-primary">Security & Privacy</span></div>}>
                            <div className="space-y-6">
                                {privacyRules.forced ? (
                                    <div className="p-5 bg-theme-primary/5 border border-theme-primary/10 rounded-[32px] flex items-center justify-between shadow-inner-soft">
                                        <div className="flex items-center gap-4">
                                            <div className={`p-3 rounded-2xl ${privacyRules.mode === 'Stealth' ? 'bg-purple-500 text-white' : 'bg-blue-500 text-white'}`}>
                                                {privacyRules.mode === 'Stealth' ? <Lock size={20}/> : <Eye size={20}/>}
                                            </div>
                                            <div>
                                                <h4 className="text-sm font-black text-theme-secondary uppercase tracking-widest">{privacyRules.mode === 'Stealth' ? 'Stealth Execution' : 'Transparent Execution'}</h4>
                                                <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest">Mandatory for selected standard</p>
                                            </div>
                                        </div>
                                        <CheckCircle2 className="text-theme-primary" size={24} />
                                    </div>
                                ) : (
                                    <div className={`grid grid-cols-2 gap-4 p-2 bg-slate-50 border-2 rounded-[40px] transition-all ${privacyError ? 'border-rose-500' : 'border-slate-100'}`}>
                                        <button type="button" onClick={() => setPrivacyMode('public')} className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'public' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}>
                                            <div className="p-3 bg-blue-50 text-blue-500 rounded-2xl"><Eye size={24}/></div>
                                            <span className="text-xs font-black uppercase tracking-widest text-slate-900">Public</span>
                                        </button>
                                        <button type="button" onClick={() => setPrivacyMode('stealth')} className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'stealth' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}>
                                            <div className="p-3 bg-purple-50 text-purple-500 rounded-2xl"><Lock size={24}/></div>
                                            <span className="text-xs font-black uppercase tracking-widest text-slate-900">Stealth</span>
                                        </button>
                                    </div>
                                )}

                                {privacyRules.mode === 'Incompatible' && (
                                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3">
                                        <AlertTriangle size={18} className="text-rose-500" />
                                        <p className="text-xs font-bold text-rose-800">Privacy Conflict: Mixed privacy requirements in selection.</p>
                                    </div>
                                )}

                                <details className="group">
                                    <summary className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-theme-light cursor-pointer hover:text-theme-primary transition-colors list-none">
                                        <Info size={14}/> Privacy Details <ArrowRight size={10} className="group-open:rotate-90 transition-transform" />
                                    </summary>
                                    <div className="mt-4 p-5 bg-slate-50 rounded-3xl border border-slate-100 space-y-4 animate-in slide-in-from-top-2">
                                        <div className="space-y-1">
                                            <h5 className="text-xs font-black text-slate-900">🔒 Stealth Mode</h5>
                                            <p className="text-[11px] text-slate-600 leading-relaxed font-medium">Anonymizes your signature in the transaction chain. Prevents mass surveillance. Reveal occurs only upon double-spend detection.</p>
                                        </div>
                                        <div className="space-y-1">
                                            <h5 className="text-xs font-black text-slate-900">👁️ Public Mode</h5>
                                            <p className="text-[11px] text-slate-600 leading-relaxed font-medium">Standard transparency. Builds high trust in closed networks and community circles.</p>
                                        </div>
                                    </div>
                                </details>
                            </div>
                        </Card>
                    </div>

                    {/* Right Column: Asset Selection & Summary */}
                    <div className="lg:col-span-5 space-y-8">
                        <Card header={<div className="flex items-center gap-2"><Coins size={18} className="text-theme-primary"/><span className="font-black text-xs uppercase tracking-widest text-theme-primary">Voucher Selection</span></div>}>
                            <div className="space-y-6">
                                <div className="flex flex-wrap gap-2">
                                    <button type="button" onClick={() => handleStandardSelect(null)} className={`px-4 py-2 text-[10px] font-black uppercase tracking-widest rounded-full border-2 transition-all ${selectedStandardId === null ? 'bg-theme-primary border-theme-primary text-white shadow-md' : 'border-slate-100 text-slate-400 hover:border-slate-200'}`}>All</button>
                                    {activeAssetClasses.map(group => {
                                        const standardId = [...standardIdToUuidMap.entries()].find(([, uuid]) => uuid === group.standardUuid)?.[0] || group.standardUuid;
                                        const key = `${standardId}:${group.isTestVoucher}`;
                                        const isSelected = selectedStandardId === standardId && selectedIsTest === group.isTestVoucher;
                                        return (
                                            <button
                                                key={key}
                                                type="button"
                                                onClick={() => handleStandardSelect(standardId, group.isTestVoucher)}
                                                className={`px-4 py-2 text-[10px] font-black uppercase tracking-widest rounded-full border-2 transition-all ${isSelected ? (group.isTestVoucher ? 'bg-rose-500 border-rose-500 text-white shadow-md' : 'bg-theme-primary border-theme-primary text-white shadow-md') : (group.isTestVoucher ? 'border-rose-200 text-rose-400 hover:border-rose-300' : 'border-slate-100 text-slate-400 hover:border-slate-200')}`}
                                            >
                                                {group.displayStandardName}
                                            </button>
                                        );
                                    })}
                                </div>

                                {selectedStandardId && (
                                    <div className="space-y-3 animate-in slide-in-from-top-2">
                                        <label htmlFor="target-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Target Amount</label>
                                        <div className="relative">
                                            <Input id="target-amount" value={targetAmountStr} onChange={(e) => handleTargetAmountChange(e.target.value)} type="number" placeholder="0.00" className="py-5 px-6 rounded-3xl font-black text-2xl tracking-tighter" />
                                            <div className="absolute right-6 top-1/2 -translate-y-1/2 font-black text-theme-primary uppercase tracking-widest">{filteredVouchers[0]?.displayCurrency}</div>
                                        </div>
                                        <p className="text-[10px] font-bold text-theme-light italic text-center">Automatic selection will prioritize optimal ledger fragmentation.</p>
                                    </div>
                                )}

                                <div className="space-y-3 pt-4 border-t border-theme-subtle/40">
                                    <div className="flex items-center justify-between">
                                        <h3 className="text-[10px] font-black text-theme-light uppercase tracking-widest">Available Vouchers</h3>
                                        <button type="button" className="text-theme-primary hover:underline flex items-center gap-1.5 text-[10px] font-black uppercase tracking-widest"><Filter size={10}/> Sort Optimal</button>
                                    </div>
                                    
                                    <div className="space-y-3 max-h-[400px] overflow-y-auto pr-2 scrollbar-thin scrollbar-thumb-theme-subtle">
                                        {filteredVouchers.length > 0 ? filteredVouchers.map(v => {
                                            const selectedAmount = selection.get(v.localInstanceId);
                                            const isSelected = selectedAmount !== undefined;
                                            return (
                                                <div key={v.localInstanceId} className={`p-4 rounded-[32px] border-2 transition-all duration-300 relative overflow-hidden group shadow-sm ${isSelected ? 'bg-theme-primary/5 border-theme-primary ring-4 ring-theme-primary/10' : 'bg-white border-slate-100 hover:border-slate-200'}`}>
                                                    <div className="flex justify-between items-start cursor-pointer" onClick={() => handleManualVoucherSelect(v)}>
                                                        <div>
                                                            <div className="flex items-baseline gap-2">
                                                                <span className="text-xl font-black text-theme-secondary tracking-tight">
                                                                    {(() => {
                                                                        const precision = uuidToPrecisionMap.get(v.voucherStandardUuid) ?? 2;
                                                                        return isSelected && selectedAmount !== v.currentAmount ? formatAmount(selectedAmount, precision) : formatAmount(v.currentAmount, precision);
                                                                    })()}
                                                                </span>
                                                                <span className="text-[10px] font-black text-theme-primary uppercase tracking-widest">{v.displayCurrency}</span>
                                                            </div>
                                                            <div className="flex items-center gap-1.5 mt-1">
                                                                <Avatar size={14} name={v.localInstanceId} variant="pixel" />
                                                                <span className="text-[9px] font-bold text-theme-light uppercase tracking-widest truncate max-w-[120px]">{v.creatorFirstName} {v.creatorLastName}</span>
                                                            </div>
                                                        </div>
                                                        <div className="text-right">
                                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-widest">{v.displayStandardName}</p>
                                                            {v.isTestVoucher && (
                                                                <span className="text-[9px] font-black bg-rose-500 text-white px-2 py-0.5 rounded-full uppercase tracking-tighter">
                                                                    Test Mode
                                                                </span>
                                                            )}
                                                            <p className="text-[9px] font-bold text-slate-400 mt-0.5">{formatDate(v.validUntil)}</p>
                                                        </div>
                                                    </div>
                                                    {isSelected && v.divisible && (
                                                        <div className="mt-4 pt-4 border-t border-theme-primary/10 animate-in fade-in slide-in-from-top-2">
                                                            <div className="flex items-center justify-between">
                                                                <span className="text-[9px] font-black text-theme-primary uppercase tracking-widest">Partial Send</span>
                                                                <div className="flex items-center gap-2">
                                                                    <Input 
                                                                        type="number" 
                                                                        value={selectedAmount} 
                                                                        onChange={(e) => handleVoucherAmountChange(v.localInstanceId, e.target.value, v.currentAmount)}
                                                                        className="w-24 py-1.5 px-3 text-right font-black text-xs rounded-xl border-theme-primary/30"
                                                                        max={v.currentAmount}
                                                                        min="0"
                                                                        step="any"
                                                                    />
                                                                </div>
                                                            </div>
                                                        </div>
                                                    )}
                                                    {isSelected && (
                                                        <div className="absolute top-3 right-3 opacity-20 group-hover:opacity-40 transition-opacity">
                                                            <CheckCircle2 className="text-theme-primary" size={16} />
                                                        </div>
                                                    )}
                                                </div>
                                            );
                                        }) : (
                                            <div className="text-center py-10 bg-slate-50 rounded-[32px] border-2 border-dashed border-slate-200">
                                                <CreditCard className="mx-auto text-slate-300 mb-2" size={32} />
                                                <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">Inventory Depleted</p>
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </div>
                        </Card>

                        {/* Summary Sticky Card */}
                        <div className="sticky bottom-8 z-20">
                            <Card variant="none" className="shadow-premium-xl border-theme-primary/20 bg-theme-primary text-white overflow-hidden p-0 rounded-[40px]">
                                <div className="p-8 space-y-6">
                                    <div className="flex justify-between items-center">
                                        <div className="space-y-1">
                                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-white/60">Disbursement Summary</p>
                                            <h3 className="text-2xl font-black tracking-tighter">
                                                {checkoutSummary.count > 0 ? (
                                                    <div className="flex flex-wrap gap-x-4 gap-y-1">
                                                        {Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => {
                                                            const voucher = availableVouchers.find(v => v.displayCurrency === unit);
                                                            const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                                                            return (
                                                                <span key={unit} className="flex items-baseline gap-1.5">
                                                                    {formatAmount(total.toString(), precision)}
                                                                    <span className="text-sm font-bold uppercase tracking-widest opacity-70">{unit}</span>
                                                                </span>
                                                            );
                                                        })}
                                                        {Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => (
                                                            <span key={unit} className="flex items-baseline gap-1.5">
                                                                {total}
                                                                <span className="text-sm font-bold uppercase tracking-widest opacity-70">{unit}{total > 1 ? 's' : ''}</span>
                                                            </span>
                                                        ))}
                                                    </div>
                                                ) : "0.00"}
                                            </h3>
                                        </div>
                                        <div className="w-14 h-14 bg-white/20 rounded-2xl flex items-center justify-center shadow-inner">
                                            <Send size={28} />
                                        </div>
                                    </div>
                                    
                                    <div className="flex items-center justify-between pt-4 border-t border-white/20">
                                        <div className="flex items-center gap-2">
                                            <div className="w-2 h-2 rounded-full bg-white animate-pulse"></div>
                                            <span className="text-[9px] font-black uppercase tracking-widest">{checkoutSummary.count} Vouchers Selected</span>
                                        </div>
                                        <Button 
                                            onClick={handlePrepareTransferClick} 
                                            disabled={isProcessing} 
                                            className="!bg-white !bg-none !text-theme-primary rounded-2xl px-6 py-3 font-black text-xs uppercase tracking-widest shadow-lg hover:scale-105 active:scale-95 transition-all"
                                        >
                                            {isProcessing ? "Sending..." : "Send Voucher"}
                                        </Button>
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>
                </div>
            </div>

            <ConfirmationModal 
                isOpen={showConfirm}
                title="Confirm Transfer"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-6 bg-theme-primary/5 rounded-[32px] border border-theme-primary/20 text-center space-y-2">
                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">Net Disbursement</p>
                            <div className="flex flex-col items-center">
                                {Object.entries(checkoutSummary.summableTotals).map(([u, t]) => {
                                    const voucher = availableVouchers.find(v => v.displayCurrency === u);
                                    const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                                    return (
                                        <p key={u} className="text-3xl font-black text-theme-primary tracking-tighter">{t.toFixed(precision)} {u}</p>
                                    );
                                })}
                                {Object.entries(checkoutSummary.countableTotals).map(([u, t]) => (
                                    <p key={u} className="text-3xl font-black text-theme-primary tracking-tighter">{t} {u}</p>
                                ))}
                            </div>
                        </div>
                        <div className="space-y-2">
                            <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">Recipient ID</p>
                            <p className="text-xs font-mono break-all p-3 bg-slate-50 border border-slate-100 rounded-2xl text-slate-600">{recipientId}</p>
                        </div>
                        <p className="text-sm font-medium text-theme-secondary leading-relaxed pt-2">
                            This action will cryptographically sign the transfer bundle. All selected vouchers will be fragmented and locked for export. Proceed?
                        </p>
                    </div>
                }
                confirmText="Yes, Prepare Transfer"
                onConfirm={executeTransfer}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isProcessing}
            />

            {/* Contact Picker Modal */}
            {showContactPicker && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
                    <Card className="w-full max-w-lg shadow-premium-xl rounded-[48px] p-0 overflow-hidden animate-in zoom-in duration-300 flex flex-col max-h-[85vh]">
                        <div className="p-8 border-b border-theme-subtle/40 flex justify-between items-center bg-slate-50/50">
                            <div>
                                <h2 className="text-2xl font-black text-theme-secondary tracking-tight uppercase">Address Book</h2>
                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">Select recipient</p>
                            </div>
                            <button onClick={() => setShowContactPicker(false)} className="p-3 hover:bg-white rounded-2xl transition-all shadow-sm border border-transparent hover:border-slate-100"><X size={20}/></button>
                        </div>
                        <div className="flex-grow overflow-y-auto p-8 space-y-4 scrollbar-thin scrollbar-thumb-theme-subtle">
                            {contacts.length === 0 ? (
                                <div className="text-center py-20 space-y-4">
                                    <div className="mx-auto w-16 h-16 bg-slate-50 text-slate-300 rounded-3xl flex items-center justify-center"><UserPlus size={32}/></div>
                                    <p className="text-xs font-black text-slate-400 uppercase tracking-widest">Address Book Empty</p>
                                </div>
                            ) : (
                                contacts.map(contact => (
                                    <button key={contact.did} onClick={() => selectContact(contact)} className="w-full flex items-center gap-4 p-5 bg-white hover:bg-theme-primary/5 border border-slate-100 hover:border-theme-primary/30 rounded-[32px] transition-all shadow-sm hover:shadow-md text-left group">
                                        <div className="w-12 h-12 rounded-2xl overflow-hidden flex-shrink-0 border-2 border-white shadow-sm group-hover:scale-110 transition-transform">
                                            <Avatar size={48} name={contact.did} variant="beam" />
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <p className="text-sm font-black text-theme-secondary truncate uppercase group-hover:text-theme-primary">{(contact.profile.firstName || contact.profile.lastName) ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim() : (contact.profile.organization || 'Anonymous')}</p>
                                            <p className="text-[9px] font-mono text-theme-light truncate opacity-60">{contact.did}</p>
                                        </div>
                                        <div className="p-2 bg-slate-50 rounded-xl text-slate-300 group-hover:text-theme-primary group-hover:bg-theme-primary/10 transition-all"><ArrowRight size={16}/></div>
                                    </button>
                                ))
                            )}
                        </div>
                        <div className="p-8 bg-slate-50/50 border-t border-theme-subtle/40">
                            <Button variant="secondary" onClick={() => setShowContactPicker(false)} className="w-full rounded-2xl">Close</Button>
                        </div>
                    </Card>
                </div>
            )}
        </PageLayout>
    );
}
