// src/components/WalletView.tsx
import { useState, useEffect, useCallback } from "react";
import { voucherService } from "../services/voucherService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { VoucherSummary, AppSettings, PublicProfile, VoucherStandardDefinition } from "../types";
import { getMissingProfileHint } from "../utils/signatureHints";
import { useSession } from "../context/SessionContext";
import { PageLayout } from "./ui/PageLayout";
import { VoucherCard } from "./ui/VoucherCard";
import { formatDate } from "../utils/format";
import { 
    Plus, 
    History
} from "lucide-react";

import { useNavigation } from "../context/NavigationContext";
import { useVoucherFilters } from "../hooks/useVoucherFilters";
import { VoucherFilterBar } from "./voucher/VoucherFilterBar";
import { ExportSigningRequestModal } from "./voucher/ExportSigningRequestModal";
import { SignatureRequestBanner } from "./voucher/SignatureRequestBanner";

export function WalletView() {
    const { protectAction } = useSession();
    const { navigate, goBack, appState } = useNavigation();
    
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [parsedStandards, setParsedStandards] = useState<Record<string, VoucherStandardDefinition>>({});

    // Export state
    const [showExportModal, setShowExportModal] = useState(false);
    const [exportId, setExportId] = useState("");

    // Extract initial filters from appState if available
    const initialStatusFilter = appState.view === 'wallet' ? appState.initialStatusFilter : undefined;
    const initialStandardFilter = appState.view === 'wallet' ? appState.initialStandardFilter : undefined;

    const {
        statusFilters,
        standardFilters,
        statusCounts,
        standardCounts,
        filteredVouchers,
        activeFilterCount,
        toggleStatusFilter,
        toggleStandardFilter,
        resetFilters
    } = useVoucherFilters({ 
        vouchers, 
        initialStatusFilter, 
        initialStandardFilter 
    });

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

    const handleRequestSignature = useCallback((voucherId: string, e: React.MouseEvent) => {
        e.stopPropagation();
        setExportId(voucherId);
        setShowExportModal(true);
    }, []);

    const getSignatureHintForVoucher = (voucher: VoucherSummary): string | null => {
        if (!userProfile) return null;
        let standard: VoucherStandardDefinition | undefined = parsedStandards[voucher.voucherStandardUuid];
        if (!standard) {
            standard = Object.values(parsedStandards).find(s => s.immutable.identity.uuid === voucher.voucherStandardUuid);
        }
        if (!standard) return null;
        return getMissingProfileHint(standard, userProfile);
    };

    const allStatusNames = ['active', 'incomplete', 'archived', 'quarantined'];
    const availableStatuses = allStatusNames.filter(s => statusCounts[s] > 0 || statusFilters.includes(s));
    const availableStandards = Array.from(new Set((vouchers || []).map(v => v?.displayStandardName).filter(Boolean))).sort() as string[];

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
                <VoucherFilterBar 
                    statusFilters={statusFilters}
                    standardFilters={standardFilters}
                    statusCounts={statusCounts}
                    standardCounts={standardCounts}
                    activeFilterCount={activeFilterCount}
                    toggleStatusFilter={toggleStatusFilter}
                    toggleStandardFilter={toggleStandardFilter}
                    resetFilters={resetFilters}
                    availableStatuses={availableStatuses}
                    availableStandards={availableStandards}
                />

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
                                onRequestSignature={handleRequestSignature}
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
                                        <SignatureRequestBanner 
                                            onAction={(e) => {
                                                e.stopPropagation();
                                                handleRequestSignature(v.localInstanceId, e);
                                            }}
                                            hint={hint}
                                        />
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

            <ExportSigningRequestModal 
                isOpen={showExportModal}
                voucherId={exportId}
                onClose={() => { setShowExportModal(false); setExportId(""); }}
                settings={settings}
                protectAction={protectAction}
                onSettingsRefresh={setSettings}
            />
        </PageLayout>
    );
}
