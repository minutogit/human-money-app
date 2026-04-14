// src/components/WalletView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { VoucherSummary, VoucherStatus, AppSettings } from "../types";
import { updateLastUsedDirectory } from "../utils/settingsUtils";
import { useSession } from "../context/SessionContext";

interface WalletViewProps {
    onShowDetails: (voucherId: string) => void;
    onBack: () => void;
    onNavigateToCreateVoucher: () => void;
    profileName: string;
}

export function WalletView(props: WalletViewProps) {
    const { protectAction } = useSession();
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [settings, setSettings] = useState<AppSettings | null>(null);

    // Export state for Wallet
    const [showExportModal, setShowExportModal] = useState(false);
    const [exportId, setExportId] = useState("");
    const [recipientId, setRecipientId] = useState("");
    const [encryptToDid, setEncryptToDid] = useState(true);
    const [protectWithPassword, setProtectWithPassword] = useState(false);
    const [exportPassword, setExportPassword] = useState("");
    const [exportPasswordConfirm, setExportPasswordConfirm] = useState("");
    const [showExportPassword, setShowExportPassword] = useState(false);
    const [isExporting, setIsExporting] = useState(false);
    const [exportError, setExportError] = useState("");

    // Filter state
    const [statusFilters, setStatusFilters] = useState<string[]>([]);
    const [standardFilters, setStandardFilters] = useState<string[]>([]);
    const [isFiltersExpanded, setIsFiltersExpanded] = useState(false);

    // Expand/collapse state for vouchers
    const [expandedVoucherIds, setExpandedVoucherIds] = useState<string[]>([]);

    useEffect(() => {
        logger.info("WalletView component displayed");
        async function fetchData() {
            try {
                const [voucherList, currentSettings] = await Promise.all([
                    invoke<VoucherSummary[]>("get_voucher_summaries"),
                    invoke<AppSettings>('get_app_settings').catch(() => null)
                ]);
                setVouchers(voucherList || []);
                setSettings(currentSettings);
            } catch (e) {
                const msg = `Failed to fetch wallet data: ${e}`;
                console.error(msg);
            }
        }
        fetchData();
    }, []);

    // Global listeners for auto-collapse
    useEffect(() => {
        if (!isFiltersExpanded) return;

        const handleGlobalAction = () => {
            setIsFiltersExpanded(false);
        };

        // Capture scroll events from any container
        window.addEventListener('scroll', handleGlobalAction, true);
        
        return () => {
            window.removeEventListener('scroll', handleGlobalAction, true);
        };
    }, [isFiltersExpanded]);

    function getVoucherStatus(status: VoucherStatus): { name: string; color: string; tooltip: string } {
        if (!status) {
            return { name: 'unknown', color: 'text-gray-800 bg-gray-200', tooltip: 'Status unknown' };
        }
        const statusName = (typeof status === 'string' ? status : (Object.keys(status)[0] || 'unknown')).toLowerCase();
        let color = 'text-gray-800 bg-gray-200';
        let tooltip: string;

        switch (statusName) {
            case 'active':
                color = 'text-green-800 bg-green-200';
                tooltip = 'This voucher is active and ready to be used or transferred.';
                break;
            case 'quarantined':
                color = 'text-red-800 bg-red-200';
                tooltip = 'This voucher has been identified as an illegal copy (double-spend) and is no longer usable.';
                break;
            case 'archived':
                color = 'text-indigo-800 bg-indigo-200';
                tooltip = 'This voucher has been fully used or archived. It cannot be used for new transfers.';
                break;
            case 'incomplete':
                color = 'text-gray-800 bg-gray-200';
                tooltip = 'This voucher cannot be used yet because not all conditions have been met (e.g., missing guarantor signatures).';
                break;
            default:
                tooltip = `Status: ${statusName}`;
                break;
        }
        return { name: statusName, color: color, tooltip: tooltip };
    }

    function formatDate(isoString: string): string {
        if (!isoString) return 'N/A';
        return new Date(isoString).toLocaleDateString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric'
        });
    }

    function truncate(text: string, length: number): string {
        if (text.length <= length) return text;
        return text.substring(0, length) + '...';
    }

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toString();
    }

    async function handleExportSigningRequest() {
        if (!exportId) return;
        setIsExporting(true);
        setExportError("");
        try {
            let config;
            if (encryptToDid) {
                if (!recipientId.trim()) {
                    setExportError("Please enter a recipient DID.");
                    setIsExporting(false);
                    return;
                }
                config = { type: "TargetDid", value: recipientId.trim() };
            } else if (protectWithPassword) {
                if (!exportPassword) {
                    setExportError("Please enter a password.");
                    setIsExporting(false);
                    return;
                }
                if (!exportPasswordConfirm) {
                    setExportError("Please confirm your password.");
                    setIsExporting(false);
                    return;
                }
                if (exportPassword !== exportPasswordConfirm) {
                    setExportError("Passwords do not match.");
                    setIsExporting(false);
                    return;
                }
                config = { type: "Password", value: exportPassword };
            } else {
                config = { type: "Cleartext" };
            }

            logger.info(`Creating signing request bundle for voucher ${exportId} from WalletView with config: ${JSON.stringify(config)}`);
            const bundleBytes = await invoke<number[]>("create_signing_request_bundle", {
                localInstanceId: exportId,
                config: config
            });

            const filePath = await save({
                defaultPath: settings?.last_used_directory 
                    ? `${settings.last_used_directory}/signature-request-${exportId.slice(0, 8)}.ask`
                    : `signature-request-${exportId.slice(0, 8)}.ask`,
                filters: [
                    { name: 'Signature Request (.ask)', extensions: ['ask'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (filePath) {
                const uint8Array = new Uint8Array(bundleBytes);
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, uint8Array);
                logger.info(`Signing request bundle saved to ${filePath}`);
                
                // Save directory for next time
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }

                setShowExportModal(false);
                setExportId("");
                setRecipientId("");
                setExportPassword("");
                setExportPasswordConfirm("");
                setExportError("");
            }
        } catch (e) {
            const msg = `Failed to export signing request: ${e}`;
            logger.error(msg);
            setExportError(msg);
        } finally {
            setIsExporting(false);
        }
    }

    // --- Filter Logic ---
    const statusCounts = (vouchers || []).reduce((acc, v) => {
        if (!v) return acc;
        const s = getVoucherStatus(v.status).name;
        acc[s] = (acc[s] || 0) + 1;
        return acc;
    }, {} as Record<string, number>);

    const standardCounts = (vouchers || []).reduce((acc, v) => {
        if (!v || !v.voucher_standard_name) return acc;
        acc[v.voucher_standard_name] = (acc[v.voucher_standard_name] || 0) + 1;
        return acc;
    }, {} as Record<string, number>);

    const allStatusNames = ['active', 'incomplete', 'archived', 'quarantined'];
    // Only show statuses that exist in the wallet OR are currently filtered
    const availableStatuses = allStatusNames.filter(s => statusCounts[s] > 0 || statusFilters.includes(s));
    const availableStandards = Array.from(new Set((vouchers || []).map(v => v?.voucher_standard_name).filter(Boolean))).sort() as string[];

    const toggleStatusFilter = (status: string) => {
        setStatusFilters(prev => 
            prev.includes(status) ? prev.filter(s => s !== status) : [...prev, status]
        );
    };

    const toggleStandardFilter = (standard: string) => {
        setStandardFilters(prev => 
            prev.includes(standard) ? prev.filter(s => s !== standard) : [...prev, standard]
        );
    };

    const filteredVouchers = (vouchers || []).filter(v => {
        if (!v) return false;
        const statusName = getVoucherStatus(v.status).name;
        const matchesStatus = statusFilters.length === 0 || statusFilters.includes(statusName);
        const matchesStandard = standardFilters.length === 0 || standardFilters.includes(v.voucher_standard_name || "");
        return matchesStatus && matchesStandard;
    });

    const activeFilterCount = statusFilters.length + standardFilters.length;

    const toggleAllVouchers = () => {
        if (expandedVoucherIds.length === filteredVouchers.length) {
            setExpandedVoucherIds([]);
        } else {
            setExpandedVoucherIds(filteredVouchers.map(v => v.local_instance_id));
        }
    };

    return (
        <div className="flex flex-col h-full">
            {/* Header with back button */}
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center gap-4 mb-2">
                    <button
                        onClick={props.onBack}
                        className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                        title="Back"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    <h1 className="text-2xl font-bold text-theme-primary">Wallet</h1>
                    <div className="flex-grow"></div>
                    <Button
                        onClick={props.onNavigateToCreateVoucher}
                        variant="primary"
                        className="flex items-center gap-2"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                        </svg>
                        Create
                    </Button>
                </div>
                <p className="text-theme-light ml-14">Manage your vouchers.</p>
            </header>

            {/* Filter Section */}
            <section className="mb-6">
                {/* Collapsible Bar */}
                <button 
                    onClick={(e) => {
                        e.stopPropagation();
                        setIsFiltersExpanded(!isFiltersExpanded);
                    }}
                    className={`w-full flex items-center justify-between px-4 py-2 bg-white border border-theme-subtle rounded-md shadow-sm transition-all hover:border-theme-primary group ${isFiltersExpanded ? 'rounded-b-none border-b-0' : ''}`}
                >
                    <div className="flex items-center gap-2">
                        <svg 
                            xmlns="http://www.w3.org/2000/svg" 
                            className={`h-4 w-4 text-theme-light transition-transform duration-200 ${isFiltersExpanded ? 'rotate-180' : ''}`} 
                            viewBox="0 0 20 20" 
                            fill="currentColor"
                        >
                            <path fillRule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clipRule="evenodd" />
                        </svg>
                        <span className="text-xs font-bold text-theme-secondary uppercase tracking-wider">Filters</span>
                        {activeFilterCount > 0 && !isFiltersExpanded && (
                            <span className="bg-theme-primary text-white text-[10px] px-1.5 py-0.5 rounded-full ml-1 animate-in zoom-in">
                                {activeFilterCount} Active
                            </span>
                        )}
                    </div>
                    {activeFilterCount > 0 && (
                        <span 
                            className="text-[10px] text-theme-accent hover:underline font-medium"
                            onClick={(e) => {
                                e.stopPropagation();
                                setStatusFilters([]);
                                setStandardFilters([]);
                            }}
                        >
                            Clear all
                        </span>
                    )}
                </button>

                {/* Ausklappbarer Bereich */}
                <div 
                    onClick={(e) => e.stopPropagation()} 
                    className={`overflow-hidden transition-all duration-300 ease-in-out bg-white border border-theme-subtle border-t-0 rounded-b-md shadow-sm ${isFiltersExpanded ? 'max-h-[500px] p-4 opacity-100' : 'max-h-0 opacity-0 p-0 border-0'}`}
                >
                    <div className="space-y-6">
                        {/* Status Filter */}
                        <div className="space-y-2">
                            <label className="text-[10px] font-bold text-theme-light uppercase tracking-wider">Voucher Status</label>
                            <div className="flex flex-wrap gap-2">
                                <button
                                    onClick={() => setStatusFilters([])}
                                    className={`px-3 py-1 rounded-full text-[10px] font-bold transition-all border ${
                                        statusFilters.length === 0 
                                        ? 'bg-theme-secondary text-white border-theme-secondary' 
                                        : 'bg-bg-app text-theme-light border-theme-subtle hover:border-theme-secondary hover:text-theme-secondary'
                                    }`}
                                >
                                    All <span className="opacity-60 ml-1">{(vouchers || []).length}</span>
                                </button>
                                {availableStatuses.map(status => {
                                    const isActive = statusFilters.includes(status);
                                    const count = statusCounts[status] || 0;
                                    return (
                                        <button
                                            key={status}
                                            onClick={() => toggleStatusFilter(status)}
                                            className={`px-3 py-1 rounded-full text-[10px] font-bold transition-all border capitalize flex items-center gap-2 ${
                                                isActive 
                                                ? 'bg-theme-primary text-white border-theme-primary' 
                                                : 'bg-white text-theme-light border-theme-subtle hover:border-theme-primary hover:text-theme-primary'
                                            }`}
                                        >
                                            {status}
                                            <span className={`text-[8px] px-1.5 py-0.5 rounded-full ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle text-theme-light'}`}>
                                                {count}
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>

                        {/* Standard Filter */}
                        <div className="space-y-2">
                            <label className="text-[10px] font-bold text-theme-light uppercase tracking-wider">Voucher Standards</label>
                            <div className="flex flex-wrap gap-2">
                                <button
                                    onClick={() => setStandardFilters([])}
                                    className={`px-3 py-1 rounded-full text-[10px] font-bold transition-all border ${
                                        standardFilters.length === 0 
                                        ? 'bg-theme-secondary text-white border-theme-secondary' 
                                        : 'bg-bg-app text-theme-light border-theme-subtle hover:border-theme-secondary hover:text-theme-secondary'
                                    }`}
                                >
                                    All <span className="opacity-60 ml-1">{(vouchers || []).length}</span>
                                </button>
                                {availableStandards.map(standard => {
                                    const isActive = standardFilters.includes(standard);
                                    const count = standardCounts[standard] || 0;
                                    return (
                                        <button
                                            key={standard}
                                            onClick={() => toggleStandardFilter(standard)}
                                            className={`px-3 py-1 rounded-full text-[10px] font-bold transition-all border flex items-center gap-2 ${
                                                isActive 
                                                ? 'bg-theme-accent text-white border-theme-accent' 
                                                : 'bg-white text-theme-light border-theme-subtle hover:border-theme-accent hover:text-theme-accent'
                                            }`}
                                        >
                                            {standard}
                                            <span className={`text-[8px] px-1.5 py-0.5 rounded-full ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle text-theme-light'}`}>
                                                {count}
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    </div>
                </div>
            </section>

            {/* Voucher List */}
            <section>
                <div className="flex items-center justify-between mb-4">
                    <h2 className="text-2xl font-semibold text-theme-secondary">My Vouchers</h2>
                    {filteredVouchers.length > 0 && (
                        <button
                            onClick={toggleAllVouchers}
                            className="text-xs font-bold text-theme-accent hover:underline uppercase tracking-wider bg-theme-accent/5 px-2 py-1 rounded transition-colors"
                        >
                            {expandedVoucherIds.length === filteredVouchers.length ? 'Collapse All' : 'Expand All'}
                        </button>
                    )}
                </div>
                <div className="space-y-4">
                    {filteredVouchers.length > 0 ? filteredVouchers.map(v => {
                        const status = getVoucherStatus(v.status);
                        const isExpanded = expandedVoucherIds.includes(v.local_instance_id);
                        return (
                            <div key={v.local_instance_id} className="relative">
                                {v.non_redeemable_test_voucher && (
                                    <div className="absolute inset-0 flex items-center justify-center pointer-events-none z-20">
                                        <span className="text-[90px] font-bold text-gray-800/20 transform -rotate-12 select-none pointer-events-none">TEST</span>
                                    </div>
                                )}
                                <button
                                    onClick={() => {
                                        if (isExpanded) {
                                            props.onShowDetails(v.local_instance_id);
                                        } else {
                                            setExpandedVoucherIds(prev => [...prev, v.local_instance_id]);
                                        }
                                    }}
                                    className={`w-full text-left bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-sm transition-all duration-200 ease-in-out hover:shadow-md hover:border-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary focus:ring-opacity-50 relative z-10 ${isExpanded ? 'p-4' : 'p-2'}`}
                                >
                                    {/* Collapsed state - minimal info */}
                                    {!isExpanded && (
                                        <div className="flex items-center justify-between">
                                            <div className="flex items-center gap-3">
                                                <div className="text-lg font-bold text-theme-primary">
                                                    {formatAmount(v.current_amount)} {v.unit}
                                                </div>
                                                <div className="text-xs text-theme-light">
                                                    {v.voucher_standard_name}
                                                </div>
                                                <div className="text-xs text-theme-secondary italic border-l border-theme-subtle pl-3">
                                                    by {v.creator_first_name} {v.creator_last_name}
                                                </div>
                                            </div>
                                            <div className="flex items-center gap-4">
                                                <span className="text-[10px] text-theme-light">
                                                    until {formatDate(v.valid_until)}
                                                </span>
                                                <span className={`px-2 py-1 text-[10px] font-bold rounded-full capitalize ${status.color}`}>
                                                    {status.name}
                                                </span>
                                                {v.non_redeemable_test_voucher && (
                                                    <span className="px-2 py-1 text-[10px] font-bold rounded-full text-purple-800 bg-purple-200">Test</span>
                                                )}
                                            </div>
                                        </div>
                                    )}

                                    {/* Expanded state - full info */}
                                    {isExpanded && (
                                        <div className="space-y-3">
                                            {/* Header: Amount and Voucher Name */}
                                            <div className="flex justify-between items-start">
                                                <div>
                                                    <div className="flex items-baseline text-2xl font-bold text-theme-primary">
                                                        <span className="inline-block min-w-[4rem] text-right">{formatAmount(v.current_amount)}</span>
                                                        <span className="ml-2 text-lg font-normal text-theme-light">{v.unit}</span>
                                                    </div>
                                                    <p className="text-xs text-theme-light font-mono">by {v.creator_first_name} {v.creator_last_name}</p>
                                                </div>
                                                <div className="text-right">
                                                    <p className="text-xl font-normal text-theme-light">{v.voucher_standard_name}</p>
                                                </div>
                                            </div>

                                            {/* Body: Description */}
                                            <p className="text-sm text-theme-secondary">{truncate(v.description, 120)}</p>

                                            {/* Footer: Validity and Indicators */}
                                            <div className="border-t border-theme-subtle pt-2">
                                                <div className="flex justify-between items-center text-xs text-theme-light">
                                                    <p>Valid until: <span className="font-semibold">{formatDate(v.valid_until)}</span></p>
                                                    <div className="flex items-center space-x-3 text-sm">
                                                        {v.has_collateral && <span title="Has Collateral">🛡️</span>}
                                                        <span title="Signatures">✍️ {v.guarantor_signatures_count + v.additional_signatures_count}</span>
                                                    </div>
                                                    <div className="flex items-center gap-2">
                                                        <span className={`px-2 py-1 text-xs font-bold rounded-full capitalize ${status.color}`} title={status.tooltip}>
                                                            {status.name}
                                                        </span>
                                                        {v.non_redeemable_test_voucher && (
                                                            <span className="px-3 py-1 text-xs font-bold rounded-full text-purple-800 bg-purple-200" title="Non-redeemable test voucher">Test</span>
                                                        )}
                                                    </div>
                                                </div>

                                                {/* Help text and action for incomplete vouchers */}
                                                {status.name === 'incomplete' && (
                                                    <div className="mt-3 p-3 bg-yellow-50 rounded-md border border-yellow-100 flex flex-col sm:flex-row items-center justify-between gap-3 animate-in fade-in zoom-in duration-300">
                                                        <p className="text-sm text-yellow-800 leading-tight">
                                                            <strong>Missing Signatures:</strong> This voucher needs more signatures to become active.
                                                        </p>
                                                        <Button 
                                                            size="xs" 
                                                            variant="primary" 
                                                            className="bg-theme-accent text-white whitespace-nowrap"
                                                            onClick={(e) => {
                                                                e.stopPropagation();
                                                                setExportId(v.local_instance_id);
                                                                setShowExportModal(true);
                                                            }}
                                                        >
                                                            ✍️ Request Signature
                                                        </Button>
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                    )}
                                </button>
                            </div>
                        );
                    }) : (
                        <div className="text-center text-theme-light py-8 bg-input-readonly rounded-lg border border-theme-subtle">
                            <p>No vouchers found.</p>
                        </div>
                    )}
                </div>
            </section>

            <ConfirmationModal
                isOpen={showExportModal}
                title="Export Signature Request"
                description={
                    <div className="space-y-4">
                        <div className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id="wallet-encryptToDid"
                                checked={encryptToDid}
                                onChange={(e) => setEncryptToDid(e.target.checked)}
                                className="h-4 w-4 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                            />
                            <label htmlFor="wallet-encryptToDid" className="text-sm font-medium text-theme-primary">
                                Encrypt for a specific contact (DID required)
                            </label>
                        </div>

                        {encryptToDid ? (
                            <div>
                                <p className="text-xs text-theme-light mb-1">Enter the signer's DID:</p>
                                <input
                                    type="text"
                                    value={recipientId}
                                    onChange={(e) => setRecipientId(e.target.value)}
                                    placeholder="did:key:z..."
                                    className="w-full px-3 py-2 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary"
                                    autoFocus
                                />
                            </div>
                        ) : (
                            <div className="space-y-4">
                                <div className="flex items-center gap-2">
                                    <input
                                        type="checkbox"
                                        id="wallet-protectWithPassword"
                                        checked={protectWithPassword}
                                        onChange={(e) => setProtectWithPassword(e.target.checked)}
                                        className="h-4 w-4 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <label htmlFor="wallet-protectWithPassword" className="text-sm font-medium text-theme-primary">
                                        Protect with password (Optional)
                                    </label>
                                </div>

                                {protectWithPassword && (
                                    <div className="space-y-3">
                                        <div>
                                            <p className="text-xs text-theme-light mb-1">Enter a password for encryption:</p>
                                            <div className="relative">
                                                <input
                                                    type={showExportPassword ? "text" : "password"}
                                                    value={exportPassword}
                                                    onChange={(e) => {
                                                        setExportPassword(e.target.value);
                                                        setExportError("");
                                                    }}
                                                    placeholder="Password"
                                                    className="w-full px-3 py-2 pr-20 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary"
                                                />
                                                <button
                                                    type="button"
                                                    onClick={() => setShowExportPassword(!showExportPassword)}
                                                    className="absolute right-2 top-1/2 -translate-y-1/2 text-sm text-theme-light hover:text-theme-primary"
                                                >
                                                    {showExportPassword ? "Hide" : "Show"}
                                                </button>
                                            </div>
                                        </div>
                                        <div>
                                            <p className="text-xs text-theme-light mb-1">Confirm password:</p>
                                            <div className="relative">
                                                <input
                                                    type={showExportPassword ? "text" : "password"}
                                                    value={exportPasswordConfirm}
                                                    onChange={(e) => {
                                                        setExportPasswordConfirm(e.target.value);
                                                        setExportError("");
                                                    }}
                                                    placeholder="Confirm Password"
                                                    className="w-full px-3 py-2 pr-20 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary"
                                                />
                                                <button
                                                    type="button"
                                                    onClick={() => setShowExportPassword(!showExportPassword)}
                                                    className="absolute right-2 top-1/2 -translate-y-1/2 text-sm text-theme-light hover:text-theme-primary"
                                                >
                                                    {showExportPassword ? "Hide" : "Show"}
                                                </button>
                                            </div>
                                        </div>
                                    </div>
                                )}
                            </div>
                        )}
                        {exportError && <p className="text-red-500 text-sm mt-2">{exportError}</p>}
                    </div>
                }
                confirmText="Export"
                onConfirm={handleExportSigningRequest}
                onCancel={() => {
                    setShowExportModal(false);
                    setExportId("");
                    setRecipientId("");
                    setExportPassword("");
                    setExportError("");
                }}
                isProcessing={isExporting}
            />
        </div>
    );
}
