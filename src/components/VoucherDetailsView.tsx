// src/components/VoucherDetailsView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { useSession } from "../context/SessionContext";
import { AppSettings, VoucherDetails, Contact } from "../types";
import { updateLastUsedDirectory } from "../utils/settingsUtils";
import ContactDialog from "./ContactDialog";

// Props for the component
interface VoucherDetailsViewProps {
    voucherId: string;
    onBack: () => void;
}

// ===== Helper Components for Structure & Style =====

// A reusable Card component for a consistent look
const Card: React.FC<{ title: string; children: React.ReactNode; className?: string }> = ({ title, children, className = '' }) => (
    <div className={`bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm ${className}`}>
        <h3 className="text-lg font-semibold text-theme-primary px-6 py-4 border-b border-theme-subtle">{title}</h3>
        <div className="p-6">{children}</div>
    </div>
);

// A component for displaying key-value pairs
const InfoRow: React.FC<{ label: string; children: React.ReactNode; isMono?: boolean }> = ({ label, children, isMono = false }) => {
    if (!children) return null;
    return (
        <div>
            <p className="text-sm font-semibold text-theme-light">{label}</p>
            <p className={`${isMono ? 'font-mono text-xs' : 'text-base'} text-theme-secondary break-words`}>{children}</p>
        </div>
    );
};

// ===== Main Component =====

export function VoucherDetailsView({ voucherId, onBack }: VoucherDetailsViewProps) {
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
    const [showExportPassword, setShowExportPassword] = useState(false);
    const [isExporting, setIsExporting] = useState(false);
    const [exportError, setExportError] = useState("");
    const [showContactDialog, setShowContactDialog] = useState(false);
    const [currentUserId, setCurrentUserId] = useState<string | null>(null);
    const [showRemoveSignatureModal, setShowRemoveSignatureModal] = useState<string | null>(null); // contains signature_id
    const [isRemovingSignature, setIsRemovingSignature] = useState(false);

    useEffect(() => {
        logger.info(`VoucherDetailsView: Displayed for voucher ID: ${voucherId}`);
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const [result, currentSettings, userId] = await Promise.all([
                    invoke<VoucherDetails>("get_voucher_details", { localId: voucherId }),
                    invoke<AppSettings>('get_app_settings').catch(() => null),
                    invoke<string>("get_user_id").catch(() => null)
                ]);
                setDetails(result);
                setSettings(currentSettings);
                setCurrentUserId(userId);
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

    const refreshDetails = async () => {
        try {
            const result = await invoke<VoucherDetails>("get_voucher_details", { localId: voucherId });
            setDetails(result);
        } catch (e) {
            logger.error(`Failed to refresh voucher details: ${e}`);
        }
    };

    function getDerivedVoucherStatus(voucher: VoucherDetails): { name: string; color: string; tooltip: string } {
        const signatures = voucher.signatures;
        // Count signatures that are NOT the issuer/creator
        const extraSignatures = signatures.filter(s => s.role !== 'issuer' && s.role !== 'creator').length;

        // Heuristic: If we have fewer than 3 extra signatures, we consider it incomplete for standards like Minuto.
        // Ideally we would read the required count from the standard TOML.
        if (extraSignatures < 3) {
            return {
                name: 'Incomplete',
                color: 'text-yellow-800 bg-yellow-200',
                tooltip: 'This voucher is waiting for required extra signatures.'
            };
        }
        return {
            name: 'Ready',
            color: 'text-green-800 bg-green-200',
            tooltip: 'This voucher is fully signed and ready for use.'
        };
    }

    const formatGender = (genderCode?: string) => {
        switch (genderCode) {
            case '1': return 'Male';
            case '2': return 'Female';
            case '0': return 'Not Known';
            case '9': return 'Not Applicable';
            default: return 'N/A';
        }
    };

    if (isLoading) return <div className="text-center p-8 text-theme-light">Loading Voucher Details...</div>;
    if (errorMsg) return (
        <div className="p-4 sm:p-6 min-h-screen">
            <div className="text-center p-8 text-theme-error bg-red-100 rounded-lg">
                <p className="font-bold">Error Loading Voucher</p>
                <p className="text-sm mt-2 font-mono">{errorMsg}</p>
                <button
                    onClick={onBack}
                    className="mt-6 p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title="Back"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
            </div>
        </div>
    );
    if (!details) return <div className="min-h-screen"><div className="text-center p-8 text-theme-light">No details found for this voucher.</div></div>;

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';
    const creator = details.creator;
    const collateral = details.collateral;
    const statusInfo = getDerivedVoucherStatus(details);

    return (
        <div className="max-w-6xl mx-auto space-y-6">
            <header className="flex items-center gap-4">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title="Back"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-theme-primary">Voucher Details</h1>
                <div className="flex-grow"></div>
                <div className="flex items-center gap-4">
                    {statusInfo.name === 'Incomplete' && (
                        <Button 
                            variant="primary" 
                            size="sm" 
                            onClick={() => setShowExportModal(true)}
                            className="bg-theme-accent text-white shadow-md animate-pulse-subtle"
                        >
                            ✍️ Request Signature
                        </Button>
                    )}
                    <Button 
                        variant="outline" 
                        size="sm" 
                        onClick={() => setShowContactDialog(true)}
                        className="flex items-center gap-1"
                    >
                        👤 Add Creator
                    </Button>
                    <Button onClick={() => setShowJson(!showJson)} variant="secondary" size="sm">
                        {showJson ? "Show Formatted View" : "Show Raw JSON"}
                    </Button>
                </div>
            </header>

            {statusInfo.name === 'Incomplete' && (
                <div className="bg-yellow-50 border-l-4 border-yellow-400 p-4 rounded-md shadow-sm animate-in fade-in slide-in-from-top-2 duration-500">
                    <div className="flex items-center">
                        <div className="flex-shrink-0 text-yellow-400 text-xl">⚠️</div>
                        <div className="ml-3">
                            <p className="text-sm text-yellow-800">
                                <strong>Action Required:</strong> This voucher needs more signatures to become valid and spendable. 
                                Click the <strong>Request Signature</strong> button above to invite signers.
                            </p>
                        </div>
                    </div>
                </div>
            )}

            {showJson ? (
                <Card title="Raw Voucher Data (JSON)">
                    <pre className="text-xs whitespace-pre-wrap break-all font-mono">{JSON.stringify(details, null, 2)}</pre>
                </Card>
            ) : (
                <div className="space-y-6">
                    {details.non_redeemable_test_voucher && (
                        <div className="bg-purple-100 border-l-4 border-purple-500 text-purple-800 p-4 rounded-md" role="alert">
                            <p className="font-bold">Test Voucher</p>
                            <p className="text-sm">This is a non-redeemable test voucher. It has no real value and is intended for testing and demonstration purposes only.</p>
                        </div>
                    )}

                    <div className="bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm p-6">
                        <div className="flex items-baseline gap-3">
                            <p className="text-4xl font-bold text-theme-accent">{details.nominal_value.amount}</p>
                            <p className="text-xl text-theme-light">{details.nominal_value.unit}</p>
                        </div>
                        <h1 className="text-2xl font-bold text-theme-primary mt-1">{details.voucher_standard.name}</h1>
                        <p className="text-theme-secondary mt-2 max-w-2xl">{details.voucher_standard.template.description}</p>
                    </div>

                    <Card title="Creator Details">
                        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-x-6 gap-y-4">
                            <InfoRow label="Name">{creator.first_name ?? ''} {creator.last_name ?? ''}</InfoRow>
                            <InfoRow label="Gender">{formatGender(creator.gender)}</InfoRow>
                            <InfoRow label="Address">
                                {creator.address?.street} {creator.address?.house_number}<br/>
                                {creator.address?.zip_code} {creator.address?.city}, {creator.address?.country}
                            </InfoRow>
                            <InfoRow label="Contact">
                                {creator.email && <div className="truncate">Email: {creator.email}</div>}
                                {creator.phone && <div className="truncate">Phone: {creator.phone}</div>}
                                {creator.url && <div className="truncate">Web: {creator.url}</div>}
                            </InfoRow>
                            <InfoRow label="Affiliation">
                                {creator.organization && <div>Org: {creator.organization}</div>}
                                {creator.community && <div>Community: {creator.community}</div>}
                            </InfoRow>
                            <InfoRow label="Coordinates">{creator.coordinates}</InfoRow>
                            <InfoRow label="Service Offer">{creator.service_offer}</InfoRow>
                            <InfoRow label="Needs">{creator.needs}</InfoRow>
                        </div>
                    </Card>

                    <div className="bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm p-4 flex flex-wrap items-center justify-start gap-x-6 gap-y-2">
                        <span title={statusInfo.tooltip} className={`px-3 py-1 text-sm font-bold rounded-full capitalize ${statusInfo.color}`}>
                            {statusInfo.name}
                        </span>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary">
                            <span><strong>{details.signatures.length}</strong> Signatures</span>
                        </div>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary" title="Indicates if this voucher is backed by collateral.">
                            <span className="text-lg">🛡️</span>
                            <span>Collateral: <strong>{collateral?.amount ? 'Yes' : 'No'}</strong></span>
                        </div>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary" title="Indicates if this voucher can be split into smaller amounts.">
                            <span className="text-lg">✂️</span>
                            <span>Divisible: <strong>{details.voucher_standard.template.divisible ? 'Yes' : 'No'}</strong></span>
                        </div>
                    </div>

                    <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                        <div className="lg:col-span-2 space-y-6">
                            <Card title="General Information">
                                <div className="space-y-4">
                                    <InfoRow label="Voucher ID" isMono>{details.voucher_id}</InfoRow>
                                    <InfoRow label="Creator ID" isMono>{creator.id}</InfoRow>
                                    <InfoRow label="Created On">{formatDateTime(details.creation_date)}</InfoRow>
                                    <InfoRow label="Valid Until">{formatDateTime(details.valid_until || undefined)}</InfoRow>
                                    <InfoRow label="Footnote">{details.voucher_standard.template.footnote || 'None'}</InfoRow>
                                </div>
                            </Card>
                        </div>

                        <div className="lg:col-span-1 space-y-6">
                            <Card title="Signatures">
                                <p className="text-sm text-theme-secondary pb-4">{details.voucher_standard.template.signature_requirements_description}</p>
                                {details.signatures.length > 0 ? (
                                    <div className="space-y-4">
                                        {details.signatures.map(g => {
                                            const isDeletable = 
                                                currentUserId === details.creator.id && 
                                                details.transactions.length === 1 && 
                                                details.transactions[0].t_type === "init" && 
                                                g.role !== "creator" && 
                                                g.role !== "issuer";

                                            return (
                                                <div key={g.signature_id} className="bg-theme-subtle/30 rounded p-2 border-t border-theme-subtle pt-3 group relative">
                                                    <div className="flex justify-between items-start">
                                                        <div className="pr-8">
                                                            <p className="font-semibold text-sm">{g.role.charAt(0).toUpperCase() + g.role.slice(1)}: {g.details?.first_name ?? ''} {g.details?.last_name ?? ''}</p>
                                                            <p className="text-[10px] font-mono text-theme-light break-all">{g.signer_id}</p>
                                                            <p className="text-[10px] text-theme-light mt-1">Signed on: {formatDateTime(g.signature_time)}</p>
                                                        </div>
                                                        {isDeletable && (
                                                            <button
                                                                onClick={() => setShowRemoveSignatureModal(g.signature_id)}
                                                                className="absolute right-2 top-1/2 -translate-y-1/2 p-1.5 text-red-500 hover:text-red-700 hover:bg-red-100/50 rounded-full transition-all duration-200 border border-transparent hover:border-red-200 shadow-sm bg-white/40"
                                                                title="Remove this signature"
                                                            >
                                                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                                </svg>
                                                            </button>
                                                        )}
                                                    </div>
                                                </div>
                                            );
                                        })}
                                    </div>
                                ) : <p className="text-sm text-theme-light">No signatures yet.</p>}
                            </Card>
                        </div>
                    </div>

                    <Card title="Transaction History">
                        <div className="space-y-3 max-h-96 overflow-y-auto pr-2 -mr-2">
                            {details.transactions.slice().reverse().map(t => (
                                <div key={t.t_id} className="border-t border-theme-subtle pt-3">
                                    <div className="flex justify-between items-center mb-1">
                                        <p className="font-semibold capitalize text-theme-primary bg-theme-subtle/30 px-2 py-1 rounded-md text-sm">{t.t_type}</p>
                                        <p className="text-lg font-semibold">{t.amount} <span className="text-base text-theme-light">{details.nominal_value.unit}</span></p>
                                    </div>
                                    <p className="text-xs text-theme-light mb-2">{formatDateTime(t.t_time)}</p>
                                    <div className="text-xs font-mono bg-theme-subtle/30 p-2 rounded">
                                        <p><strong>From:</strong> {t.sender_id}</p>
                                        <p><strong>To: </strong> {t.recipient_id}</p>
                                    </div>
                                </div>
                            ))}
                        </div>
                    </Card>
                </div>
            )}

            <ConfirmationModal
                isOpen={showExportModal}
                title="Export Signature Request"
                description={
                    <div className="space-y-4">
                        <div className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id="encryptToDid"
                                checked={encryptToDid}
                                onChange={(e) => setEncryptToDid(e.target.checked)}
                                className="h-4 w-4 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                            />
                            <label htmlFor="encryptToDid" className="text-sm font-medium text-theme-primary">
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
                                        id="protectWithPassword"
                                        checked={protectWithPassword}
                                        onChange={(e) => setProtectWithPassword(e.target.checked)}
                                        className="h-4 w-4 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <label htmlFor="protectWithPassword" className="text-sm font-medium text-theme-primary">
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
                    setRecipientId("");
                    setExportPassword("");
                    setExportPasswordConfirm("");
                    setExportError("");
                }}
                isProcessing={isExporting}
            />
            
            <ContactDialog
                isOpen={showContactDialog}
                onClose={() => setShowContactDialog(false)}
                onSave={async (contact: Contact) => {
                    await invoke('save_contact', { contact });
                }}
                initialProfile={details.creator}
                initialDid={details.creator.id}
            />

            <ConfirmationModal
                isOpen={showRemoveSignatureModal !== null}
                title="Remove Signature"
                description="Are you sure you want to remove this signature? This will revert the voucher's status to incomplete if the minimum signature requirements are no longer met."
                confirmText="Remove"
                confirmVariant="danger"
                onConfirm={handleRemoveSignature}
                onCancel={() => setShowRemoveSignatureModal(null)}
                isProcessing={isRemovingSignature}
            />
        </div>
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

            logger.info(`Creating signing request bundle for voucher ${voucherId} with config: ${JSON.stringify(config)}`);
            const bundleBytes = await invoke<number[]>("create_signing_request_bundle", {
                localInstanceId: voucherId,
                config: config
            });

            const filePath = await save({
                defaultPath: settings?.last_used_directory 
                    ? `${settings.last_used_directory}/signature-request-${voucherId.slice(0, 8)}.ask`
                    : `signature-request-${voucherId.slice(0, 8)}.ask`,
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
                setRecipientId("");
            }
        } catch (e) {
            const msg = `Failed to export signing request: ${e}`;
            logger.error(msg);
            setExportError(msg);
        } finally {
            setIsExporting(false);
        }
    }
}