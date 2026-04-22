// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { AggregatedBalance, TransactionRecord, VoucherSummary, Contact } from "../types";

interface DashboardProps {
    onNavigateToCreateVoucher: () => void;
    onNavigateToSend: () => void;
    onNavigateToHistory: () => void;
    onNavigateToReceive: () => void;
    onNavigateToConflicts?: () => void;
    onNavigateToWallet: (filter?: { status?: string; standard?: string }) => void;
    onNavigateToSettings?: () => void;
    profileName: string;
}

export function Dashboard(props: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [recentTransactions, setRecentTransactions] = useState<TransactionRecord[]>([]);
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copied, setCopied] = useState(false);
    const [activeVouchersCount, setActiveVouchersCount] = useState(0);
    const [incompleteCount, setIncompleteCount] = useState(0);
    const [quarantinedCount, setQuarantinedCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [isProfileComplete, setIsProfileComplete] = useState(true);
    const [voucherCountsByStandard, setVoucherCountsByStandard] = useState<Record<string, number>>({});

    useEffect(() => {
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, history, voucherSummaries, userProfile, contactsList] = await Promise.all([
                    invoke<string>("get_user_id"),
                    invoke<AggregatedBalance[]>("get_total_balance_by_currency"),
                    invoke<TransactionRecord[]>("get_transaction_history").catch(() => []),
                    invoke<VoucherSummary[]>("get_voucher_summaries").catch(() => []),
                    invoke<any>("get_user_profile").catch(() => null),
                    invoke<Contact[]>("get_contacts").catch(() => [])
                ]);
                setUserId(id);
                setBalances(balanceList || []);
                setContacts(contactsList || []);
                // Sort by timestamp, newest first, and take only the first 5
                const sortedHistory = (history || [])
                    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
                    .slice(0, 5);
                setRecentTransactions(sortedHistory);
                
                // Calculate voucher counts
                const activeCount = (voucherSummaries || []).filter(v => v.status === "Active").length;
                const incompleteCount = (voucherSummaries || []).filter(v => {
                    return typeof v.status === 'object' && 'Incomplete' in v.status;
                }).length;
                const quarantinedCount = (voucherSummaries || []).filter(v => {
                    return typeof v.status === 'object' && 'Quarantined' in v.status;
                }).length;

                // Calculate voucher counts by standard (only active vouchers)
                const countsByStandard: Record<string, number> = {};
                (voucherSummaries || [])
                    .filter(v => v.status === "Active")
                    .forEach(v => {
                        const standardUuid = v.voucher_standard_uuid;
                        if (standardUuid) {
                            countsByStandard[standardUuid] = (countsByStandard[standardUuid] || 0) + 1;
                        }
                    });
                setVoucherCountsByStandard(countsByStandard);

                setActiveVouchersCount(activeCount);
                setIncompleteCount(incompleteCount);
                setQuarantinedCount(quarantinedCount);

                // Check if profile is complete (requires first_name and last_name)
                const profileComplete = userProfile && userProfile.first_name && userProfile.last_name;
                setIsProfileComplete(!!profileComplete);
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${e}`;
                console.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            } finally {
                setIsLoading(false);
            }
        }
        fetchData();
    }, []);

    async function handleCopyUserId() {
        if (!userId) return;
        try {
            await writeText(userId);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        } catch (e) {
            const msg = `Failed to copy User ID: ${e}`;
            console.error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toString();
    }

    function formatTimestamp(isoString: string): string {
        return new Date(isoString).toLocaleDateString(undefined, {
            month: 'short',
            day: 'numeric'
        });
    }

    function formatSummary(
        summable: Record<string, string> | undefined,
        countable: Record<string, number> | undefined
    ): string {
        const s = Object.entries(summable || {}).map(([unit, amount]) => `${amount} ${unit}`);
        const c = Object.entries(countable || {}).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);
        const all = [...s, ...c];
        return all.length > 0 ? all.join(', ') : '0.00';
    }

    function getContactName(did: string, contacts: Contact[]): string {
        const contact = contacts.find(c => c.did === did);
        if (contact) {
            const firstName = contact.profile.first_name || '';
            const lastName = contact.profile.last_name || '';
            const org = contact.profile.organization || '';
            if (firstName || lastName) {
                return `${firstName} ${lastName}`.trim();
            }
            if (org) {
                return org;
            }
        }
        return did.substring(0, 10) + '...';
    }

    function getContactForTransaction(record: TransactionRecord, contacts: Contact[]): { name: string; notes?: string } {
        const did = record.direction === 'sent' ? record.recipient_id : record.sender_id;
        const name = did ? getContactName(did, contacts) : 'Unknown';
        const notes = record.notes;
        return { name, notes };
    }

    const truncatedUserId = userId ? `${userId.substring(0, 15)}...${userId.substring(userId.length - 8)}` : "Lade...";

    return (
        <div className="flex flex-col h-full">
            {/* Fixierte Kopfleiste */}
            <header className="flex-shrink-0 border-b border-theme-subtle px-6 py-0.5 bg-bg-card">
                <div className="flex items-center justify-between gap-3 text-xs text-theme-light">
                    <div className="flex items-center gap-3">
                        <span className="relative flex h-2 w-2">
                            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                        </span>
                        <div className="flex items-center gap-1">
                            <span>Profile:</span>
                            <span className="font-semibold text-theme-secondary">{props.profileName}</span>
                        </div>
                    </div>
                    
                    <div className="flex items-center gap-2">
                        <span className="font-semibold text-theme-secondary">Your ID:</span>
                        <button 
                            onClick={handleCopyUserId}
                            title={copied ? "Copied!" : "Click to copy User ID"}
                            className={`font-mono text-xs sm:text-sm font-medium transition-all duration-200 border px-3 py-1 rounded-md ${
                                copied 
                                ? 'bg-theme-success/10 text-theme-success border-theme-success/30 shadow-sm' 
                                : 'bg-bg-card text-theme-primary border-theme-subtle hover:border-theme-primary hover:bg-theme-primary/5 hover:shadow-sm'
                            }`}
                        >
                            {copied ? "✓ Copied!" : truncatedUserId}
                        </button>
                    </div>
                </div>
            </header>

            {/* Main content area that is scrollable */}
            <div className="flex-grow overflow-y-auto p-4 sm:p-6">
                <div className="mx-auto max-w-4xl">
                    {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

                    {/* Profile Warning Banner */}
                    {!isProfileComplete && (
                        <div className="mb-6 bg-amber-50 border-l-4 border-amber-500 p-4 rounded-md shadow-sm animate-in fade-in slide-in-from-top-2 duration-500">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center">
                                    <div className="flex-shrink-0 text-amber-500 text-2xl">⚠️</div>
                                    <div className="ml-3">
                                        <p className="text-sm font-semibold text-amber-800">
                                            Your profile is incomplete.
                                        </p>
                                        <p className="text-xs text-amber-700">
                                            A complete profile is essential for building trust in the network.
                                        </p>
                                    </div>
                                </div>
                                {props.onNavigateToSettings && (
                                    <Button
                                        onClick={props.onNavigateToSettings}
                                        variant="primary"
                                        size="sm"
                                        className="bg-amber-600 hover:bg-amber-700 text-white"
                                    >
                                        Complete Profile
                                    </Button>
                                )}
                            </div>
                        </div>
                    )}

                    {/* Zone 1: Quarantine & Double Spends (Red Warning) */}
                    {quarantinedCount > 0 && (
                        <div 
                            className="mb-6 bg-red-50 border-l-4 border-red-500 p-4 rounded-md shadow-sm animate-in fade-in slide-in-from-top-2 duration-500 cursor-pointer hover:bg-red-100 transition-colors"
                            onClick={props.onNavigateToConflicts}
                        >
                            <div className="flex items-center justify-between">
                                <div className="flex items-center">
                                    <div className="flex-shrink-0 text-red-500 text-2xl">🚫</div>
                                    <div className="ml-3">
                                        <p className="text-sm font-bold text-red-800">
                                            Quarantine Warning: {quarantinedCount} voucher{quarantinedCount > 1 ? 's' : ''} locked
                                        </p>
                                        <p className="text-xs text-red-700">
                                            Double-spend detected. Please review the conflict resolution view.
                                        </p>
                                    </div>
                                </div>
                                <div className="text-red-500">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                                    </svg>
                                </div>
                            </div>
                        </div>
                    )}

                    {/* Zone 2: Kaufkraft & Inventar (Central) */}
                    <section className="text-center mb-8">
                        {isLoading ? (
                            <div className="animate-pulse">
                                <div className="h-20 bg-gray-200 rounded mx-auto w-48 mb-2"></div>
                                <div className="h-4 bg-gray-200 rounded mx-auto w-32"></div>
                            </div>
                        ) : (
                            <>
                                <h1 className="text-sm font-semibold text-theme-light uppercase tracking-wider mb-8">Balances</h1>
                                <div className="flex flex-wrap justify-center gap-8 md:gap-12">
                                    {balances.map((balance) => {
                                        const count = voucherCountsByStandard[balance.standard_uuid] || 0;
                                        return (
                                            <div
                                                key={balance.standard_uuid}
                                                onClick={() => props.onNavigateToWallet({ standard: balance.standard_name, status: 'active' })}
                                                className="flex flex-col items-center hover:opacity-80 transition-opacity cursor-pointer group"
                                            >
                                                <div className="flex items-baseline mb-2">
                                                    <p className="text-5xl md:text-6xl font-bold text-theme-primary transition-colors group-hover:text-theme-accent">
                                                        {formatAmount(balance.total_amount)}
                                                    </p>
                                                    <span className="text-2xl md:text-3xl font-normal text-theme-light ml-2">{balance.unit}</span>
                                                </div>
                                                <p className="font-semibold text-theme-secondary text-lg">
                                                    {balance.standard_name}
                                                </p>
                                                <p className="text-sm text-theme-light mt-1">
                                                    ({count} voucher{count !== 1 ? 's' : ''})
                                                </p>
                                            </div>
                                        );
                                    })}
                                </div>
                            </>
                        )}
                    </section>

                    {/* Zone 3: Dynamic Quick-Actions */}
                    <section className="flex justify-center gap-4 mb-6">
                        {activeVouchersCount === 0 ? (
                            // Scenario A: 0 active vouchers - Create is primary
                            <>
                                <Button 
                                    onClick={props.onNavigateToCreateVoucher}
                                    className="px-8 py-4 text-lg"
                                    variant="primary"
                                >
                                    Create Voucher
                                </Button>
                                <Button 
                                    onClick={props.onNavigateToReceive} 
                                    className="px-8 py-4 text-lg"
                                    variant="secondary"
                                >
                                    Receive
                                </Button>
                                <Button 
                                    onClick={props.onNavigateToSend} 
                                    className="px-8 py-4 text-lg"
                                    variant="secondary"
                                    disabled
                                >
                                    Send
                                </Button>
                            </>
                        ) : (
                            // Scenario B: >0 active vouchers - Send is primary
                            <>
                                <Button 
                                    onClick={props.onNavigateToSend} 
                                    className="px-8 py-4 text-lg"
                                    variant="primary"
                                >
                                    Send
                                </Button>
                                <Button 
                                    onClick={props.onNavigateToReceive} 
                                    className="px-8 py-4 text-lg"
                                    variant="secondary"
                                >
                                    Receive
                                </Button>
                                <Button 
                                    onClick={props.onNavigateToCreateVoucher}
                                    className="px-6 py-4 text-lg border border-theme-subtle text-theme-light hover:border-theme-primary hover:text-theme-primary"
                                    variant="secondary"
                                >
                                    Create
                                </Button>
                            </>
                        )}
                    </section>

                    {/* Zone 4: Action Area / To-Dos (Incomplete Vouchers) */}
                    {incompleteCount > 0 && (
                        <div 
                            className="mb-6 bg-yellow-50 border-l-4 border-yellow-500 p-4 rounded-md shadow-sm cursor-pointer hover:bg-yellow-100 transition-colors"
                            onClick={() => props.onNavigateToWallet({ status: 'incomplete' })}
                        >
                            <div className="flex items-center justify-between">
                                <div className="flex items-center">
                                    <div className="flex-shrink-0 text-yellow-500 text-2xl">⏳</div>
                                    <div className="ml-3">
                                        <p className="text-sm font-semibold text-yellow-800">
                                            {incompleteCount} voucher{incompleteCount > 1 ? 's' : ''} incomplete
                                        </p>
                                        <p className="text-xs text-yellow-700">
                                            Need more signatures. Click to view in wallet.
                                        </p>
                                    </div>
                                </div>
                                <div className="text-yellow-500">
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                                    </svg>
                                </div>
                            </div>
                        </div>
                    )}

                    {/* Zone 5: Recent Activity & History Link OR Welcome Empty State */}
                    {recentTransactions.length > 0 ? (
                        <section className="mb-8">
                            <div className="flex items-center justify-between mb-4">
                                <h2 className="text-lg font-semibold text-theme-secondary">Recent Activity</h2>
                                <button 
                                    onClick={props.onNavigateToHistory}
                                    className="text-sm text-theme-accent hover:underline"
                                >
                                    View All ➔
                                </button>
                            </div>
                            <div className="space-y-3">
                                {recentTransactions.map(record => {
                                    const { name, notes } = getContactForTransaction(record, contacts);
                                    const truncatedNotes = notes ? notes.substring(0, 30) + (notes.length > 30 ? '...' : '') : '';
                                    return (
                                        <button
                                            key={record.id}
                                            onClick={props.onNavigateToHistory}
                                            className="w-full bg-bg-card-alternate rounded-lg p-3 border border-theme-subtle flex items-center justify-between hover:border-theme-primary hover:shadow-sm transition-all text-left group"
                                        >
                                            <div className="flex items-center gap-3">
                                                <div className={`flex items-center justify-center h-8 w-8 rounded-full transition-transform group-hover:scale-110 ${record.direction === 'sent' ? 'bg-red-200 text-red-800' : 'bg-green-200 text-green-800'}`}>
                                                    {record.direction === 'sent' ? '↑' : '↓'}
                                                </div>
                                                <div>
                                                    <p className="font-semibold text-sm text-theme-primary capitalize">
                                                        {record.direction === 'sent' ? 'Sent' : 'Received'}
                                                    </p>
                                                    <p className="text-xs text-theme-secondary font-medium mt-0.5">
                                                        {name}{truncatedNotes ? ` · ${truncatedNotes}` : ''}
                                                    </p>
                                                </div>
                                            </div>
                                            <div className="text-right">
                                                <p className="font-semibold text-theme-secondary">
                                                    {formatSummary(record.summableAmounts, record.countableItems)}
                                                </p>
                                                <p className="text-xs text-theme-light mt-0.5">
                                                    {formatTimestamp(record.timestamp)}
                                                </p>
                                            </div>
                                        </button>
                                    );
                                })}
                            </div>
                        </section>
                    ) : (
                        // Welcome Empty State - shows when wallet is completely empty
                        balances.length === 0 && activeVouchersCount === 0 && incompleteCount === 0 && (
                            <section className="mb-8 bg-gradient-to-br from-blue-50 to-indigo-50 border border-blue-200 rounded-xl p-8 shadow-sm animate-in fade-in slide-in-from-bottom-4 duration-500">
                                <div className="text-center">
                                    <h2 className="text-2xl font-bold text-theme-primary mb-4">👋 Welcome to your Wallet!</h2>
                                    <p className="text-theme-secondary mb-6">Your balance is currently empty. You have two ways to get started:</p>
                                    
                                    <div className="grid md:grid-cols-2 gap-6 max-w-2xl mx-auto">
                                        {/* Option 1: Receive */}
                                        <div className="bg-white rounded-lg p-6 border border-theme-subtle shadow-sm hover:shadow-md transition-shadow">
                                            <div className="text-3xl mb-3">📥</div>
                                            <h3 className="text-lg font-semibold text-theme-primary mb-2">Receive Vouchers</h3>
                                            <p className="text-sm text-theme-light mb-4">
                                                Receive vouchers in exchange for goods or services. Simply share your ID or QR code with your partner.
                                            </p>
                                            <Button 
                                                onClick={props.onNavigateToReceive}
                                                variant="secondary"
                                                className="w-full"
                                            >
                                                Receive
                                            </Button>
                                        </div>
                                        
                                        {/* Option 2: Create */}
                                        <div className="bg-white rounded-lg p-6 border border-theme-subtle shadow-sm hover:shadow-md transition-shadow">
                                            <div className="text-3xl mb-3">✨</div>
                                            <h3 className="text-lg font-semibold text-theme-primary mb-2">Create Vouchers</h3>
                                            <p className="text-sm text-theme-light mb-4">
                                                Create your own vouchers to act as a medium of exchange within your trusted network.
                                            </p>
                                            <Button 
                                                onClick={props.onNavigateToCreateVoucher}
                                                variant="primary"
                                                className="w-full"
                                            >
                                                Create
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            </section>
                        )
                    )}
                </div>
            </div>

        </div>
    );
}