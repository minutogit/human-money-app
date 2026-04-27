// src/components/SendView.tsx
import { useState, useEffect, useMemo, FormEvent, ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { VoucherSummary, VoucherStandardInfo, SourceTransfer, TransactionRecord, InvolvedVoucherInfo, Contact, TrustStatus, AppSettings } from "../types";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { ContactBadge } from "./ui/ContactBadge";
import Avatar from "boring-avatars";

interface SendViewProps {
    onBack: () => void;
    onTransferPrepared: (bundleData: number[], recipientId: string, summary: string) => void;
    profileName: string | null; // NEU: Profilname des aktuellen Benutzers
}

function formatDate(isoString: string): string {
    if (!isoString) return 'N/A';
    return new Date(isoString).toLocaleDateString(undefined, {
        year: 'numeric', month: 'short', day: 'numeric'
    });
}

function formatAmount(amountStr: string): string {
    const num = parseFloat(amountStr);
    if (isNaN(num)) return amountStr;
    return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}


export function SendView({ onBack, onTransferPrepared, profileName }: SendViewProps) {
    const { protectAction } = useSession();
    const [recipientId, setRecipientId] = useState("");
    const [notes, setNotes] = useState(""); // NEU
    const [sendProfileName, setSendProfileName] = useState(true); // NEU
    const [customSenderName, setCustomSenderName] = useState("");
    const [ownUserId, setOwnUserId] = useState("");
    const [targetAmountStr, setTargetAmountStr] = useState("");
    const [availableVouchers, setAvailableVouchers] = useState<VoucherSummary[]>([]);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string | null>(null);
    const [selection, setSelection] = useState<Map<string, string>>(new Map());
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isProcessing, setIsProcessing] = useState(false);
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());
    const [standardIdToNameMap, setStandardIdToNameMap] = useState<Map<string, string>>(new Map());
    const [showConfirm, setShowConfirm] = useState(false);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("Clean");
    const [privacyMode, setPrivacyMode] = useState<'public' | 'stealth' | null>(null);
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    
    // Adressebuch & Vorschläge
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [suggestions, setSuggestions] = useState<Contact[]>([]);
    const [showSuggestions, setShowSuggestions] = useState(false);
    const [showContactPicker, setShowContactPicker] = useState(false);
    const [recipientError, setRecipientError] = useState(false);
    const [privacyError, setPrivacyError] = useState(false);

    // Reset error indicator after short delay
    useEffect(() => {
        if (recipientError) {
            const timer = setTimeout(() => setRecipientError(false), 2000);
            return () => clearTimeout(timer);
        }
    }, [recipientError]);

    // Reset privacy error indicator after short delay
    useEffect(() => {
        if (privacyError) {
            const timer = setTimeout(() => setPrivacyError(false), 2000);
            return () => clearTimeout(timer);
        }
    }, [privacyError]);

    // Reset privacy mode when selection changes (for flexible mode)
    useEffect(() => {
        if (privacyRules.mode === 'Flexible') {
            setPrivacyMode(null);
        }
    }, [selection]);

    // Reputation Check useEffect
    useEffect(() => {
        if (recipientId.length < 10) {
            setTrustStatus("Clean");
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
            // Log added here
            logger.info("SendView component displayed. Fetching initial data...");
            try {
                // KORREKTUR: Rufe die Gutscheine ohne Argumente ab, genau wie im Dashboard.
                const allFetchedVouchers = await invoke<VoucherSummary[]>("get_voucher_summaries");
                logger.info(`1. Fetched ${allFetchedVouchers.length} vouchers from backend.`);

                // Use robust logic from Dashboard to handle both string and object statuses.
                const activeVouchers = allFetchedVouchers.filter(v => {
                    const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
                    return statusName === 'Active';
                });
                logger.info(`2. Filtered down to ${activeVouchers.length} active vouchers.`);

                const userId = await invoke<string>("get_user_id");
                
                // Fetch public profile to build a better default display name
                try {
                    const userProfile = await invoke<any>("get_user_profile");
                    let defaultName = "";
                    if (userProfile.organization) {
                        defaultName = userProfile.organization;
                    } else if (userProfile.first_name || userProfile.last_name) {
                        defaultName = `${userProfile.first_name || ''} ${userProfile.last_name || ''}`.trim();
                    } else {
                        defaultName = profileName || "";
                    }
                    setCustomSenderName(defaultName);
                } catch (e) {
                    logger.warn(`Could not fetch user profile for default sender name: ${e}`);
                    setCustomSenderName(profileName || "");
                }

                const standards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                const settings = await invoke<AppSettings>("get_app_settings");
                const newMap = new Map<string, string>();
                const newNameMap = new Map<string, string>();
                standards.forEach(s => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        newMap.set(s.id, uuidMatch[1]);
                    }
                    const nameMatch = s.content.match(/name\s*=\s*"([^"]+)"/);
                    if (nameMatch && nameMatch[1]) {
                        newNameMap.set(s.id, nameMatch[1]);
                    }
                });
                setStandardIdToUuidMap(newMap);
                setStandardIdToNameMap(newNameMap);

                // KORREKTUR (von 13:31): Die Annahme des Prototyps wiederherstellen.
                // Die 'divisible'-Eigenschaft kommt noch nicht vom Backend-Summary.
                const enrichedVouchers = activeVouchers.map(v => ({ ...v, divisible: true }));

                setFeedbackMsg("");
                setAvailableVouchers(enrichedVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                setAppSettings(settings);
                
                // Kontakte laden
                const fetchedContacts = await invoke<Contact[]>("get_contacts");
                setContacts(fetchedContacts);
                
                logger.info(`3. Set state with ${enrichedVouchers.length} vouchers and ${fetchedContacts.length} contacts.`);
            } catch (e) {
                const msg = `Failed to fetch data for SendView: ${e}`;
                logger.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchData();
    }, []);

    const filteredVouchers = useMemo(() => {
        if (!selectedStandardId) return availableVouchers;
        const selectedStandardUuid = standardIdToUuidMap.get(selectedStandardId);
        if (!selectedStandardUuid) return [];
        return availableVouchers.filter(v => v.voucher_standard_uuid === selectedStandardUuid);
    }, [availableVouchers, selectedStandardId, standardIdToUuidMap]);


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

        const selectedStandard = voucherStandards.find(s => s.id === selectedStandardId);
        let precision = 4;
        if (selectedStandard) {
            const match = selectedStandard.content.match(/amount_decimal_places\s*=\s*(\d+)/);
            if (match && match[1]) {
                precision = parseInt(match[1], 10);
            }
        }

        // Calculate max possible amount for the current standard
        let maxPossibleAmount = 0;
        filteredVouchers.forEach(v => {
            const amt = parseFloat(v.current_amount);
            if (!isNaN(amt)) maxPossibleAmount += amt;
        });
        
        // Clean up float precision issues
        maxPossibleAmount = parseFloat(maxPossibleAmount.toFixed(precision));

        // Cap target amount if it exceeds max
        let finalValStr = valStr;
        if (targetAmount > maxPossibleAmount && maxPossibleAmount > 0) {
            targetAmount = maxPossibleAmount;
            finalValStr = targetAmount.toString();
        }
        
        setTargetAmountStr(finalValStr);

        const newSelection = new Map<string, string>();
        let currentTotal = 0;

        const sortedVouchers = [...filteredVouchers].sort((a, b) => parseFloat(a.current_amount) - parseFloat(b.current_amount));
        for (const voucher of sortedVouchers) {
            const voucherAmount = parseFloat(voucher.current_amount);
            if (currentTotal + voucherAmount <= targetAmount) {
                newSelection.set(voucher.local_instance_id, voucher.current_amount);
                currentTotal += voucherAmount;
            } else {
                const neededAmount = targetAmount - currentTotal;
                // @ts-ignore
                if (voucher.divisible) {
                    newSelection.set(voucher.local_instance_id, neededAmount.toFixed(precision));
                    currentTotal += neededAmount;
                    break;
                }
            }
            if (currentTotal >= targetAmount) break;
        }
        
        // Use a small epsilon to avoid floating point precision issues in comparison
        if (currentTotal < targetAmount - 0.000001) {
            setFeedbackMsg("The entered amount cannot be met with the available vouchers.");
        } else {
            setFeedbackMsg("");
        }
        setSelection(newSelection);
    };

    const handleStandardSelect = (id: string | null) => {
        setSelectedStandardId(id);
        setTargetAmountStr("");
        setSelection(new Map());
        setFeedbackMsg("");
    };

    const handleVoucherAmountChange = (voucherId: string, newAmountStr: string, maxAmount: string) => {
        let val = parseFloat(newAmountStr);
        const max = parseFloat(maxAmount);
        
        if (!isNaN(val) && val > max) {
            newAmountStr = maxAmount.toString();
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
            setTargetAmountStr(total > 0 ? total.toString() : "");
        }
    };

    // Vorschlaggs-Logik
    const handleRecipientChange = (e: ChangeEvent<HTMLInputElement>) => {
        const value = e.target.value;
        setRecipientId(value);

        if (value.length > 0) {
            const filtered = contacts.filter(c =>
                c.did.toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.first_name || '').toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.last_name || '').toLowerCase().includes(value.toLowerCase()) ||
                (c.profile.organization || '').toLowerCase().includes(value.toLowerCase())
            );
            setSuggestions(filtered.slice(0, 5)); // Max 5 Vorschläge
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

    // Ermittle die Privacy-Regeln basierend auf der aktuellen Auswahl
    const privacyRules = useMemo(() => {
        if (selection.size === 0) return { mode: 'Flexible', forced: false };
        
        const selectedVoucherIds = Array.from(selection.keys());
        const selectedStandards = new Set<string>();
        
        selectedVoucherIds.forEach(vid => {
            const v = availableVouchers.find(av => av.local_instance_id === vid);
            if (v) selectedStandards.add(v.voucher_standard_uuid);
        });

        let hasPublic = false;
        let hasPrivate = false;

        selectedStandards.forEach(uuid => {
            const standard = voucherStandards.find(s => standardIdToUuidMap.get(s.id) === uuid);
            if (standard) {
                const match = standard.content.match(/privacy_mode\s*=\s*"([^"]+)"/);
                const mode = match ? match[1] : "Public"; // Default
                if (mode === "Public" || mode === "public") hasPublic = true;
                if (mode === "Stealth" || mode === "stealth" || mode === "Private" || mode === "private") hasPrivate = true;
            }
        });

        // Logik für Mischpakete:
        // Wenn sowohl Public als auch Stealth dabei sind -> Inkompatibel
        if (hasPublic && hasPrivate) return { mode: 'Incompatible', forced: false };
        if (hasPrivate) return { mode: 'Stealth', forced: true };
        if (hasPublic) return { mode: 'Public', forced: true };
        return { mode: 'Flexible', forced: false };
    }, [selection, availableVouchers, voucherStandards, standardIdToUuidMap]);

    // Effekt: Setze den Privacy-Schalter automatisch, wenn er erzwungen wird
    useEffect(() => {
        if (privacyRules.mode === 'Stealth') {
            setPrivacyMode('stealth');
        } else if (privacyRules.mode === 'Public') {
            setPrivacyMode('public');
        } else if (privacyRules.mode === 'Flexible' && privacyMode === null && appSettings) {
            // Apply default setting when flexible mode is detected and user hasn't selected yet
            if (appSettings.privacy_default === 'stealth') {
                setPrivacyMode('stealth');
            } else if (appSettings.privacy_default === 'public') {
                setPrivacyMode('public');
            } else {
                // 'ask' mode - don't auto-select, require user to choose
                setPrivacyMode(null);
            }
        }
    }, [privacyRules, privacyMode, appSettings]);


    const handleManualVoucherSelect = (voucher: VoucherSummary) => {
        const newSelection = new Map(selection);
        if (newSelection.has(voucher.local_instance_id)) {
            newSelection.delete(voucher.local_instance_id);
        } else {
            newSelection.set(voucher.local_instance_id, voucher.current_amount);
        }
        setSelection(newSelection);

        if (selectedStandardId) {
            let total = 0;
            newSelection.forEach(amount => {
                const parsed = parseFloat(amount);
                if (!isNaN(parsed)) total += parsed;
            });
            setTargetAmountStr(total > 0 ? total.toString() : "");
        } else {
            setTargetAmountStr("");
        }
    };

    // KORREKTUR (von 13:28): Unterscheidet summable/countable
    const checkoutSummary = useMemo(() => {
        let count = 0;
        const summableTotals = {} as Record<string, number>;
        const countableTotals = {} as Record<string, number>;

        for (const [voucherId, amountStr] of selection.entries()) {
            const voucher = availableVouchers.find(v => v.local_instance_id === voucherId);
            if (voucher) {
                count++;
                const amount = parseFloat(amountStr);

                // NEUE LOGIK: Unterscheidung basierend auf 'divisible'
                // @ts-ignore (wir wissen, dass 'divisible' durch 'enrichedVouchers' hinzugefügt wurde)
                if (voucher.divisible) {
                    if (!summableTotals[voucher.unit]) {
                        summableTotals[voucher.unit] = 0;
                    }
                    summableTotals[voucher.unit] += amount;
                } else {
                    // Nicht-teilbare Gutscheine werden gezählt
                    if (!countableTotals[voucher.unit]) {
                        countableTotals[voucher.unit] = 0;
                    }
                    countableTotals[voucher.unit] += 1; // Wir senden 1 ganzen Gutschein
                }
            }
        }
        return { count, summableTotals, countableTotals };
    }, [selection, availableVouchers]);

    // 1. Schritt: Validierung und Modal öffnen
    const handlePrepareTransferClick = (event: FormEvent) => {
        event.preventDefault();
        // Prüfe zuerst recipient
        if (!recipientId) {
            setRecipientError(true);
            // Scroll zum recipient Feld
            document.getElementById('recipientId')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        // Dann prüfe selection
        if (selection.size === 0) {
            setFeedbackMsg("Please select at least one voucher");
            return;
        }
        // Prüfe ob Privacy Mode für flexible Standards ausgewählt wurde
        if (privacyRules.mode === 'Flexible' && privacyMode === null) {
            setPrivacyError(true);
            setFeedbackMsg("Please select privacy mode to continue");
            document.getElementById('privacyMode')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            return;
        }
        setFeedbackMsg("");
        setShowConfirm(true);
    };

    // 2. Schritt: Die eigentliche Ausführung (wird vom Modal aufgerufen)
    async function executeTransfer() {
        setIsProcessing(true);
        // Modal bleibt offen oder zeigt Loading, wir schließen es erst am Ende oder bei Fehler
        try {
            const senderProfileNameToSend = (sendProfileName && customSenderName.trim() !== "") ? customSenderName.trim() : null;
            const notesToSend = notes.trim() === "" ? null : notes.trim();

            const sources: SourceTransfer[] = Array.from(selection.entries()).map(([id, amount]) => ({
                local_instance_id: id,
                amount_to_send: amount,
            }));
            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(standard => {
                //Das Backend erwartet die UUID als Key
                const uuid = standardIdToUuidMap.get(standard.id) || standard.id;
                standardDefinitionsToml[uuid] = standard.content;
            });

            // AKTUALISIERT: Erwarte ein Objekt statt nur bytes, um die bundle_id für den Record zu haben.
            const bundleResult = await protectAction(async (password) => {
                return await invoke<CreateBundleResult>("create_transfer_bundle", {
                    recipientId,
                    sources,
                    notes: notesToSend,
                    senderProfileName: senderProfileNameToSend,
                    standardDefinitionsToml,
                    usePrivacyMode: privacyMode === 'stealth',
                    password
                });
            });
            if (!bundleResult) {
                setIsProcessing(false);
                return;
            }
            
            // NEU: Definieren der vollständigen Rückgabe-Struktur
            interface CreateBundleResult {
                bundleData: number[];
                bundleId: string;
                involvedSourcesDetails: InvolvedVoucherInfo[];
            }

            logger.info(`Successfully created transfer bundle ${bundleResult.bundleId} with ${bundleResult.bundleData.length} bytes and ${bundleResult.involvedSourcesDetails.length} source details.`);

            // KORREKTUR (von 13:28): summableAmounts und countableItems getrennt verarbeiten
            const summableAmounts: Record<string, string> = {};
            Object.entries(checkoutSummary.summableTotals).forEach(([unit, total]) => {
                summableAmounts[unit] = total.toFixed(2);
            });

            const countableItems: Record<string, number> = checkoutSummary.countableTotals;

            const record: Omit<TransactionRecord, 'bundle_data'> & { bundle_data: number[] } = {
                id: crypto.randomUUID(),
                direction: 'sent',
                recipient_id: recipientId,
                sender_id: ownUserId,
                timestamp: new Date().toISOString(),
                summableAmounts: summableAmounts, // KORREKTUR (von 13:28)
                countableItems: countableItems, // KORREKTUR (von 13:28)
                // NEU: Speichere sowohl die reinen IDs als auch die vollen Details
                involved_vouchers: bundleResult.involvedSourcesDetails.map(d => d.local_instance_id),
                involved_sources_details: bundleResult.involvedSourcesDetails,
                bundle_data: bundleResult.bundleData,
                bundle_id: bundleResult.bundleId, // NEU
                notes: notesToSend ?? undefined, // NEU
                sender_profile_name: senderProfileNameToSend ?? undefined, // NEU
            };

            // Auch das Speichern des Records muss geschützt sein (nutzt die selbe Session/Passwort)
            await protectAction(async (password) => {
                 await invoke("save_transaction_record", { record: record as TransactionRecord, password });
            });
            logger.info(`Transaction record ${record.id} saved successfully.`);

            // KORREKTUR (von 13:28): Detaillierte summaryString erstellen
            const summableStrings = Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => `${formatAmount(total.toString())} ${unit}`);
            const countableStrings = Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);
            const summaryString = [...summableStrings, ...countableStrings].join(', ');

            onTransferPrepared(bundleResult.bundleData, recipientId, summaryString);

        } catch (e) {
            const msg = `Failed to create transfer bundle: ${e}`;
            logger.error(msg);
            setFeedbackMsg(msg);
            setIsProcessing(false);
            setShowConfirm(false); // Modal schließen bei Fehler
        }
    }

    return (
        <div className="flex flex-col h-full max-w-4xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center gap-4 mb-2">
                    <button
                        onClick={onBack}
                        className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                        title="Cancel"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    <h1 className="text-2xl font-bold text-theme-primary">Create Transfer</h1>
                </div>
                <p className="text-theme-light ml-14">Select vouchers and prepare a transfer for a recipient.</p>
            </header>
            <div className="flex-grow overflow-y-auto pr-4 -mr-4">
                <form onSubmit={handlePrepareTransferClick}>
                    {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}
                    <section className="bg-bg-card border border-theme-subtle rounded-lg p-4 mb-6">
                        <h2 className="font-semibold text-theme-secondary mb-3">1. The Order</h2>
                        <div className="space-y-4">
                            <div className="relative">
                                <label htmlFor="recipientId" className="block text-sm font-medium text-theme-light mb-1">Recipient User ID</label>
                                <div className="flex gap-2">
                                    <div className="relative flex-1">
                                        {currentContact ? (
                                            // Badge display when contact is selected
                                            <div className="flex items-center gap-2">
                                                <ContactBadge did={currentContact.did} contacts={contacts} size="md" />
                                                <button
                                                    type="button"
                                                    onClick={() => setRecipientId('')}
                                                    className="flex-shrink-0 p-1 rounded hover:bg-theme-accent/20 text-theme-light hover:text-red-500 transition-colors"
                                                    title="Remove contact"
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6 v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                    </svg>
                                                </button>
                                            </div>
                                        ) : (
                                            // Normal input when no contact is selected
                                            <input
                                                id="recipientId"
                                                type="text"
                                                placeholder="did:key:z..."
                                                value={recipientId}
                                                onChange={handleRecipientChange}
                                                onFocus={() => recipientId.length > 0 && suggestions.length > 0 && setShowSuggestions(true)}
                                                onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
                                                className={`w-full p-3 border rounded-lg text-base transition-all duration-150 ease-in-out focus:outline-none focus:ring-2 focus:border-transparent placeholder-theme-placeholder ${
                                                    recipientError
                                                        ? 'border-4 border-red-500 focus:ring-red-500 text-red-900'
                                                        : 'border border-theme-light-border text-theme-secondary focus:ring-theme-primary'
                                                }`}
                                            />
                                        )}
                                        {/* Suggestions List */}
                                        {showSuggestions && (
                                            <div className="absolute z-30 w-full mt-1 bg-white border border-theme-subtle rounded-xl shadow-xl overflow-hidden animate-in fade-in slide-in-from-top-1 duration-200">
                                                {suggestions.map(contact => (
                                                    <button
                                                        key={contact.did}
                                                        type="button"
                                                        onClick={() => selectContact(contact)}
                                                        className="w-full flex items-center gap-3 p-3 hover:bg-bg-app text-left transition-colors border-b border-theme-subtle last:border-0"
                                                    >
                                                        <div className="w-8 h-8 rounded-full overflow-hidden flex-shrink-0">
                                                            <Avatar size={32} name={contact.did} variant="beam" colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]} />
                                                        </div>
                                                        <div className="flex-1 min-w-0">
                                                            <p className="text-sm font-bold text-theme-secondary truncate">
                                                                {contact.profile.first_name || contact.profile.last_name 
                                                                    ? `${contact.profile.first_name || ''} ${contact.profile.last_name || ''}`.trim()
                                                                    : contact.profile.organization || 'Anonymous'}
                                                            </p>
                                                            <p className="text-[10px] text-theme-light font-mono truncate">{contact.did}</p>
                                                        </div>
                                                    </button>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                    <button
                                        type="button"
                                        onClick={() => setShowContactPicker(true)}
                                        className="bg-white border border-theme-subtle p-2.5 rounded-xl text-theme-light hover:text-theme-primary hover:border-theme-primary transition-all shadow-sm active:scale-95"
                                        title="Open Address Book"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
                                        </svg>
                                    </button>
                                </div>
                                
                                {typeof trustStatus === 'object' && 'KnownOffender' in trustStatus && (
                                    <div className="mt-3 bg-red-50 border border-red-200 rounded-xl p-4 animate-in fade-in slide-in-from-top-2 duration-300">
                                        <div className="flex gap-3">
                                            <span className="text-xl">⚠️</span>
                                            <div>
                                                <p className="text-sm font-bold text-red-800">High Risk Recipient</p>
                                                <p className="text-xs text-red-700 leading-relaxed mt-1">
                                                    CAUTION: This address has been linked to unresolved double-spend conflicts in the past. 
                                                    This could be a mistake, but it represents a high risk. Proceed at your own risk.
                                                </p>
                                            </div>
                                        </div>
                                    </div>
                                )}
                            </div>
                            {/* --- NEU START --- */}
                            <div>
                                <label htmlFor="notes" className="block text-sm font-medium text-theme-light mb-1">Notes / Purpose (Optional)</label>
                                <Textarea
                                    id="notes"
                                    placeholder="e.g., Monthly contribution, Coffee..."
                                    value={notes}
                                    onChange={(e: ChangeEvent<HTMLTextAreaElement>) => setNotes(e.target.value)}
                                    rows={2}
                                />
                            </div>
                            <div>
                                <div className="flex items-center mb-2">
                                     <input
                                        id="sendProfileName"
                                        type="checkbox"
                                        checked={sendProfileName}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) => setSendProfileName(e.target.checked)}
                                        className="h-4 w-4 text-theme-accent border-input-border rounded focus:ring-theme-accent"
                                    />
                                    <label htmlFor="sendProfileName" className="ml-2 block text-sm font-medium text-theme-light">
                                        Include sender name in transaction
                                    </label>
                                </div>
                                {sendProfileName && (
                                    <div className="ml-6 animate-in fade-in duration-200">
                                        <label htmlFor="customSenderName" className="block text-xs font-semibold text-theme-secondary mb-1">
                                            Sender Name
                                        </label>
                                        <Input
                                            id="customSenderName"
                                            type="text"
                                            value={customSenderName}
                                            onChange={(e: ChangeEvent<HTMLInputElement>) => setCustomSenderName(e.target.value)}
                                            placeholder="e.g. John Doe"
                                            className="w-full sm:max-w-xs"
                                        />
                                        <p className="text-[10px] text-theme-light mt-1">
                                            <strong>Note:</strong> Only the direct recipient can read this name; it remains completely hidden from subsequent recipients. Providing a clear sender name builds trust and helps the receiver keep track of their incoming transactions.
                                        </p>
                                    </div>
                                )}
                            </div>
                            
                            {/* NEU: Privacy Mode Segmented Control */}
                            <div id="privacyMode" className={`pt-2 border-t border-theme-subtle/50 mt-2 transition-all duration-150 ease-in-out ${privacyError ? 'border-4 border-red-500' : ''}`}>
                                <label className={`block text-xs font-medium mb-1.5 ${privacyError ? 'text-red-900' : 'text-theme-light'}`}>
                                    Privacy Mode
                                </label>
                                
                                {privacyRules.forced ? (
                                    // Show read-only state when forced by standard
                                    <div className="inline-flex items-center gap-1.5 px-2.5 py-1.5 bg-bg-input-readonly rounded-md border border-theme-subtle text-xs">
                                        <span>{privacyRules.mode === 'Stealth' ? '🔒' : '👁️'}</span>
                                        <span className={`font-medium ${privacyRules.mode === 'Stealth' ? 'text-theme-accent' : 'text-theme-secondary'}`}>
                                            {privacyRules.mode === 'Stealth' ? 'Stealth' : 'Public'}
                                        </span>
                                        <span className="text-theme-light">(forced)</span>
                                    </div>
                                ) : (
                                    <div className="flex items-center gap-2">
                                        {/* Show 2-state segmented control for flexible mode */}
                                        <div className={`inline-flex rounded-md border overflow-hidden transition-all duration-150 ease-in-out ${
                                            privacyError 
                                                ? 'border-4 border-red-500' 
                                                : 'border border-theme-subtle'
                                        }`}>
                                            <button
                                                type="button"
                                                onClick={() => setPrivacyMode('public')}
                                                disabled={privacyRules.mode === 'Incompatible'}
                                                className={`px-3 py-1.5 text-xs font-medium transition-colors border-r ${
                                                    privacyError
                                                        ? 'border-red-500 text-red-900'
                                                        : 'border-theme-subtle'
                                                } ${
                                                    privacyMode === 'public'
                                                        ? 'bg-blue-50 text-blue-700'
                                                        : privacyError
                                                            ? 'bg-red-50 hover:bg-red-100'
                                                            : 'bg-white text-theme-light hover:bg-bg-input-readonly'
                                                } ${privacyRules.mode === 'Incompatible' ? 'opacity-50 cursor-not-allowed' : ''}`}
                                            >
                                                👁️ Public
                                            </button>
                                            <button
                                                type="button"
                                                onClick={() => setPrivacyMode('stealth')}
                                                disabled={privacyRules.mode === 'Incompatible'}
                                                className={`px-3 py-1.5 text-xs font-medium transition-colors ${
                                                    privacyMode === 'stealth'
                                                        ? 'bg-purple-50 text-purple-700'
                                                        : privacyError
                                                            ? 'bg-red-50 hover:bg-red-100'
                                                            : 'bg-white text-theme-light hover:bg-bg-input-readonly'
                                                } ${privacyRules.mode === 'Incompatible' ? 'opacity-50 cursor-not-allowed' : ''}`}
                                            >
                                                🔒 Stealth
                                            </button>
                                        </div>
                                        {/* Show hint when not selected */}
                                        {privacyRules.mode === 'Flexible' && privacyMode === null && (
                                            <span className={`text-xs italic ${privacyError ? 'text-red-500' : 'text-theme-accent'}`}>select privacy mode</span>
                                        )}
                                    </div>
                                )}
                                
                                {privacyRules.mode === 'Incompatible' && (
                                    <p className="mt-1.5 text-[10px] text-red-500 font-medium">
                                        ⚠️ Incompatible privacy rules
                                    </p>
                                )}
                                
                                {/* Info Box for Privacy Modes */}
                                <details className="mt-3 text-xs group">
                                    <summary className="cursor-pointer text-theme-light hover:text-theme-primary flex items-center gap-1 font-medium list-none select-none">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 text-theme-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                                        </svg>
                                        How do Privacy Modes work?
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-3 w-3 ml-1 transition-transform group-open:rotate-180 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                                        </svg>
                                    </summary>
                                    <div className="mt-2 p-3 bg-bg-app rounded-md border border-theme-subtle text-theme-secondary space-y-3 leading-relaxed">
                                        <p>
                                            <strong>🔒 Stealth Mode:</strong> If the voucher is passed on, nobody will know you sent it. Your sender ID is hidden. This protects against mass surveillance by states or third parties. If everyone uses stealth, observers can only see how many times a voucher was transferred, but not by whom. <br/>
                                            <em>Note: Fraud can still be detected. In the event of a double-spend, the sender's identity is cryptographically revealed.</em>
                                        </p>
                                        <p>
                                            <strong>👁️ Public Mode:</strong> Useful in trusted environments or when you have no fear of negative consequences. Revealing your identity can increase trust, as it signals transparency and makes any potential fraud easier to trace.
                                        </p>
                                        <div className="border-t border-theme-subtle/50 pt-2 mt-1">
                                            <p className="text-[10px] text-theme-light italic">
                                                Important: Regardless of the mode chosen, the original creator of a voucher is always visible in plain text, and their first transaction transferring the voucher is always transparent.
                                            </p>
                                        </div>
                                    </div>
                                </details>
                            </div>
                            {/* --- NEU ENDE --- */}
                            <div>
                                <label className="block text-sm font-medium text-theme-light mb-1">Filter by Standard</label>
                                <div className="flex flex-wrap gap-2">
                                    <button type="button" onClick={() => handleStandardSelect(null)} className={`px-3 py-1 text-sm rounded-full ${selectedStandardId === null ? 'bg-theme-accent text-white font-semibold' : 'bg-input-readonly hover:bg-theme-subtle'}`}>All</button>
                                    {voucherStandards.map(standard => {
                                        const displayName = standardIdToNameMap.get(standard.id) || standard.id;
                                        return (
                                            <button type="button" key={standard.id} onClick={() => handleStandardSelect(standard.id)} className={`px-3 py-1 text-sm rounded-full ${selectedStandardId === standard.id ? 'bg-theme-accent text-white font-semibold' : 'bg-input-readonly hover:bg-theme-subtle'}`}>{displayName}</button>
                                        );
                                    })}
                                </div>
                                <p className="text-xs text-theme-light mt-2">Select a specific standard to enable automatic mode by entering an amount.</p>
                            </div>
                            {selectedStandardId && (
                                <div>
                                    <label htmlFor="amount" className="block text-sm font-medium text-theme-light mb-1">Amount to Send (Automatic Mode)</label>
                                    <Input id="amount" placeholder="e.g., 50" value={targetAmountStr} onChange={(e: ChangeEvent<HTMLInputElement>) => handleTargetAmountChange(e.target.value)} type="number" min="0" step="any" />
                                </div>
                            )}
                        </div>
                    </section>

                    {/* NEW: COMPACT CHECKOUT SECTION */}
                    <section className="rounded-lg border border-theme-accent/50 bg-bg-card-alternate p-4 mb-6">
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 items-center">
                            <div className="md:col-span-1">
                                <p className="text-sm text-theme-light">{checkoutSummary.count} Vouchers Selected</p>
                                <div className="text-lg font-bold text-theme-primary truncate">
                                    {/* KORREKTUR (von 13:28): Detaillierte Anzeige */}
                                    {checkoutSummary.count > 0 ? (
                                        <>
                                            {Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => (
                                                <span key={unit} className="mr-4">{formatAmount(total.toString())} <span className="text-base font-normal">{unit}</span></span>
                                            ))}
                                            {Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => (
                                                <span key={unit} className="mr-4">{total} <span className="text-base font-normal">{unit}{total > 1 ? 's' : ''}</span></span>
                                            ))}
                                        </>
                                    ) : <span>0.00</span>}
                                </div>
                            </div>

                            <div className="md:col-span-1">
                                 <Button size="lg" type="button" onClick={handlePrepareTransferClick} className="w-full" disabled={isProcessing}>
                                    {isProcessing ? "Processing..." : "Prepare Transfer"}
                                </Button>
                            </div>
                        </div>
                    </section>

                    {/* NEW: SORTING MOVED HERE */}
                    <div className="flex justify-between items-center mb-4">
                        <h2 className="font-semibold text-theme-secondary">3. Your Inventory</h2>
                        <select className="bg-input-readonly border border-theme-subtle rounded-md px-3 py-1.5 text-sm">
                            <option>Sort by: Optimal</option>
                            <option>Sort by: Validity (expiring soonest)</option>
                            <option>Sort by: Amount (highest first)</option>
                            <option>Sort by: Amount (lowest first)</option>
                        </select>
                    </div>

                    <section className="mb-6">
                        <div className="space-y-3">
                            {filteredVouchers.length > 0 ? filteredVouchers.map(v => {
                                // @ts-ignore (wir wissen, dass 'divisible' durch 'enrichedVouchers' hinzugefügt wurde)
                                const selectedAmount = selection.get(v.local_instance_id);
                                const isSelected = selectedAmount !== undefined;
                                return (
                                    <div key={v.local_instance_id} className={`w-full text-left bg-bg-card-alternate rounded-lg border shadow-sm p-3 transition-all duration-150 ease-in-out ${isSelected ? 'border-theme-accent ring-2 ring-theme-accent' : 'border-theme-subtle hover:border-theme-primary'}`}>
                                        <div className="flex justify-between items-start cursor-pointer" onClick={() => handleManualVoucherSelect(v)}>
                                            <div>
                                                <div className="flex items-baseline text-xl font-bold text-theme-primary">
                                                    {/* @ts-ignore */}
                                                    {isSelected && selectedAmount !== v.current_amount ? (
                                                        <>
                                                            <span>{formatAmount(selectedAmount)}</span>
                                                            {/* @ts-ignore */}
                                                            <span className="ml-2 text-base font-normal text-gray-400">of {formatAmount(v.current_amount)}</span>
                                                        </>
                                                    ) : (
                                                        // @ts-ignore
                                                        <span>{formatAmount(v.current_amount)}</span>
                                                    )}
                                                    {/* @ts-ignore */}
                                                    <span className="ml-2 text-base font-normal text-theme-light">{v.unit}</span>
                                                </div>
                                                {/* @ts-ignore */}
                                                <p className="text-xs text-theme-light font-mono">by {v.creator_first_name} {v.creator_last_name}</p>
                                            </div>
                                            <div className="text-right">
                                                {/* @ts-ignore */}
                                                <p className="text-base font-medium text-theme-light">{v.voucher_standard_name}</p>
                                                {/* @ts-ignore */}
                                                <p className="text-xs text-theme-light">until {formatDate(v.valid_until)}</p>
                                            </div>
                                        </div>
                                        {/* @ts-ignore */}
                                        {isSelected && v.divisible && (
                                            <div className="mt-4 p-4 bg-theme-accent/5 rounded-xl border-2 border-theme-accent/20 flex items-center justify-between animate-in fade-in slide-in-from-top-2 duration-200 shadow-inner">
                                                <label className="text-sm font-bold text-theme-primary">Adjust Amount:</label>
                                                <div className="flex items-center gap-2">
                                                    <Input 
                                                        type="number" 
                                                        value={selectedAmount} 
                                                        onChange={(e: ChangeEvent<HTMLInputElement>) => handleVoucherAmountChange(v.local_instance_id, e.target.value, v.current_amount)}
                                                        className="w-32 text-right !py-2 !px-3 h-10 !text-lg !font-black !border-2 !border-theme-accent/40 !bg-white !shadow-sm !rounded-md"
                                                        max={v.current_amount}
                                                        min="0"
                                                        step="any"
                                                    />
                                                    {/* @ts-ignore */}
                                                    <span className="text-sm font-black text-theme-primary">{v.unit}</span>
                                                </div>
                                            </div>
                                        )}
                                    </div>
                                )
                            }) : (
                                <div className="text-center text-theme-light py-8 bg-input-readonly rounded-lg border border-theme-subtle"><p>No active vouchers found matching your criteria.</p></div>
                            )}
                        </div>
                    </section>
                </form>
            </div>
            
            <ConfirmationModal 
                isOpen={showConfirm}
                title="Confirm Transfer"
                description={
                    <p>Are you sure you want to send vouchers worth <br/><span className="font-bold text-theme-primary">{Object.entries(checkoutSummary.summableTotals).map(([u, t]) => `${t.toFixed(2)} ${u}`).join(', ')} {Object.entries(checkoutSummary.countableTotals).map(([u, t]) => `${t} ${u}`).join(', ')}</span><br/> to <span className="font-mono text-xs break-all">{recipientId}</span>?</p>
                }
                confirmText="Yes, Prepare Transfer"
                onConfirm={executeTransfer}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isProcessing}
            />

            {/* Contact Picker Modal */}
            {showContactPicker && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[2px] p-4">
                    <div className="w-full max-w-lg bg-bg-app border border-theme-subtle rounded-2xl shadow-2xl overflow-hidden animate-in fade-in zoom-in duration-200 flex flex-col max-h-[80vh]">
                        <div className="p-4 border-b border-theme-subtle bg-white flex justify-between items-center">
                            <h2 className="text-xl font-extrabold text-theme-secondary">Select Contact</h2>
                            <button onClick={() => setShowContactPicker(false)} className="p-1 hover:bg-bg-app rounded-full transition-colors">
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>
                        <div className="flex-grow overflow-y-auto p-4 space-y-2">
                            {contacts.length === 0 ? (
                                <p className="text-center text-theme-light py-8">No contacts in address book.</p>
                            ) : (
                                contacts.map(contact => (
                                    <button
                                        key={contact.did}
                                        onClick={() => selectContact(contact)}
                                        className="w-full flex items-center gap-4 p-4 bg-white hover:bg-bg-input-readonly border border-theme-subtle rounded-xl transition-all shadow-sm hover:shadow-md text-left"
                                    >
                                        <div className="w-10 h-10 rounded-full overflow-hidden flex-shrink-0 border border-theme-subtle">
                                            <Avatar size={40} name={contact.did} variant="beam" colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]} />
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <p className="font-bold text-theme-secondary truncate">
                                                {contact.profile.first_name || contact.profile.last_name 
                                                    ? `${contact.profile.first_name || ''} ${contact.profile.last_name || ''}`.trim()
                                                    : contact.profile.organization || 'Anonymous'}
                                            </p>
                                            <p className="text-xs text-theme-light font-mono truncate">{contact.did}</p>
                                        </div>
                                    </button>
                                ))
                            )}
                        </div>
                        <div className="p-4 bg-white border-t border-theme-subtle">
                            <Button variant="secondary" onClick={() => setShowContactPicker(false)} className="w-full">Close</Button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}