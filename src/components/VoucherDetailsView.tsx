// src/components/VoucherDetailsView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { useSession } from "../context/SessionContext";
import { AppSettings, VoucherDetails, Contact, TrustStatus, PublicProfile, VoucherStandardInfo } from "../types";
import { updateLastUsedDirectory } from "../utils/settingsUtils";
import { getMissingProfileHint } from "../utils/signatureHints";
import ContactDialog from "./ContactDialog";
import { PageLayout } from "./ui/PageLayout";
import { 
    User, 
    Shield, 
    History, 
    FileSignature, 
    AlertCircle, 
    Info, 
    Terminal as Json,
    CheckCircle2,
    Clock,
    ShieldAlert,
    MoreVertical,
    Trash2,
    UserPlus,
    ExternalLink,
    MapPin,
    Building2,
    Heart,
    Briefcase
} from "lucide-react";

interface VoucherDetailsViewProps {
    voucherId: string;
    onBack: () => void;
    onViewConflict?: (proofId: string) => void;
}

const InfoRow: React.FC<{ label: string; children: React.ReactNode; isMono?: boolean; icon?: any }> = ({ label, children, isMono = false, icon: Icon }) => {
    if (!children) return null;
    return (
        <div className="flex flex-col gap-1">
            <div className="flex items-center gap-1.5 opacity-60">
                {Icon && <Icon size={12} className="text-theme-light" />}
                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">{label}</p>
            </div>
            <p className={`${isMono ? 'font-mono text-xs' : 'text-sm font-bold'} text-theme-secondary break-words`}>{children}</p>
        </div>
    );
};

export function VoucherDetailsView({ voucherId, onBack, onViewConflict }: VoucherDetailsViewProps) {
    const [details, setDetails] = useState<VoucherDetails | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const { protectAction } = useSession();
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");
    const [showJson, setShowJson] = useState(false);
    const [showExportModal, setShowExportModal] = useState(false);
    const [recipientId, setRecipientId] = useState("");
    const [encryptToDid, setEncryptToDid] = useState(true);
    const [protectWithPassword, setProtectWithPassword] = useState(false);
    const [exportPassword, setExportPassword] = useState("");
    const [exportPasswordConfirm, setExportPasswordConfirm] = useState("");
    const [isExporting, setIsExporting] = useState(false);
    const [exportError, setExportError] = useState("");
    const [showContactDialog, setShowContactDialog] = useState(false);
    const [pendingContactDID, setPendingContactDID] = useState<string | null>(null);
    const [existingContact, setExistingContact] = useState<Contact | null>(null);
    const [showRemoveSignatureModal, setShowRemoveSignatureModal] = useState<string | null>(null);
    const [isRemovingSignature, setIsRemovingSignature] = useState(false);
    const [proofId, setProofId] = useState<string | null>(null);
    const [isFetchingProofId, setIsFetchingProofId] = useState(false);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("Clean");
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [contacts, setContacts] = useState<Contact[]>([]);

    const voucher = details?.voucher;
    const isQuarantined = details && typeof details.status === 'object' && 'Quarantined' in details.status;

    useEffect(() => {
        if (voucher?.creator.id) {
            invoke<TrustStatus>("check_reputation", { offenderId: voucher.creator.id })
                .then(setTrustStatus)
                .catch(e => logger.error(`Reputation check error: ${e}`));
        }
    }, [voucher?.creator.id]);

    useEffect(() => {
        logger.info(`VoucherDetailsView: Displayed for voucher ID: ${voucherId}`);
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const [result, currentSettings, _userId, profile, standards, _sourceId] = await Promise.all([
                    invoke<VoucherDetails>("get_voucher_details", { localId: voucherId }),
                    invoke<AppSettings>('get_app_settings').catch(() => null),
                    invoke<string>("get_user_id").catch(() => null),
                    invoke<PublicProfile>("get_user_profile").catch(() => null),
                    invoke<VoucherStandardInfo[]>("get_voucher_standards").catch(() => []),
                    invoke<string | null>("get_voucher_source_sender", { localId: voucherId }).catch(() => null)
                ]);
                setDetails(result);
                setSettings(currentSettings);
                setUserProfile(profile);
                setVoucherStandards(standards);
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
        async function fetchContacts() {
            try {
                const result = await invoke<Contact[]>('get_contacts');
                setContacts(result);
            } catch (e) {
                logger.error(`Failed to fetch contacts: ${e}`);
            }
        }
        fetchContacts();
    }, []);

    useEffect(() => {
        if (isQuarantined && !proofId && !isFetchingProofId) {
            const fetchProofId = async () => {
                setIsFetchingProofId(true);
                try {
                    const id = await invoke<string | null>("get_proof_id_for_voucher", { localId: voucherId });
                    setProofId(id);
                } catch (e) {
                    logger.error(`Failed to fetch proof ID for voucher ${voucherId}: ${e}`);
                } finally {
                    setIsFetchingProofId(false);
                }
            };
            fetchProofId();
        }
    }, [details, proofId, voucherId]);

    const refreshDetails = async () => {
        try {
            const result = await invoke<VoucherDetails>("get_voucher_details", { localId: voucherId });
            setDetails(result);
        } catch (e) {
            logger.error(`Failed to refresh voucher details: ${e}`);
        }
    };

    function getDerivedVoucherStatus(details: VoucherDetails): { name: string; color: string; bgColor: string; icon: any; tooltip: string } {
        const { status } = details;
        if (status === "Active") return { name: 'Active', color: 'text-emerald-600', bgColor: 'bg-emerald-50', icon: CheckCircle2, tooltip: 'Fully signed and ready.' };
        if (status === "Archived") return { name: 'Archived', color: 'text-gray-600', bgColor: 'bg-gray-50', icon: History, tooltip: 'Fully spent.' };
        
        if (typeof status === 'object') {
            if ('Incomplete' in status) return { name: 'Incomplete', color: 'text-amber-600', bgColor: 'bg-amber-50', icon: Clock, tooltip: 'Awaiting signatures.' };
            if ('Quarantined' in status) return { name: 'Quarantined', color: 'text-rose-600', bgColor: 'bg-rose-50', icon: ShieldAlert, tooltip: 'Double-spend detected.' };
            if ('Endorsed' in status) return { name: 'Endorsed', color: 'text-indigo-600', bgColor: 'bg-indigo-50', icon: FileSignature, tooltip: 'Signed by you.' };
        }
        return { name: 'Unknown', color: 'text-gray-600', bgColor: 'bg-gray-50', icon: Info, tooltip: 'Status unknown.' };
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';
    const statusInfo = details ? getDerivedVoucherStatus(details) : null;
    const StatusIcon = statusInfo?.icon;

    if (isLoading) return <div className="text-center p-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">Loading Voucher Details...</div>;
    if (errorMsg || !details || !voucher) return <div className="p-10 text-center text-rose-500 font-bold">{errorMsg || "Voucher not found"}</div>;

    const creator = voucher.creator;
    const signatureHint = statusInfo?.name === 'Incomplete' && userProfile && voucherStandards.length > 0
        ? (() => {
            const targetUuid = voucher.voucher_standard.uuid;
            const standard = voucherStandards.find(s => s.id === targetUuid || s.content.includes(targetUuid));
            return standard ? getMissingProfileHint(standard.content, userProfile) : null;
        })() : null;

    return (
        <PageLayout 
            title="Voucher Details" 
            description={details.display_standard_name}
            onBack={onBack}
            actions={
                <div className="flex items-center gap-2">
                    {statusInfo?.name === 'Incomplete' && (
                        <Button 
                            variant="primary" 
                            size="sm" 
                            onClick={() => setShowExportModal(true)}
                            className="gap-2"
                        >
                            <FileSignature size={16} />
                            Sign
                        </Button>
                    )}
                    <button 
                        onClick={() => setShowJson(!showJson)}
                        className={`p-2 rounded-xl transition-all ${showJson ? 'bg-theme-secondary text-white' : 'bg-white text-theme-light hover:bg-white/80 border border-theme-subtle/50 shadow-sm'}`}
                        title="Toggle JSON View"
                    >
                        <Json size={18} />
                    </button>
                </div>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6">
                {/* Status Hero Card */}
                <Card variant={isQuarantined ? "danger" : (details.is_test_voucher ? "warning" : "glass")} className="border-none shadow-premium overflow-hidden">
                    <div className="p-8 flex flex-col md:flex-row items-center justify-between gap-8">
                        <div className="flex items-center gap-6">
                            <div className={`p-5 rounded-3xl shadow-xl transition-transform hover:scale-105 ${statusInfo?.bgColor} ${statusInfo?.color}`}>
                                {StatusIcon && <StatusIcon size={40} />}
                            </div>
                            <div>
                                <div className="flex items-baseline gap-2">
                                    <h1 className="text-5xl font-black text-theme-primary tracking-tighter">
                                        {voucher.nominal_value.amount}
                                    </h1>
                                    <span className="text-xl font-bold text-theme-light uppercase tracking-widest">{details.display_currency}</span>
                                </div>
                                <div className="flex items-center gap-2 mt-2">
                                    <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${statusInfo?.bgColor} ${statusInfo?.color}`}>
                                        {statusInfo?.name}
                                    </span>
                                    {details.is_test_voucher && (
                                        <span className="px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest bg-rose-500 text-white shadow-lg shadow-rose-200">
                                            Test Voucher
                                        </span>
                                    )}
                                </div>
                            </div>
                        </div>
                        
                        <div className="flex flex-col gap-3 w-full md:w-auto">
                            {isQuarantined && (
                                <Button 
                                    variant="secondary" 
                                    size="sm" 
                                    className="bg-white/20 text-white hover:bg-white/30 border-white/30 backdrop-blur-md"
                                    onClick={() => proofId && onViewConflict?.(proofId)}
                                    disabled={!proofId}
                                >
                                    <ShieldAlert size={16} className="mr-2" />
                                    View Proof
                                </Button>
                            )}
                            <div className="bg-white/20 backdrop-blur-md px-4 py-2 rounded-2xl border border-white/30">
                                <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-1">Standard</p>
                                <p className="text-xs font-bold text-theme-secondary truncate max-w-[200px]">{details.display_standard_name}</p>
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
                            {JSON.stringify(details, null, 2)}
                        </pre>
                    </Card>
                ) : (
                    <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                        {/* Main Content Area */}
                        <div className="lg:col-span-2 space-y-6">
                            {/* Creator Information */}
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center justify-between w-full">
                                    <div className="flex items-center gap-2">
                                        <User size={18} className="text-theme-primary" />
                                        <span className="font-black text-xs uppercase tracking-widest">Creator</span>
                                    </div>
                                    <Button 
                                        variant="outline" 
                                        size="xs" 
                                        className="h-8 rounded-lg"
                                        onClick={() => {
                                            const contact = contacts.find(c => c.did === creator.id);
                                            setPendingContactDID(creator.id);
                                            setExistingContact(contact || null);
                                            setShowContactDialog(true);
                                        }}
                                    >
                                        {contacts.some(c => c.did === creator.id) ? <MoreVertical size={14} /> : <UserPlus size={14} />}
                                    </Button>
                                </div>
                            }>
                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-8 p-2">
                                    <div className="space-y-6">
                                        <InfoRow label="Legal Name" icon={User}>{creator.first_name} {creator.last_name}</InfoRow>
                                        <InfoRow label="DID / Identifier" icon={Shield} isMono>{creator.id}</InfoRow>
                                        <InfoRow label="Postal Address" icon={MapPin}>
                                            {creator.address?.street} {creator.address?.house_number}<br/>
                                            {creator.address?.zip_code} {creator.address?.city}
                                            {creator.address?.country && <><br/>{creator.address.country}</>}
                                        </InfoRow>
                                        <InfoRow label="Map Coordinates" icon={MapPin}>{creator.coordinates}</InfoRow>
                                    </div>
                                    <div className="space-y-6">
                                        <InfoRow label="Organization / Community" icon={Building2}>{creator.organization || creator.community || "Independent"}</InfoRow>
                                        <InfoRow label="Contact Channels" icon={ExternalLink}>
                                            {creator.email && <div className="text-theme-accent">{creator.email}</div>}
                                            {creator.phone && <div>{creator.phone}</div>}
                                            {creator.url && <a href={creator.url} target="_blank" rel="noopener noreferrer" className="text-theme-primary hover:underline block">{creator.url}</a>}
                                        </InfoRow>
                                        <InfoRow label="Gender Orientation">
                                            {creator.gender === "1" ? "Male" : creator.gender === "2" ? "Female" : creator.gender === "0" ? "Other / Not Declared" : creator.gender === "9" ? "Not Applicable" : "Unknown"}
                                        </InfoRow>
                                        <InfoRow label="Reputation Status">
                                            {typeof trustStatus === 'object' && 'KnownOffender' in trustStatus ? (
                                                <span className="text-rose-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                                                    <ShieldAlert size={14} /> High Risk Account
                                                </span>
                                            ) : (
                                                <span className="text-emerald-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                                                    <CheckCircle2 size={14} /> Clean Reputation
                                                </span>
                                            )}
                                        </InfoRow>
                                    </div>
                                </div>
                            </Card>

                            {/* Community Offerings */}
                            {(creator.service_offer || creator.needs) && (
                                <Card className="border-none shadow-premium" header={
                                    <div className="flex items-center gap-2">
                                        <Heart size={18} className="text-theme-primary" />
                                        <span className="font-black text-xs uppercase tracking-widest">Community Offerings</span>
                                    </div>
                                }>
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-8 p-2">
                                        <InfoRow label="I can help with..." icon={Briefcase}>{creator.service_offer}</InfoRow>
                                        <InfoRow label="I am looking for..." icon={Heart}>{creator.needs}</InfoRow>
                                    </div>
                                </Card>
                            )}

                            {/* Signatures Timeline */}
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center gap-2">
                                    <FileSignature size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Signatures</span>
                                </div>
                            }>
                                <div className="space-y-4">
                                    {voucher.signatures.map((sig) => {
                                        const isCreator = sig.role === "creator" || sig.role === "issuer";
                                        return (
                                            <div key={sig.signature_id} className="relative pl-8 pb-4 last:pb-0">
                                                {/* Timeline Line */}
                                                <div className="absolute left-[11px] top-2 bottom-0 w-0.5 bg-theme-subtle/30"></div>
                                                {/* Timeline Dot */}
                                                <div className={`absolute left-0 top-1.5 w-[24px] h-[24px] rounded-full flex items-center justify-center border-4 border-bg-app z-10 ${isCreator ? 'bg-theme-primary text-white' : 'bg-theme-subtle text-theme-light'}`}>
                                                    <CheckCircle2 size={10} />
                                                </div>
                                                
                                                <div className="bg-white/40 border border-theme-subtle/30 rounded-2xl p-4 flex items-center justify-between group hover:border-theme-primary/30 transition-all shadow-sm">
                                                    <div>
                                                        <div className="flex items-center gap-2 mb-1">
                                                            <span className="text-[10px] font-black uppercase tracking-widest text-theme-light">{sig.role}</span>
                                                            <span className="text-xs font-bold text-theme-secondary">
                                                                {sig.details?.first_name} {sig.details?.last_name}
                                                            </span>
                                                        </div>
                                                        <p className="text-[10px] font-mono text-theme-light opacity-50 truncate max-w-[200px]">{sig.signer_id}</p>
                                                    </div>
                                                    <div className="flex items-center gap-3">
                                                        <span className="text-[10px] font-bold text-theme-light opacity-60">{formatDateTime(sig.signature_time)}</span>
                                                        <button 
                                                            className="p-1.5 rounded-lg opacity-0 group-hover:opacity-100 hover:bg-rose-50 text-rose-500 transition-all"
                                                            onClick={() => setShowRemoveSignatureModal(sig.signature_id)}
                                                        >
                                                            <Trash2 size={14} />
                                                        </button>
                                                    </div>
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            </Card>
                        </div>

                        {/* Sidebar Information */}
                        <div className="space-y-6">
                            {/* Technical Specs */}
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center gap-2">
                                    <Info size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Details</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <InfoRow label="Voucher ID" isMono icon={Shield}>{voucher.voucher_id}</InfoRow>
                                    <InfoRow label="Issue Date" icon={Clock}>{formatDateTime(voucher.creation_date)}</InfoRow>
                                    <InfoRow label="Valid Until" icon={AlertCircle}>{formatDateTime(voucher.valid_until || undefined)}</InfoRow>
                                    <div className="pt-4 border-t border-theme-subtle/30 grid grid-cols-2 gap-4">
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">Divisible</p>
                                            <p className="text-xs font-bold">{voucher.voucher_standard.template.divisible ? "YES" : "NO"}</p>
                                        </div>
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">Collateral</p>
                                            <p className="text-xs font-bold">{voucher.collateral?.amount ? "YES" : "NO"}</p>
                                        </div>
                                    </div>
                                </div>
                            </Card>

                            {/* Activity Log */}
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center gap-2">
                                    <History size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Transaction History</span>
                                </div>
                            }>
                                <div className="space-y-3 max-h-[300px] overflow-y-auto pr-2 custom-scrollbar">
                                    {voucher.transactions.slice().reverse().map((t) => (
                                        <div key={t.t_id} className="p-3 bg-white/40 border border-theme-subtle/30 rounded-xl relative">
                                            <div className="flex justify-between items-start mb-2">
                                                <span className={`text-[8px] font-black uppercase px-2 py-0.5 rounded-full ${t.t_type === 'init' ? 'bg-emerald-500 text-white' : 'bg-theme-secondary text-white'}`}>
                                                    {t.t_type}
                                                </span>
                                                <span className="text-xs font-black text-theme-primary">
                                                    {t.amount} {details.display_currency}
                                                </span>
                                            </div>
                                            <div className="space-y-1">
                                                <p className="text-[9px] text-theme-light flex items-center justify-between">
                                                    <span>By: {t.sender_id ? t.sender_id.slice(0, 12) + "..." : "SYSTEM"}</span>
                                                    <span>{formatDateTime(t.t_time).split(',')[0]}</span>
                                                </p>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </Card>
                        </div>
                    </div>
                )}
            </div>

            {/* Modals & Dialogs */}
            <ConfirmationModal
                isOpen={showExportModal}
                title="Send for Signature"
                description={
                    <div className="space-y-6 pt-2">
                        <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20 space-y-4">
                            <div className="flex items-center gap-3">
                                <input
                                    type="checkbox"
                                    id="encryptToDid"
                                    checked={encryptToDid}
                                    onChange={(e) => setEncryptToDid(e.target.checked)}
                                    className="h-5 w-5 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
                                />
                                <label htmlFor="encryptToDid" className="text-sm font-bold text-theme-secondary cursor-pointer select-none">
                                    Identity-Based Encryption
                                </label>
                            </div>
                            {encryptToDid && (
                                <div className="animate-in slide-in-from-top-2">
                                    <input
                                        type="text"
                                        value={recipientId}
                                        onChange={(e) => setRecipientId(e.target.value)}
                                        placeholder="Enter Signer DID (did:key:...)"
                                        className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-inner-soft text-sm"
                                        autoFocus
                                    />
                                </div>
                            )}
                        </div>
                        
                        {!encryptToDid && (
                            <div className="p-4 bg-theme-accent/5 rounded-2xl border border-theme-accent/20 space-y-4">
                                <div className="flex items-center gap-3">
                                    <input
                                        type="checkbox"
                                        id="protectWithPassword"
                                        checked={protectWithPassword}
                                        onChange={(e) => setProtectWithPassword(e.target.checked)}
                                        className="h-5 w-5 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent transition-all"
                                    />
                                    <label htmlFor="protectWithPassword" className="text-sm font-bold text-theme-secondary cursor-pointer select-none">
                                        Password Protection
                                    </label>
                                </div>
                                {protectWithPassword && (
                                    <div className="space-y-3 animate-in slide-in-from-top-2">
                                        <input
                                            type="password"
                                            value={exportPassword}
                                            onChange={(e) => setExportPassword(e.target.value)}
                                            placeholder="Access Password"
                                            className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                                        />
                                        <input
                                            type="password"
                                            value={exportPasswordConfirm}
                                            onChange={(e) => setExportPasswordConfirm(e.target.value)}
                                            placeholder="Confirm Access Password"
                                            className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                                        />
                                    </div>
                                )}
                            </div>
                        )}
                        {exportError && <p className="text-rose-500 text-xs font-bold px-2">{exportError}</p>}
                    </div>
                }
                confirmText="Create Signature File"
                onConfirm={handleExportSigningRequest}
                onCancel={() => {
                    setShowExportModal(false);
                    setRecipientId("");
                    setExportPassword("");
                    setExportPasswordConfirm("");
                    setExportError("");
                }}
                isProcessing={isExporting}
            />
            
            <ContactDialog
                isOpen={showContactDialog}
                onClose={() => {
                    setShowContactDialog(false);
                    setPendingContactDID(null);
                    setExistingContact(null);
                }}
                onSave={async (contact: Contact) => {
                    await invoke('save_contact', { contact });
                    await invoke<Contact[]>('get_contacts').then(setContacts);
                }}
                existingContact={existingContact}
                initialProfile={pendingContactDID ? voucher.signatures.find(s => s.signer_id === pendingContactDID)?.details : voucher.creator}
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

    async function handleRemoveSignature() {
        if (!showRemoveSignatureModal) return;
        setIsRemovingSignature(true);
        try {
            await protectAction(async (pwd) => {
                await invoke("remove_voucher_signature", {
                    localInstanceId: voucherId,
                    signatureId: showRemoveSignatureModal,
                    password: pwd
                });
            });
            logger.info(`Signature ${showRemoveSignatureModal} removed from voucher ${voucherId}`);
            setShowRemoveSignatureModal(null);
            await refreshDetails();
        } catch (e) {
            const msg = `Failed to remove signature: ${e}`;
            logger.error(msg);
            alert(msg);
        } finally {
            setIsRemovingSignature(false);
        }
    }

    async function handleExportSigningRequest() {
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
                if (exportPassword !== exportPasswordConfirm) {
                    setExportError("Passwords do not match.");
                    setIsExporting(false);
                    return;
                }
                config = { type: "Password", value: exportPassword };
            } else {
                config = { type: "Cleartext" };
            }

            const bundleBytes = await invoke<number[]>("create_signing_request_bundle", {
                localInstanceId: voucherId,
                config: config
            });

            const filePath = await save({
                defaultPath: settings?.last_used_directory 
                    ? `${settings.last_used_directory}/signature-request-${voucherId.slice(0, 8)}.ask`
                    : `signature-request-${voucherId.slice(0, 8)}.ask`,
                filters: [{ name: 'Signature Request (.ask)', extensions: ['ask'] }]
            });

            if (filePath) {
                const uint8Array = new Uint8Array(bundleBytes);
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, uint8Array);
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }
                setShowExportModal(false);
            }
        } catch (e) {
            setExportError(`Export failed: ${e}`);
        } finally {
            setIsExporting(false);
        }
    }
}