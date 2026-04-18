// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { AggregatedBalance, TransactionRecord } from "../types";

interface DashboardProps {
    onNavigateToCreateVoucher: () => void;
    onNavigateToSend: () => void;
    onNavigateToHistory: () => void;
    onNavigateToReceive: () => void;
    onNavigateToConflicts?: () => void;
    profileName: string;
}

export function Dashboard(props: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [recentTransactions, setRecentTransactions] = useState<TransactionRecord[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copied, setCopied] = useState(false);
    const [conflictCount, setConflictCount] = useState(0);

    useEffect(() => {
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, history, conflicts] = await Promise.all([
                    invoke<string>("get_user_id"),
                    invoke<AggregatedBalance[]>("get_total_balance_by_currency"),
                    invoke<TransactionRecord[]>("get_transaction_history").catch(() => []),
                    invoke<any[]>("get_double_spend_conflicts").catch(() => [])
                ]);
                setUserId(id);
                setBalances(balanceList || []);
                // Sort by timestamp, newest first, and take only the first 5
                const sortedHistory = (history || [])
                    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
                    .slice(0, 5);
                setRecentTransactions(sortedHistory);
                // Count unresolved conflicts (neither officially resolved nor locally overridden)
                const unresolvedConflicts = (conflicts || []).filter((c: any) => !c.is_resolved && !c.local_override).length;
                setConflictCount(unresolvedConflicts);
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${e}`;
                console.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
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

    // Group balances by currency unit
    const balancesByUnit = balances.reduce((acc, bal) => {
        const unit = bal.unit;
        if (!acc[unit]) {
            acc[unit] = { total: 0, balances: [] };
        }
        acc[unit].total += parseFloat(bal.total_amount);
        acc[unit].balances.push(bal);
        return acc;
    }, {} as Record<string, { total: number; balances: AggregatedBalance[] }>);

    const uniqueUnits = Object.keys(balancesByUnit);
    const singleCurrency = uniqueUnits.length === 1;
    const primaryUnit = singleCurrency ? uniqueUnits[0] : '';

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
                        <span>ID:</span>
                        <button 
                            onClick={handleCopyUserId}
                            title={copied ? "Copied!" : "Click to copy User ID"}
                            className={`font-mono text-[10px] sm:text-xs transition-all duration-200 border px-2 py-0.5 rounded ${
                                copied 
                                ? 'bg-theme-success/10 text-theme-success border-theme-success/30' 
                                : 'bg-transparent text-theme-light border-theme-subtle hover:border-theme-primary hover:text-theme-primary'
                            }`}
                        >
                            {copied ? "Copied!" : truncatedUserId}
                        </button>
                    </div>
                </div>
            </header>

            {/* Main content area that is scrollable */}
            <div className="flex-grow overflow-y-auto p-4 sm:p-6">
                <div className="mx-auto max-w-4xl">
                    {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

                    {/* Fraud Warning Badge */}
                    {conflictCount > 0 && (
                        <div className="mb-6 bg-red-50 border-l-4 border-red-500 p-4 rounded-md shadow-sm animate-in fade-in slide-in-from-top-2 duration-500">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center">
                                    <div className="flex-shrink-0 text-red-500 text-2xl">🚫</div>
                                    <div className="ml-3">
                                        <p className="text-sm font-bold text-red-800">
                                            Fraud Warning: {conflictCount} Unresolved Conflict{conflictCount > 1 ? 's' : ''}
                                        </p>
                                        <p className="text-xs text-red-700">
                                            Double-spend detected. Review cryptographic proofs in Fraud Reports.
                                        </p>
                                    </div>
                                </div>
                                {props.onNavigateToConflicts && (
                                    <button
                                        onClick={props.onNavigateToConflicts}
                                        className="px-3 py-1.5 text-sm font-semibold text-red-800 bg-red-100 hover:bg-red-200 rounded-md transition-colors"
                                    >
                                        View Reports
                                    </button>
                                )}
                            </div>
                        </div>
                    )}

                    {/* Hero-Section: Centered Total Balance */}
                    <section className="text-center mb-12">
                        <h1 className="text-sm font-semibold text-theme-light uppercase tracking-wider mb-2">Total Balance</h1>
                        {singleCurrency && primaryUnit ? (
                            <div className="flex items-center justify-center">
                                <p className="text-6xl md:text-7xl font-bold text-theme-primary">
                                    {formatAmount(balancesByUnit[primaryUnit].total.toString())}
                                </p>
                                <span className="text-3xl md:text-4xl font-normal text-theme-light ml-3">{primaryUnit}</span>
                            </div>
                        ) : (
                            <div className="flex flex-wrap justify-center gap-6">
                                {uniqueUnits.map(unit => (
                                    <div key={unit} className="flex items-baseline">
                                        <p className="text-5xl md:text-6xl font-bold text-theme-primary">
                                            {formatAmount(balancesByUnit[unit].total.toString())}
                                        </p>
                                        <span className="text-2xl md:text-3xl font-normal text-theme-light ml-2">{unit}</span>
                                    </div>
                                ))}
                            </div>
                        )}
                        {balances.length > 1 && (
                            <div className="mt-4 flex flex-wrap justify-center gap-4">
                                {balances.map((balance) => (
                                    <div key={balance.standard_uuid} className="bg-bg-card-alternate rounded-lg px-4 py-2 border border-theme-subtle">
                                        <p className="text-xs text-theme-light">{balance.standard_name}</p>
                                        <p className="text-lg font-semibold text-theme-secondary">
                                            {formatAmount(balance.total_amount)} {balance.unit}
                                        </p>
                                    </div>
                                ))}
                            </div>
                        )}
                    </section>

                    {/* Quick Actions */}
                    <section className="flex justify-center gap-4 mb-12">
                        <Button 
                            onClick={props.onNavigateToReceive} 
                            className="px-8 py-4 text-lg"
                            variant="primary"
                        >
                            Receive
                        </Button>
                        <Button 
                            onClick={props.onNavigateToSend} 
                            className="px-8 py-4 text-lg"
                            variant="secondary"
                        >
                            Send
                        </Button>
                        <Button 
                            onClick={props.onNavigateToCreateVoucher}
                            className="px-4 py-4 text-lg"
                            variant="secondary"
                            title="Create Voucher"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                            </svg>
                        </Button>
                    </section>

                    {/* Optional History Preview */}
                    {recentTransactions.length > 0 && (
                        <section className="mb-8">
                            <div className="flex items-center justify-between mb-4">
                                <h2 className="text-lg font-semibold text-theme-secondary">Recent Activity</h2>
                                <button 
                                    onClick={props.onNavigateToHistory}
                                    className="text-sm text-theme-accent hover:underline"
                                >
                                    View All
                                </button>
                            </div>
                            <div className="space-y-3">
                                {recentTransactions.map(record => (
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
                                                <p className="text-xs text-theme-light">
                                                    {formatTimestamp(record.timestamp)}
                                                </p>
                                            </div>
                                        </div>
                                        <p className="font-semibold text-theme-secondary">
                                            {formatSummary(record.summableAmounts, record.countableItems)}
                                        </p>
                                    </button>
                                ))}
                            </div>
                        </section>
                    )}
                </div>
            </div>

        </div>
    );
}