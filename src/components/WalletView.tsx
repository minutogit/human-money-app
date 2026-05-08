// src/components/WalletView.tsx
import { useState, useEffect, useCallback } from "react";
import { voucherService, SigningRequestConfig } from "../services/voucherService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { VoucherSummary, AppSettings, PublicProfile, VoucherStandardDefinition } from "../types";
import { updateLastUsedDirectory } from "../utils/settingsUtils";
import { getMissingProfileHint } from "../utils/signatureHints";
import { useSession } from "../context/SessionContext";
import { PageLayout } from "./ui/PageLayout";
import { VoucherCard } from "./ui/VoucherCard";
import { formatDate } from "../utils/format";
import { 
    Plus, 
    Filter, 
    ChevronDown, 
    X, 
    History,
    FileSignature
} from "lucide-react";

import { useNavigation } from "../context/NavigationContext";

export function WalletView() {
    const { protectAction } = useSession();
    const { navigate, goBack, appState } = useNavigation();
    
    // Extract initial filters from appState if available
    const initialStatusFilter = appState.view === 'wallet' ? appState.initialStatusFilter : undefined;
    const initialStandardFilter = appState.view === 'wallet' ? appState.initialStandardFilter : undefined;
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [parsedStandards, setParsedStandards] = useState<Record<string, VoucherStandardDefinition>>({});

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
    const [statusFilters, setStatusFilters] = useState<string[]>(initialStatusFilter ? [initialStatusFilter] : []);
    const [standardFilters, setStandardFilters] = useState<string[]>(initialStandardFilter ? [initialStandardFilter] : []);
    const [isFiltersExpanded, setIsFiltersExpanded] = useState(false);

    // Expand/collapse state for vouchers
    const [expandedVoucherIds, setExpandedVoucherIds] = useState<string[]>([]);

    useEffect(() => {
        logger.info("WalletView component displayed");
        async function fetchData() {
            try {
                const [voucherList, currentSettings, profile, standards] = await Promise.all([
                    voucherService.getSummaries(),
                    settingsService.getSettings().catch(() => null),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards().catch(() => [])
                ]);
                setVouchers(voucherList || []);
                setSettings(currentSettings);
                setUserProfile(profile);

                // Parse standards for hints
                const parsed: Record<string, VoucherStandardDefinition> = {};
                for (const s of standards) {
                    try {
                        parsed[s.id] = await standardsService.parseStandard(s.content);
                    } catch (e) {
                        logger.error(`Failed to parse standard ${s.id}: ${e}`);
                    }
                }
                setParsedStandards(parsed);
            } catch (e) {
                const msg = `Failed to fetch wallet data: ${e}`;
                console.error(msg);
            }
        }
        fetchData();
    }, []);

    const toggleVoucherExpansion = useCallback((voucherId: string) => {
        setExpandedVoucherIds(prev => 
            prev.includes(voucherId) 
                ? prev.filter(id => id !== voucherId) 
                : [...prev, voucherId]
        );
    }, []);

    const getSignatureHintForVoucher = (voucher: VoucherSummary): string | null => {
        if (!userProfile) return null;
        // Try to find by UUID first (new way)
        let standard: VoucherStandardDefinition | undefined = parsedStandards[voucher.voucherStandardUuid];
        
        // Fallback for cases where UUID might not match the ID in the list
        if (!standard) {
            standard = Object.values(parsedStandards).find(s => s.immutable.identity.uuid === voucher.voucherStandardUuid);
        }

        if (!standard) return null;
        return getMissingProfileHint(standard, userProfile);
    };

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
                config = { type: "TargetDid", value: [recipientId.trim(), "TrialDecryption"] };
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

            const bundleBytes = await voucherService.createSigningRequest(exportId, config as SigningRequestConfig);

            const filePath = await save({
                defaultPath: settings?.lastUsedDirectory 
                    ? `${settings.lastUsedDirectory}/signature-request-${exportId.slice(0, 8)}.ask`
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
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        settingsService.getSettings().then(setSettings).catch(() => {});
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

    const statusCounts = (vouchers || []).reduce((acc, v) => {
        if (!v) return acc;
        // Basic mapping for filter counting
        const s = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
        acc[s] = (acc[s] || 0) + 1;
        return acc;
    }, {} as Record<string, number>);

    const standardCounts = (vouchers || []).reduce((acc, v) => {
        if (!v || !v.displayStandardName) return acc;
        acc[v.displayStandardName] = (acc[v.displayStandardName] || 0) + 1;
        return acc;
    }, {} as Record<string, number>);

    const allStatusNames = ['active', 'incomplete', 'archived', 'quarantined'];
    const availableStatuses = allStatusNames.filter(s => statusCounts[s] > 0 || statusFilters.includes(s));
    const availableStandards = Array.from(new Set((vouchers || []).map(v => v?.displayStandardName).filter(Boolean))).sort() as string[];

    const toggleStatusFilter = (status: string) => {
        setStatusFilters(prev => prev.includes(status) ? prev.filter(s => s !== status) : [...prev, status]);
    };

    const toggleStandardFilter = (standard: string) => {
        setStandardFilters(prev => prev.includes(standard) ? prev.filter(s => s !== standard) : [...prev, standard]);
    };

    const filteredVouchers = (vouchers || []).filter(v => {
        if (!v) return false;
        const statusName = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
        const matchesStatus = statusFilters.length === 0 || statusFilters.includes(statusName);
        const matchesStandard = standardFilters.length === 0 || standardFilters.includes(v.displayStandardName || "");
        return matchesStatus && matchesStandard;
    });

    const activeFilterCount = statusFilters.length + standardFilters.length;

    return (
        <PageLayout 
            title="Wallet" 
            description="Manage your digital assets." 
            onBack={goBack}
            actions={
                <Button onClick={() => navigate({ view: 'create_voucher' })} size="sm" className="gap-2 px-6">
                    <Plus size={18} />
                    Create
                </Button>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6">
                {/* Filter Section */}
                <Card variant="glass" className="overflow-hidden border-none shadow-premium">
                    <button 
                        onClick={() => setIsFiltersExpanded(!isFiltersExpanded)}
                        className="w-full flex items-center justify-between px-6 py-4 hover:bg-white/40 transition-colors group"
                    >
                        <div className="flex items-center gap-3">
                            <div className={`p-2 rounded-lg transition-colors ${activeFilterCount > 0 ? 'bg-theme-primary text-white' : 'bg-theme-subtle/50 text-theme-light'}`}>
                                <Filter size={16} />
                            </div>
                            <span className="text-xs font-black text-theme-secondary uppercase tracking-[0.15em]">Filters</span>
                            {activeFilterCount > 0 && (
                                <span className="bg-theme-primary text-white text-[10px] px-2 py-0.5 rounded-full font-bold">
                                    {activeFilterCount}
                                </span>
                            )}
                        </div>
                        <ChevronDown size={18} className={`text-theme-light transition-transform duration-300 ${isFiltersExpanded ? 'rotate-180' : ''}`} />
                    </button>

                    <div className={`transition-all duration-500 ease-in-out ${isFiltersExpanded ? 'max-h-[500px] p-6 pt-0 opacity-100' : 'max-h-0 opacity-0'}`}>
                        <div className="space-y-6 pt-4 border-t border-theme-subtle/30">
                            {/* Status Filter */}
                            <div className="space-y-3">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-2">By Status</label>
                                <div className="flex flex-wrap gap-2">
                                    {availableStatuses.map(status => {
                                        const isActive = statusFilters.includes(status);
                                        // Mock object for simple filter icons
                                        const iconMap: Record<string, React.ElementType> = { active: History, incomplete: History, archived: History, quarantined: History };
                                        const Icon = iconMap[status] || History;
                                        return (
                                            <button
                                                key={status}
                                                onClick={() => toggleStatusFilter(status)}
                                                className={`px-4 py-2 rounded-xl text-xs font-bold transition-all border flex items-center gap-2 ${
                                                    isActive 
                                                    ? 'bg-theme-primary text-white border-theme-primary shadow-md scale-105' 
                                                    : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-primary hover:bg-white shadow-sm'
                                                }`}
                                            >
                                                <Icon size={14} className={isActive ? 'text-white' : 'text-theme-light'} />
                                                <span className="capitalize">{status}</span>
                                                <span className={`text-[10px] px-1.5 py-0.5 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                                    {statusCounts[status] || 0}
                                                </span>
                                            </button>
                                        );
                                    })}
                                </div>
                            </div>

                            {/* Standard Filter */}
                            <div className="space-y-3">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-2">By Standard</label>
                                <div className="flex flex-wrap gap-2">
                                    {availableStandards.map(standard => {
                                        const isActive = standardFilters.includes(standard);
                                        return (
                                            <button
                                                key={standard}
                                                onClick={() => toggleStandardFilter(standard)}
                                                className={`px-4 py-2 rounded-xl text-xs font-bold transition-all border flex items-center gap-2 ${
                                                    isActive 
                                                    ? 'bg-theme-accent text-white border-theme-accent shadow-md scale-105' 
                                                    : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-accent hover:bg-white shadow-sm'
                                                }`}
                                            >
                                                <span className="capitalize">{standard}</span>
                                                <span className={`text-[10px] px-1.5 py-0.5 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                                    {standardCounts[standard] || 0}
                                                </span>
                                            </button>
                                        );
                                    })}
                                </div>
                            </div>
                            
                            {activeFilterCount > 0 && (
                                <div className="flex justify-end pt-2">
                                    <button 
                                        onClick={() => { setStatusFilters([]); setStandardFilters([]); }}
                                        className="text-[10px] font-black text-rose-500 uppercase tracking-widest flex items-center gap-1.5 hover:underline px-2"
                                    >
                                        <X size={12} />
                                        Reset Filters
                                    </button>
                                </div>
                            )}
                        </div>
                    </div>
                </Card>

                {/* Voucher List */}
                <div className="space-y-4">
                    {filteredVouchers.length > 0 ? filteredVouchers.map(v => {
                        const isExpanded = expandedVoucherIds.includes(v.localInstanceId);
                        const hint = getSignatureHintForVoucher(v);
                        const statusName = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
                        
                        return (
                            <VoucherCard 
                                key={v.localInstanceId}
                                voucher={v}
                                isExpanded={isExpanded}
                                onToggleExpand={toggleVoucherExpansion}
                                mode="view"
                            >
                                <div className="space-y-6 pt-6 border-t border-theme-subtle/30">
                                    <div>
                                        <h4 className="text-[10px] font-black text-theme-light uppercase tracking-widest mb-2">Description</h4>
                                        <p className="text-sm text-theme-secondary leading-relaxed bg-theme-subtle/10 p-4 rounded-xl border border-theme-subtle/20">
                                            {v.description || "No description provided."}
                                        </p>
                                    </div>

                                    <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
                                        <div className="bg-white/50 p-3 rounded-xl border border-theme-subtle/30">
                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-widest mb-1">Signatures</p>
                                            <p className="text-sm font-bold text-theme-secondary">✍️ {v.signaturesCount}</p>
                                        </div>
                                        <div className="bg-white/50 p-3 rounded-xl border border-theme-subtle/30">
                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-widest mb-1">Collateral</p>
                                            <p className="text-sm font-bold text-theme-secondary">{v.hasCollateral ? "✅ Yes" : "❌ No"}</p>
                                        </div>
                                        <div className="bg-white/50 p-3 rounded-xl border border-theme-subtle/30">
                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-widest mb-1">Validity</p>
                                            <p className="text-sm font-bold text-theme-secondary">Until {formatDate(v.validUntil)}</p>
                                        </div>
                                    </div>

                                    {statusName === 'incomplete' && (
                                        <div className="space-y-3">
                                            <Card variant="accent" className="p-4 flex flex-col sm:flex-row items-center justify-between gap-4 border-none shadow-premium">
                                                <div className="flex items-center gap-4">
                                                    <div className="p-3 bg-white/20 rounded-2xl text-white">
                                                        <FileSignature size={24} />
                                                    </div>
                                                    <div className="text-white">
                                                        <p className="text-[10px] font-black uppercase tracking-widest opacity-80">Action Required</p>
                                                        <p className="text-base font-bold">Needs more signatures</p>
                                                    </div>
                                                </div>
                                                <Button 
                                                    size="sm" 
                                                    variant="secondary" 
                                                    className="bg-white text-theme-accent hover:bg-white/90 shadow-lg px-6"
                                                    onClick={(e) => {
                                                        e.stopPropagation();
                                                        setExportId(v.localInstanceId);
                                                        setShowExportModal(true);
                                                    }}
                                                >
                                                    Request Signatures
                                                </Button>
                                            </Card>
                                            
                                            {hint && (
                                                <div className="p-3 bg-blue-50/50 rounded-xl border border-blue-100 flex items-start gap-3">
                                                    <span className="text-blue-500 text-sm mt-0.5">💡</span>
                                                    <p className="text-xs text-blue-800 font-medium leading-normal">
                                                        {hint}
                                                    </p>
                                                </div>
                                            )}
                                        </div>
                                    )}
                                    
                                    <div className="flex justify-end pt-4">
                                        <Button variant="outline" size="sm" onClick={() => navigate({ view: "voucher_details", voucherId: v.localInstanceId, previousView: appState })}>
                                            Full Details View
                                        </Button>
                                    </div>
                                </div>
                            </VoucherCard>
                        );
                    }) : (
                        <div className="text-center py-20 bg-white/50 rounded-3xl border-2 border-dashed border-theme-subtle/50">
                            <History size={48} className="mx-auto text-theme-light opacity-20 mb-4" />
                            <p className="text-sm font-black text-theme-placeholder uppercase tracking-[0.2em]">No vouchers found</p>
                        </div>
                    )}
                </div>
            </div>

            <ConfirmationModal
                isOpen={showExportModal}
                title="Export Signature Request"
                description={
                    <div className="space-y-6 pt-2">
                        <div className="flex items-center gap-3 p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20">
                            <input
                                type="checkbox"
                                id="wallet-encryptToDid"
                                checked={encryptToDid}
                                onChange={(e) => setEncryptToDid(e.target.checked)}
                                className="h-5 w-5 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
                            />
                            <label htmlFor="wallet-encryptToDid" className="text-sm font-bold text-theme-secondary">
                                Encrypt for a specific contact
                            </label>
                        </div>

                        {encryptToDid ? (
                            <div className="space-y-2 animate-in slide-in-from-top-2 duration-300">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">Signer's DID</label>
                                <input
                                    type="text"
                                    value={recipientId}
                                    onChange={(e) => setRecipientId(e.target.value)}
                                    placeholder="did:key:z..."
                                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                    autoFocus
                                />
                            </div>
                        ) : (
                            <div className="space-y-4 animate-in slide-in-from-top-2 duration-300">
                                <div className="flex items-center gap-3 p-4 bg-theme-accent/5 rounded-2xl border border-theme-accent/20">
                                    <input
                                        type="checkbox"
                                        id="wallet-protectWithPassword"
                                        checked={protectWithPassword}
                                        onChange={(e) => setProtectWithPassword(e.target.checked)}
                                        className="h-5 w-5 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent transition-all"
                                    />
                                    <label htmlFor="wallet-protectWithPassword" className="text-sm font-bold text-theme-secondary">
                                        Protect with password
                                    </label>
                                </div>

                                {protectWithPassword && (
                                    <div className="space-y-4 animate-in slide-in-from-top-2 duration-300">
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">Password</label>
                                            <div className="relative">
                                                <input
                                                    type={showExportPassword ? "text" : "password"}
                                                    value={exportPassword}
                                                    onChange={(e) => { setExportPassword(e.target.value); setExportError(""); }}
                                                    placeholder="Enter password"
                                                    className="w-full px-4 py-3 pr-20 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                                />
                                                <button
                                                    type="button"
                                                    onClick={() => setShowExportPassword(!showExportPassword)}
                                                    className="absolute right-3 top-1/2 -translate-y-1/2 text-xs font-bold text-theme-light hover:text-theme-primary transition-colors"
                                                >
                                                    {showExportPassword ? "HIDE" : "SHOW"}
                                                </button>
                                            </div>
                                        </div>
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">Confirm Password</label>
                                            <input
                                                type={showExportPassword ? "text" : "password"}
                                                value={exportPasswordConfirm}
                                                onChange={(e) => { setExportPasswordConfirm(e.target.value); setExportError(""); }}
                                                placeholder="Confirm password"
                                                className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                            />
                                        </div>
                                    </div>
                                )}
                            </div>
                        )}
                        {exportError && (
                            <div className="p-3 bg-rose-50 text-rose-600 rounded-xl text-xs font-bold border border-rose-100 animate-in shake duration-500">
                                {exportError}
                            </div>
                        )}
                    </div>
                }
                confirmText="Export Request"
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
        </PageLayout>
    );
}
