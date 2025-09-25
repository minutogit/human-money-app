// src/components/VoucherDetailsView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { VoucherDetails } from "../types";
import { Button } from "./ui/Button";

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
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");
    const [showJson, setShowJson] = useState(false);

    useEffect(() => {
        logger.info(`VoucherDetailsView: Displayed for voucher ID: ${voucherId}`);
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const result = await invoke<VoucherDetails>("get_voucher_details", { localId: voucherId });
                setDetails(result);
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

    function getDerivedVoucherStatus(voucher: VoucherDetails): { name: string; color: string; tooltip: string } {
        if (voucher.guarantor_signatures.length < voucher.needed_guarantors) {
            return {
                name: 'Incomplete',
                color: 'text-yellow-800 bg-yellow-200',
                tooltip: 'This voucher is waiting for all required guarantor signatures.'
            };
        }
        return {
            name: 'Ready',
            color: 'text-green-800 bg-green-200',
            tooltip: 'This voucher is fully signed by guarantors and ready for use.'
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
                <Button onClick={onBack} variant="secondary" className="mt-6">Back to Dashboard</Button>
            </div>
        </div>
    );
    if (!details) return <div className="p-4 sm:p-6 min-h-screen"><div className="text-center p-8 text-theme-light">No details found for this voucher.</div></div>;

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';
    const creator = details.creator as any;
    const collateral = (details as any).collateral;
    const statusInfo = getDerivedVoucherStatus(details);

    return (
        <div className="max-w-6xl mx-auto space-y-6 p-4 sm:p-6">
            <header className="flex justify-between items-center">
                <Button onClick={onBack} variant="outline" size="sm">&larr; Back to Dashboard</Button>
                <div className="flex items-center gap-4">
                    <Button onClick={() => setShowJson(!showJson)} variant="secondary" size="sm">
                        {showJson ? "Show Formatted View" : "Show Raw JSON"}
                    </Button>
                </div>
            </header>

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
                        <p className="text-theme-secondary mt-2 max-w-2xl">{details.description}</p>
                    </div>

                    <Card title="Creator Details">
                        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-x-6 gap-y-4">
                            <InfoRow label="Name">{creator.first_name} {creator.last_name}</InfoRow>
                            <InfoRow label="Gender">{formatGender(creator.gender)}</InfoRow>
                            <InfoRow label="Address">
                                {creator.address?.street} {creator.address?.house_number}<br/>
                                {creator.address?.zip_code} {creator.address?.city}, {creator.address?.country}
                            </InfoRow>
                            <InfoRow label="Contact">
                                {creator.email && <div>Email: {creator.email}</div>}
                                {creator.phone && <div>Phone: {creator.phone}</div>}
                                {creator.url && <div>Web: {creator.url}</div>}
                            </InfoRow>
                            <InfoRow label="Affiliation">
                                {creator.organization && <div>Org: {creator.organization}</div>}
                                {creator.community && <div>Community: {creator.community}</div>}
                            </InfoRow>
                            <InfoRow label="Coordinates">{creator.coordinates}</InfoRow>
                            <InfoRow label="Service Offer">{creator.service_offer}</InfoRow>
                            <InfoRow label="Needs">{creator.needs}</InfoRow>
                            <InfoRow label="Creator ID" isMono>{creator.id}</InfoRow>
                        </div>
                    </Card>

                    <div className="bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm p-4 flex flex-wrap items-center justify-start gap-x-6 gap-y-2">
                        <span title={statusInfo.tooltip} className={`px-3 py-1 text-sm font-bold rounded-full capitalize ${statusInfo.color}`}>
                            {statusInfo.name}
                        </span>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary" title="Required guarantor signatures.">
                            <span className="text-lg">✍️</span>
                            <span><strong>{details.guarantor_signatures.length} / {details.needed_guarantors}</strong> Guarantors</span>
                        </div>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary" title="Indicates if this voucher is backed by collateral.">
                            <span className="text-lg">🛡️</span>
                            <span>Collateral: <strong>{collateral?.amount ? 'Yes' : 'No'}</strong></span>
                        </div>
                        <div className="flex items-center gap-2 text-sm text-theme-secondary" title="Indicates if this voucher can be split into smaller amounts.">
                            <span className="text-lg">✂️</span>
                            <span>Divisible: <strong>{details.divisible ? 'Yes' : 'No'}</strong></span>
                        </div>
                    </div>

                    <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                        <div className="lg:col-span-2 space-y-6">
                            <Card title="General Information">
                                <div className="space-y-4">
                                    <InfoRow label="Voucher ID" isMono>{details.voucher_id}</InfoRow>
                                    <InfoRow label="Created On">{formatDateTime(details.creation_date)}</InfoRow>
                                    <InfoRow label="Valid Until">{formatDateTime(details.valid_until || undefined)}</InfoRow>
                                    <InfoRow label="Footnote">{details.footnote || 'None'}</InfoRow>
                                </div>
                            </Card>
                        </div>

                        <div className="lg:col-span-1 space-y-6">
                            <Card title="Guarantors">
                                <p className="text-sm text-theme-secondary pb-4">{details.guarantor_requirements_description}</p>
                                {details.guarantor_signatures.length > 0 ? (
                                    <div className="space-y-4">
                                        {details.guarantor_signatures.map(g => (
                                            <div key={g.signature_id} className="bg-theme-subtle/30 rounded p-2 border-t border-theme-subtle pt-3">
                                                <p className="font-semibold">{g.first_name} {g.last_name}</p>
                                                <p className="text-xs font-mono text-theme-light">{g.guarantor_id}</p>
                                                <p className="text-xs text-theme-light mt-1">Signed on: {formatDateTime(g.signature_time)}</p>
                                            </div>
                                        ))}
                                    </div>
                                ) : <p className="text-sm text-theme-light">No guarantor signatures yet.</p>}
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
        </div>
    );
}