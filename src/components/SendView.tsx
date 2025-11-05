// src/components/SendView.tsx
import { useState, useEffect, useMemo, FormEvent, ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { VoucherSummary, VoucherStandardInfo, SourceTransfer, TransactionRecord } from "../types";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";

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
    const [recipientId, setRecipientId] = useState("");
    const [notes, setNotes] = useState(""); // NEU
    const [sendProfileName, setSendProfileName] = useState(true); // NEU
    const [ownUserId, setOwnUserId] = useState("");
    const [targetAmountStr, setTargetAmountStr] = useState("");
    const [availableVouchers, setAvailableVouchers] = useState<VoucherSummary[]>([]);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string | null>(null);
    const [selection, setSelection] = useState<Map<string, string>>(new Map());
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isProcessing, setIsProcessing] = useState(false);
    const [password, setPassword] = useState("");
    const [standardIdToUuidMap, setStandardIdToUuidMap] = useState<Map<string, string>>(new Map());

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
                const standards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                const newMap = new Map<string, string>();
                standards.forEach(s => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        newMap.set(s.id, uuidMatch[1]);
                    }
                });
                setStandardIdToUuidMap(newMap);

                // KORREKTUR (von 13:31): Die Annahme des Prototyps wiederherstellen.
                // Die 'divisible'-Eigenschaft kommt noch nicht vom Backend-Summary.
                const enrichedVouchers = activeVouchers.map(v => ({ ...v, divisible: true }));

                setAvailableVouchers(enrichedVouchers);
                setVoucherStandards(standards);
                setOwnUserId(userId);
                logger.info(`3. Set state with ${enrichedVouchers.length} vouchers to be displayed.`);
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


    useEffect(() => {
        if (!selectedStandardId || !targetAmountStr) {
            setSelection(new Map());
            return;
        }
        const targetAmount = parseFloat(targetAmountStr);
        if (isNaN(targetAmount) || targetAmount <= 0) {
            setSelection(new Map());
            return;
        }
        const newSelection = new Map<string, string>();
        let currentTotal = 0;

        // Finde den ausgewählten Standard, um die korrekte Präzision zu ermitteln.
        const selectedStandard = voucherStandards.find(s => s.id === selectedStandardId);
        let precision = 4; // Ein sicherer Standard-Fallback.

        // ROBUSTE LÖSUNG: Parse die Präzision direkt aus dem TOML-Content,
        // da das Backend sie nicht als separates Feld bereitstellt.
        if (selectedStandard) {
            const match = selectedStandard.content.match(/amount_decimal_places\s*=\s*(\d+)/);
            if (match && match[1]) {
                precision = parseInt(match[1], 10);
            }
        }

        const sortedVouchers = [...filteredVouchers].sort((a, b) => parseFloat(a.current_amount) - parseFloat(b.current_amount));
        for (const voucher of sortedVouchers) {
            const voucherAmount = parseFloat(voucher.current_amount);
            // KORREKTUR: Stelle sicher, dass 'divisible' geprüft wird (kommt von enrichedVouchers)
            if (currentTotal + voucherAmount <= targetAmount) {
                newSelection.set(voucher.local_instance_id, voucher.current_amount);
                currentTotal += voucherAmount;
            } else {
                const neededAmount = targetAmount - currentTotal;
                if (voucher.divisible) {
                    // KORREKTUR: Wende die dynamisch ermittelte Präzision an.
                    newSelection.set(voucher.local_instance_id, neededAmount.toFixed(precision));
                    currentTotal += neededAmount;
                    break;
                }
            }
            if (currentTotal >= targetAmount) break;
        }
        if (currentTotal < targetAmount) {
            setFeedbackMsg("The entered amount cannot be met with the available vouchers.");
        } else {
            setFeedbackMsg("");
        }
        setSelection(newSelection);
    }, [targetAmountStr, selectedStandardId, filteredVouchers]);


    const handleManualVoucherSelect = (voucher: VoucherSummary) => {
        setTargetAmountStr("");
        const newSelection = new Map(selection);
        if (newSelection.has(voucher.local_instance_id)) {
            newSelection.delete(voucher.local_instance_id);
        } else {
            newSelection.set(voucher.local_instance_id, voucher.current_amount);
        }
        setSelection(newSelection);
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

    async function handlePrepareTransfer(event: FormEvent) {
        event.preventDefault();
        if (!recipientId || selection.size === 0 || !password) {
            setFeedbackMsg("Please provide a recipient ID, select vouchers, and enter your password.");
            return;
        }
        setIsProcessing(true);
        setFeedbackMsg("");
        try {
            const senderProfileNameToSend = sendProfileName && profileName ? profileName : null;
            const notesToSend = notes.trim() === "" ? null : notes.trim();

            const sources: SourceTransfer[] = Array.from(selection.entries()).map(([id, amount]) => ({
                local_instance_id: id,
                amount_to_send: amount,
            }));
            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(standard => {
                standardDefinitionsToml[standard.id] = standard.content;
            });

            // AKTUALISIERT: Erwarte ein Objekt statt nur bytes, um die bundle_id für den Record zu haben.
            const bundleResult = await invoke<{ bundleData: number[], bundleId: string }>("create_transfer_bundle", {
                recipientId,
                sources,
                notes: notesToSend, // NEU
                senderProfileName: senderProfileNameToSend, // NEU
                standardDefinitionsToml,
                password
            });

            logger.info(`Successfully created transfer bundle ${bundleResult.bundleId} with ${bundleResult.bundleData.length} bytes.`);

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
                involved_vouchers: Array.from(selection.keys()),
                bundle_data: bundleResult.bundleData,
                bundle_id: bundleResult.bundleId, // NEU
                notes: notesToSend ?? undefined, // NEU
                sender_profile_name: senderProfileNameToSend ?? undefined, // NEU
            };

            await invoke("save_transaction_record", { record: record as TransactionRecord, password });
            logger.info(`Transaction record ${record.id} saved successfully.`);

            // KORREKTUR (von 13:28): Detaillierte summaryString erstellen
            const summableStrings = Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => `${formatAmount(total.toString())} ${unit}`);
            const countableStrings = Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);
            const summaryString = [...summableStrings, ...countableStrings].join(', ');

            onTransferPrepared(bundleResult.bundleData, recipientId, summaryString);

        } catch (e) {
            const msg = `Failed to create transfer bundle: ${e}`;
            logger.error(msg);
            setIsProcessing(false);
        }
    }

    return (
        <div className="flex flex-col h-full max-w-4xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-bold text-theme-primary">Create Transfer</h1>
                    <Button variant="secondary" onClick={onBack}>Cancel</Button>
                </div>
                <p className="text-theme-light mt-1">Select vouchers and prepare a transfer for a recipient.</p>
            </header>
            <div className="flex-grow overflow-y-auto pr-4 -mr-4">
                <form onSubmit={handlePrepareTransfer}>
                    {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}
                    <section className="bg-bg-card border border-theme-subtle rounded-lg p-4 mb-6">
                        <h2 className="font-semibold text-theme-secondary mb-3">1. The Order</h2>
                        <div className="space-y-4">
                            <div>
                                <label htmlFor="recipientId" className="block text-sm font-medium text-theme-light mb-1">Recipient User ID</label>
                                <Input id="recipientId" placeholder="did:key:z..." value={recipientId} onChange={(e: ChangeEvent<HTMLInputElement>) => setRecipientId(e.target.value)} required />
                            </div>
                            {/* --- NEU START --- */}
                            <div>
                                <label htmlFor="notes" className="block text-sm font-medium text-theme-light mb-1">Notes / Verwendungszweck (Optional)</label>
                                <Textarea
                                    id="notes"
                                    placeholder="e.g., Monthly contribution, Coffee..."
                                    value={notes}
                                    onChange={(e: ChangeEvent<HTMLTextAreaElement>) => setNotes(e.target.value)}
                                    rows={2}
                                />
                            </div>
                            <div>
                                <div className="flex items-center">
                                    <input
                                        id="sendProfileName"
                                        type="checkbox"
                                        checked={sendProfileName}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) => setSendProfileName(e.target.checked)}
                                        disabled={!profileName}
                                        className="h-4 w-4 text-theme-accent border-input-border rounded focus:ring-theme-accent"
                                    />
                                    <label htmlFor="sendProfileName" className="ml-2 block text-sm text-theme-light">
                                        Send my profile name ({profileName || 'No name set'})
                                    </label>
                                </div>
                                {!profileName && <p className="text-xs text-theme-light mt-1">Profile name not available to send.</p>}
                            </div>
                            {/* --- NEU ENDE --- */}
                            <div>
                                <label className="block text-sm font-medium text-theme-light mb-1">Filter by Standard</label>
                                <div className="flex flex-wrap gap-2">
                                    <button type="button" onClick={() => setSelectedStandardId(null)} className={`px-3 py-1 text-sm rounded-full ${selectedStandardId === null ? 'bg-theme-accent text-white font-semibold' : 'bg-input-readonly hover:bg-theme-subtle'}`}>All</button>
                                    {voucherStandards.map(standard => (
                                        <button type="button" key={standard.id} onClick={() => setSelectedStandardId(standard.id)} className={`px-3 py-1 text-sm rounded-full ${selectedStandardId === standard.id ? 'bg-theme-accent text-white font-semibold' : 'bg-input-readonly hover:bg-theme-subtle'}`}>{standard.id}</button>
                                    ))}
                                </div>
                                <p className="text-xs text-theme-light mt-2">Select a specific standard to enable automatic mode by entering an amount.</p>
                            </div>
                            {selectedStandardId && (
                                <div>
                                    <label htmlFor="amount" className="block text-sm font-medium text-theme-light mb-1">Amount to Send (Automatic Mode)</label>
                                    <Input id="amount" placeholder="e.g., 50" value={targetAmountStr} onChange={(e: ChangeEvent<HTMLInputElement>) => setTargetAmountStr(e.target.value)} type="number" />
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
                                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} required disabled={isProcessing} autoComplete="current-password" placeholder="Your Wallet Password"/>
                            </div>
                            <div className="md:col-span-1">
                                <Button size="lg" type="submit" className="w-full" disabled={!recipientId || selection.size === 0 || isProcessing || !password}>
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
                                    <button type="button" key={v.local_instance_id} onClick={() => handleManualVoucherSelect(v)} className={`w-full text-left bg-bg-card-alternate rounded-lg border shadow-sm p-3 transition-all duration-150 ease-in-out ${isSelected ? 'border-theme-accent ring-2 ring-theme-accent' : 'border-theme-subtle hover:border-theme-primary'}`}>
                                        <div className="flex justify-between items-start">
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
                                    </button>
                                )
                            }) : (
                                <div className="text-center text-theme-light py-8 bg-input-readonly rounded-lg border border-theme-subtle"><p>No active vouchers found matching your criteria.</p></div>
                            )}
                        </div>
                    </section>
                </form>
            </div>
        </div>
    );
}