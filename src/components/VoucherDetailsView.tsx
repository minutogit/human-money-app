// src/components/VoucherDetailsView.tsx
import { useState, useEffect } from "react";
import { voucherService, SigningRequestConfig } from "../services/voucherService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { useSession } from "../context/SessionContext";
import { AppSettings, VoucherDetails, Contact, TrustStatus, PublicProfile, VoucherStandardDefinition } from "../types";
import { updateLastUsedDirectory } from "../utils/settingsUtils";
import { getMissingProfileHint } from "../utils/signatureHints";
import ContactDialog from "./ContactDialog";
import { PageLayout } from "./ui/PageLayout";
import { InfoRow } from "./ui/InfoRow";
import { 
    Shield, 
    History, 
    FileSignature, 
    AlertCircle, 
    Info, 
    Terminal as Json,
    CheckCircle2,
    Clock,
    ShieldAlert
} from "lucide-react";

// Modular Components
import { CreatorSection } from "./voucher/CreatorSection";
import { PolicySection } from "./voucher/PolicySection";
import { TimelineSection } from "./voucher/TimelineSection";
import { ExportDialog } from "./voucher/ExportDialog";

import { useNavigation } from "../context/NavigationContext";

export function VoucherDetailsView() {
    const { navigate, goBack, appState } = useNavigation();
    
    // Extract voucherId from appState
    const voucherId = appState.view === 'voucher_details' ? appState.voucherId : "";
    
    const [details, setDetails] = useState<VoucherDetails | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const { protectAction } = useSession();
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");
    const [showJson, setShowJson] = useState(false);
    const [showExportModal, setShowExportModal] = useState(false);
    const [isExporting, setIsExporting] = useState(false);
    const [exportError, setExportError] = useState("");
    const [showContactDialog, setShowContactDialog] = useState(false);
    const [pendingContactDID, setPendingContactDID] = useState<string | null>(null);
    const [existingContact, setExistingContact] = useState<Contact | null>(null);
    const [showRemoveSignatureModal, setShowRemoveSignatureModal] = useState<string | null>(null);
    const [isRemovingSignature, setIsRemovingSignature] = useState(false);
    const [proofId, setProofId] = useState<string | null>(null);
    const [isFetchingProofId, setIsFetchingProofId] = useState(false);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [parsedStandards, setParsedStandards] = useState<Record<string, VoucherStandardDefinition>>({});
    const [contacts, setContacts] = useState<Contact[]>([]);

    const voucher = details?.voucher;
    const isQuarantined = details && details.status === 'quarantined';

    useEffect(() => {
        if (voucher?.creator.id) {
            voucherService.checkReputation(voucher.creator.id)
                .then(setTrustStatus)
                .catch(e => logger.error(`Reputation check error: ${e}`));
        }
    }, [voucher?.creator.id]);

    useEffect(() => {
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const [result, currentSettings, profile, standards] = await Promise.all([
                    voucherService.getDetails(voucherId),
                    settingsService.getSettings().catch(() => null),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards().catch(() => [])
                ]);
                setDetails(result);
                setSettings(currentSettings);
                setUserProfile(profile);

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
                const msg = `Failed to fetch voucher details: ${e}`;
                logger.error(msg);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchDetails();
    }, [voucherId]);

    useEffect(() => {
        contactService.getContacts().then(setContacts).catch(e => logger.error(`Failed to fetch contacts: ${e}`));
    }, []);

    useEffect(() => {
        if (isQuarantined && !proofId && !isFetchingProofId) {
            const fetchProofId = async () => {
                setIsFetchingProofId(true);
                try {
                    const id = await voucherService.getProofId(voucherId);
                    setProofId(id);
                } catch (e) {
                    logger.error(`Failed to fetch proof ID for voucher ${voucherId}: ${e}`);
                } finally {
                    setIsFetchingProofId(false);
                }
            };
            fetchProofId();
        }
    }, [isQuarantined, proofId, isFetchingProofId, voucherId]);

    const refreshDetails = async () => {
        try {
            const result = await voucherService.getDetails(voucherId);
            setDetails(result);
        } catch (e) {
            logger.error(`Failed to refresh voucher details: ${e}`);
        }
    };

    function getStatusInfo(details: VoucherDetails) {
        const { status } = details;
        if (status === "active") return { name: 'Active', color: 'text-emerald-600', bgColor: 'bg-emerald-50', icon: CheckCircle2 };
        if (status === "archived") return { name: 'Archived', color: 'text-gray-600', bgColor: 'bg-gray-50', icon: History };
        if (status === "incomplete") return { name: 'Incomplete', color: 'text-amber-600', bgColor: 'bg-amber-50', icon: Clock };
        if (status === "quarantined") return { name: 'Quarantined', color: 'text-rose-600', bgColor: 'bg-rose-50', icon: ShieldAlert };
        if (status === "endorsed") return { name: 'Endorsed', color: 'text-indigo-600', bgColor: 'bg-indigo-50', icon: FileSignature };
        if (status === "expired") return { name: 'Expired', color: 'text-orange-600', bgColor: 'bg-orange-50', icon: Clock };
        return { name: 'Unknown', color: 'text-gray-600', bgColor: 'bg-gray-50', icon: Info };
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';
    const statusInfo = details ? getStatusInfo(details) : null;
    const StatusIcon = statusInfo?.icon;

    if (isLoading) return <div className="text-center p-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">Loading Voucher Details...</div>;
    if (errorMsg || !details || !voucher) return <div className="p-10 text-center text-rose-500 font-bold">{errorMsg || "Voucher not found"}</div>;

    const signatureHint = statusInfo?.name === 'Incomplete' && userProfile
        ? (() => {
            const targetUuid = voucher.voucherStandard.uuid;
            const standard = parsedStandards[targetUuid] || Object.values(parsedStandards).find(s => s.immutable.identity.uuid === targetUuid);
            return standard ? getMissingProfileHint(standard, userProfile) : null;
        })() : null;

    const handleExport = async (config: SigningRequestConfig) => {
        setIsExporting(true);
        setExportError("");
        try {
            const bundleBytes = await voucherService.createSigningRequest(voucherId, config);
            const filePath = await save({
                defaultPath: settings?.lastUsedDirectory 
                    ? `${settings.lastUsedDirectory}/signature-request-${voucherId.slice(0, 8)}.ask`
                    : `signature-request-${voucherId.slice(0, 8)}.ask`,
                filters: [{ name: 'Signature Request (.ask)', extensions: ['ask'] }]
            });

            if (filePath) {
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, new Uint8Array(bundleBytes));
                if (settings) {
                    await updateLastUsedDirectory(filePath, settings, protectAction);
                    settingsService.getSettings().then(setSettings).catch(() => {});
                }
                setShowExportModal(false);
            }
        } catch (e) {
            setExportError(`Export failed: ${e}`);
        } finally {
            setIsExporting(false);
        }
    };

    const handleRemoveSignature = async () => {
        if (!showRemoveSignatureModal) return;
        setIsRemovingSignature(true);
        try {
            await protectAction(async (pwd) => {
                await voucherService.removeSignature(voucherId, showRemoveSignatureModal, pwd || undefined);
            });
            setShowRemoveSignatureModal(null);
            await refreshDetails();
        } catch (e) {
            logger.error(`Failed to remove signature: ${e}`);
            alert(`Failed to remove signature: ${e}`);
        } finally {
            setIsRemovingSignature(false);
        }
    };

    return (
        <PageLayout 
            title="Voucher Details" 
            description={details.displayStandardName}
            onBack={goBack}
            actions={
                <div className="flex items-center gap-2">
                    {statusInfo?.name === 'Incomplete' && (
                        <Button variant="primary" size="sm" onClick={() => setShowExportModal(true)} className="gap-2">
                            <FileSignature size={16} /> Sign
                        </Button>
                    )}
                    <button 
                        onClick={() => setShowJson(!showJson)}
                        className={`p-2 rounded-xl transition-all ${showJson ? 'bg-theme-secondary text-white' : 'bg-white text-theme-light hover:bg-white/80 border border-theme-subtle/50 shadow-sm'}`}
                    >
                        <Json size={18} />
                    </button>
                </div>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6">
                <Card variant={isQuarantined ? "danger" : (details.isTestVoucher ? "warning" : "glass")} className="border-none shadow-premium overflow-hidden">
                    <div className="p-8 flex flex-col md:flex-row items-center justify-between gap-8">
                        <div className="flex items-center gap-6">
                            <div className={`p-5 rounded-3xl shadow-xl transition-transform hover:scale-105 ${statusInfo?.bgColor} ${statusInfo?.color}`}>
                                {StatusIcon && <StatusIcon size={40} />}
                            </div>
                            <div>
                                <div className="flex items-baseline gap-2">
                                    <h1 className="text-5xl font-black text-theme-primary tracking-tighter">{voucher.nominalValue.amount}</h1>
                                    <span className="text-xl font-bold text-theme-light uppercase tracking-widest">{details.displayCurrency}</span>
                                </div>
                                <div className="flex items-center gap-2 mt-2">
                                    <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${statusInfo?.bgColor} ${statusInfo?.color}`}>{statusInfo?.name}</span>
                                    {details.isTestVoucher && <span className="px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest bg-rose-500 text-white shadow-lg shadow-rose-200">Test Voucher</span>}
                                </div>
                            </div>
                        </div>
                        
                        <div className="flex flex-col gap-3 w-full md:w-auto">
                            {isQuarantined && (
                                <Button variant="secondary" size="sm" className="bg-white/20 text-white hover:bg-white/30 border-white/30 backdrop-blur-md" onClick={() => proofId && navigate({ view: 'conflict_details', proofId, previousView: appState })} disabled={!proofId}>
                                    <ShieldAlert size={16} className="mr-2" /> View Proof
                                </Button>
                            )}
                            <div className="bg-white/20 backdrop-blur-md px-4 py-2 rounded-2xl border border-white/30">
                                <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-1">Standard</p>
                                <p className="text-xs font-bold text-theme-secondary truncate max-w-[200px]">{details.displayStandardName}</p>
                            </div>
                        </div>
                    </div>
                    
                    {signatureHint && (
                        <div className="bg-blue-500/10 border-t border-blue-500/20 p-4 px-8 flex items-start gap-4">
                            <Info size={18} className="text-blue-500 mt-1 shrink-0" />
                            <p className="text-sm text-blue-900 font-medium leading-relaxed">
                                <span className="font-black uppercase text-[10px] tracking-widest block mb-1">Signature Hint</span>
                                {signatureHint}
                            </p>
                        </div>
                    )}
                </Card>

                {showJson ? (
                    <Card variant="glass" className="bg-slate-900 border-none shadow-2xl p-0 overflow-hidden">
                        <pre className="text-[11px] leading-relaxed text-indigo-300 font-mono p-6 overflow-x-auto selection:bg-indigo-500 selection:text-white max-h-[600px]">
                            {JSON.stringify(details.voucher, null, 2)}
                        </pre>
                    </Card>
                ) : (
                    <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                        <div className="lg:col-span-2 space-y-6">
                            <CreatorSection 
                                creator={voucher.creator}
                                trustStatus={trustStatus}
                                isContact={contacts.some(c => c.did === voucher.creator.id)}
                                onManageContact={() => {
                                    setPendingContactDID(voucher.creator.id);
                                    setExistingContact(contacts.find(c => c.did === voucher.creator.id) || null);
                                    setShowContactDialog(true);
                                }}
                            />

                            <PolicySection 
                                description={voucher.voucherStandard.template.description}
                                footnote={voucher.voucherStandard.template.footnote}
                            />

                            <TimelineSection 
                                signatures={voucher.signatures}
                                transactions={voucher.transactions}
                                displayCurrency={details.displayCurrency}
                                onRemoveSignature={setShowRemoveSignatureModal}
                            />
                        </div>

                        <div className="space-y-6">
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center gap-2">
                                    <Info size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Details</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <InfoRow label="Voucher ID" isMono icon={Shield}>{voucher.voucherId}</InfoRow>
                                    <InfoRow label="Issue Date" icon={Clock}>{formatDateTime(voucher.creationDate)}</InfoRow>
                                    <InfoRow label="Valid Until" icon={AlertCircle}>{formatDateTime(voucher.validUntil || undefined)}</InfoRow>
                                    <div className="pt-4 border-t border-theme-subtle/30 grid grid-cols-2 gap-4">
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">Divisible</p>
                                            <p className="text-xs font-bold">{voucher.voucherStandard.template.divisible ? "YES" : "NO"}</p>
                                        </div>
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">Collateral</p>
                                            <p className="text-xs font-bold">{voucher.collateral?.amount ? "YES" : "NO"}</p>
                                        </div>
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>
                )}
            </div>

            <ExportDialog 
                isOpen={showExportModal}
                onClose={() => setShowExportModal(false)}
                onExport={handleExport}
                isProcessing={isExporting}
                error={exportError}
            />
            
            <ContactDialog
                isOpen={showContactDialog}
                onClose={() => { setShowContactDialog(false); setPendingContactDID(null); setExistingContact(null); }}
                onSave={async (contact: Contact) => {
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(contact, pwd || undefined);
                        await contactService.getContacts().then(setContacts);
                    });
                }}
                existingContact={existingContact}
                initialProfile={pendingContactDID ? voucher.signatures.find(s => s.signerId === pendingContactDID)?.details : voucher.creator}
                initialDid={pendingContactDID || voucher.creator.id}
            />

            <ConfirmationModal
                isOpen={showRemoveSignatureModal !== null}
                title="Remove Signature"
                description="Are you sure you want to remove this signature? This will be recorded and might affect the voucher's validity."
                confirmText="Remove"
                confirmVariant="danger"
                onConfirm={handleRemoveSignature}
                onCancel={() => setShowRemoveSignatureModal(null)}
                isProcessing={isRemovingSignature}
            />
        </PageLayout>
    );
}