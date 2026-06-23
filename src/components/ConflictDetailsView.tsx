import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { integrityService } from "../services/integrityService";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { FullProofDetails } from "../types";
import { translateError } from "../utils/errorHelper";

interface ConflictDetailsViewProps {
    proofId: string;
    onBack: () => void;
}

export function ConflictDetailsView({ proofId, onBack }: ConflictDetailsViewProps) {
    const { t } = useTranslation();
    const [details, setDetails] = useState<FullProofDetails | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isOverriding, setIsOverriding] = useState(false);
    const [errorMsg, setErrorMsg] = useState("");
    
    const [showPasswordModal, setShowPasswordModal] = useState(false);
    const [password, setPassword] = useState("");
    const [localNote, setLocalNote] = useState("");
    const [overrideError, setOverrideError] = useState("");

    useEffect(() => {
        async function fetchProof() {
            setIsLoading(true);
            try {
                logger.info(`Fetching double-spend proof details: ${proofId}`);
                const result = await integrityService.getProofOfDoubleSpend(proofId);
                setDetails(result);
            } catch (e) {
                const msg = t('conflict.fetchDetailsFailed', { error: translateError(e, t) });
                logger.error(`Failed to fetch double-spend proof: ${e}`);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProof();
    }, [proofId, t]);

    const handleLocalOverride = async () => {
        setIsOverriding(true);
        setOverrideError("");
        try {
            await integrityService.setConflictLocalOverride({ 
                proofId, 
                isOverridden: true, 
                resolutionNote: localNote || undefined,
                password: password || undefined 
            });
            // Refresh details
            const updated = await integrityService.getProofOfDoubleSpend(proofId);
            setDetails(updated);
            setShowPasswordModal(false);
            setPassword("");
            setLocalNote("");
        } catch (e) {
            setOverrideError(translateError(e, t));
        } finally {
            setIsOverriding(false);
        }
    };

    if (isLoading) return <div className="text-center p-8 text-theme-light">{t('conflict.loadingDetails')}</div>;
    if (errorMsg) return (
        <div className="p-8 text-center">
            <p className="text-red-500 font-bold mb-4">{t('conflict.errorPrefix', { error: errorMsg })}</p>
            <Button onClick={onBack}>{t('common.back')}</Button>
        </div>
    );
    if (!details) return <div className="text-center p-8 text-theme-light">{t('conflict.proofNotFound')}</div>;

    const { proof, localOverride, conflictRole } = details;
    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    // Sort conflicting transactions by timestamp (earliest first) so the winner is index 0
    const sortedTransactions = [...proof.conflictingTransactions].sort((a, b) => {
        const timeA = a.tTime ? new Date(a.tTime).getTime() : 0;
        const timeB = b.tTime ? new Date(b.tTime).getTime() : 0;
        return timeA - timeB;
    });

    return (
        <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in duration-500 pb-12">
            <header className="flex items-center gap-4">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title={t('common.back')}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-theme-primary flex items-center gap-2">
                    <span>{conflictRole === 'victim' ? '🛑' : '👁️'}</span> 
                    {conflictRole === 'victim' ? t('conflict.criticalConflictTitle') : t('conflict.observationReportTitle')}
                </h1>
            </header>

            {conflictRole === 'victim' ? (
                <div className="bg-red-50 border-l-4 border-red-500 p-4 rounded-r-lg">
                    <div className="flex">
                        <div className="flex-shrink-0">
                            <span className="text-red-500 text-xl">⚠️</span>
                        </div>
                        <div className="ml-3">
                            <h3 className="text-sm font-bold text-red-800">{t('auth.actionRequired')}</h3>
                            <div className="mt-1 text-sm text-red-700 leading-relaxed" dangerouslySetInnerHTML={{ __html: t('conflict.victimDescription') }} />
                        </div>
                    </div>
                </div>
            ) : (
                <div className="bg-blue-50 border-l-4 border-blue-500 p-4 rounded-r-lg">
                    <div className="flex">
                        <div className="flex-shrink-0">
                            <span className="text-blue-500 text-xl">ℹ️</span>
                        </div>
                        <div className="ml-3">
                            <h3 className="text-sm font-bold text-blue-800">{t('conflict.informationOnlyTitle')}</h3>
                            <div className="mt-1 text-sm text-blue-700 leading-relaxed">
                                {t('conflict.witnessDescription')}
                            </div>
                        </div>
                    </div>
                </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-6">
                    <div className="bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-sm p-6">
                        <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">{t('conflict.offenderInfoTitle')}</h2>
                        <div className="space-y-4">
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase tracking-wider">{t('conflict.offenderIdLabel')}</p>
                                <p className="font-mono text-sm break-all text-red-700 bg-red-50 p-2 rounded mt-1">{proof.offenderId}</p>
                            </div>
                            <p className="text-sm text-theme-light leading-relaxed italic">
                                {t('conflict.offenderDescription')}
                            </p>
                        </div>
                    </div>

                    <div className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6">
                        <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">{t('conflict.metadataTitle')}</h2>
                        <div className="space-y-4">
                            <div className="grid grid-cols-2 gap-4">
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">{t('conflict.reportDateLabel')}</p>
                                    <p className="text-sm">{formatDateTime(proof.reportTimestamp)}</p>
                                </div>
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">{t('conflict.trustStatusLabel')}</p>
                                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold uppercase ${localOverride ? 'bg-blue-100 text-blue-800' : proof.isResolved ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}`}>
                                        {proof.isResolved ? t('conflict.statusOfficiallyResolved') : localOverride ? t('conflict.statusLocallySettled') : t('conflict.statusUnresolved')}
                                    </span>
                                </div>
                            </div>
                            {details.localNote && (
                                <div className="space-y-1">
                                    <p className="text-[10px] font-bold text-blue-600 uppercase tracking-tight">{t('conflict.personalResolutionNoteLabel')}</p>
                                    <div className="p-4 bg-blue-50 border border-blue-100 rounded-lg text-sm italic text-blue-900 shadow-sm leading-relaxed">
                                        "{details.localNote}"
                                    </div>
                                </div>
                            )}
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">{t('conflict.affectedVoucherLabel')}</p>
                                <p className="text-sm font-medium">{proof.affectedVoucherName || t('conflict.genericVoucherInstance')}</p>
                            </div>
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">{t('conflict.proofIdLabel')}</p>
                                <p className="font-mono text-[10px] break-all text-theme-light">{proof.proofId}</p>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6">
                    <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">{t('conflict.evidenceTitle')}</h2>
                    <p className="text-xs text-theme-light mb-4">{t('conflict.evidenceDescription')}</p>
                    
                    <div className="space-y-4">
                        {sortedTransactions.map((tx, idx) => (
                            <div key={tx.tId} className={`p-3 rounded border-l-4 ${idx === 0 ? 'border-green-500 bg-green-50/30' : 'border-red-500 bg-red-50/30'}`}>
                                <div className="flex justify-between items-center mb-2">
                                    <span className="text-[10px] font-bold uppercase text-gray-400">
                                        {idx === 0 
                                            ? t('conflict.evidenceTxWinner', { index: idx + 1 }) 
                                            : t('conflict.evidenceTx', { index: idx + 1 })}
                                    </span>
                                    <span className="text-sm font-bold">{tx.amount}</span>
                                </div>
                                <div className="space-y-1 text-[10px] font-mono text-theme-light">
                                    <p><strong>{t('conflict.evidenceId')}</strong> {tx.tId}</p>
                                    <p><strong>{t('conflict.evidenceTime')}</strong> {formatDateTime(tx.tTime)}</p>
                                    <p><strong>{t('conflict.evidenceRecipient')}</strong> {tx.recipientId.slice(0, 20)}...</p>
                                </div>
                            </div>
                        ))}
                    </div>
                    
                    <div className="mt-6 p-3 bg-blue-50 border border-blue-100 rounded text-xs text-blue-800 leading-relaxed">
                        <p><strong>{t('conflict.protocolRulesTitle')}</strong></p>
                        <p className="mt-1" dangerouslySetInnerHTML={{ __html: t('conflict.protocolRulesDescription') }} />
                    </div>
                </div>
            </div>

            {!localOverride && !proof.isResolved && (
                <div className="bg-white rounded-lg border border-theme-subtle p-8 shadow-sm text-center">
                    <h3 className="text-lg font-bold text-theme-primary mb-2">{t('conflict.resolutionTitle')}</h3>
                    <p className="text-sm text-theme-light mb-6 max-w-lg mx-auto">
                        {t('conflict.resolutionDescription')}
                    </p>
                    <div className="max-w-md mx-auto space-y-4">
                        <textarea
                            className="w-full bg-bg-main border border-theme-subtle rounded-lg px-4 py-2 text-sm focus:ring-2 focus:ring-theme-primary outline-none transition-all h-24"
                            placeholder={t('conflict.resolutionPlaceholder')}
                            value={localNote}
                            onChange={(e) => setLocalNote(e.target.value)}
                        />
                        <Button 
                            size="lg" 
                            variant="secondary"
                            className="w-full"
                            onClick={() => setShowPasswordModal(true)}
                        >
                            {t('conflict.markSettledButton')}
                        </Button>
                    </div>
                </div>
            )}

            {proof.resolutions && proof.resolutions.length > 0 && (
                <div className="bg-green-50 rounded-lg border border-green-200 p-6">
                    <h2 className="text-lg font-bold text-green-800 mb-4">{t('conflict.officialResolutionsTitle')}</h2>
                    <div className="space-y-3">
                        {proof.resolutions.map(res => (
                            <div key={res.endorsementId} className="bg-white p-3 rounded border border-green-100 shadow-sm text-sm">
                                <div className="flex justify-between mb-1">
                                    <p className="font-bold">{t('conflict.compensationSignatureLabel')}</p>
                                    <p className="text-xs text-gray-400">{formatDateTime(res.resolutionTimestamp)}</p>
                                </div>
                                <p className="text-gray-600 text-xs">{t('conflict.victimConfirmedSolution', { victim: res.victimId.slice(0, 15) })}</p>
                                {res.notes && <p className="mt-2 text-xs italic text-gray-500">"{res.notes}"</p>}
                            </div>
                        ))}
                    </div>
                </div>
            )}

            <div className="flex justify-center pt-8">
                <Button size="lg" onClick={onBack} variant="outline" className="px-12">{t('conflict.closeReportButton')}</Button>
            </div>

            {/* Password Modal */}
            {showPasswordModal && (
                <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-white rounded-xl shadow-2xl max-w-sm w-full p-6 animate-in zoom-in-95 duration-200">
                        <h3 className="text-xl font-bold text-theme-primary mb-2">{t('conflict.verifySecurityTitle')}</h3>
                        <p className="text-sm text-theme-light mb-6">
                            {t('conflict.verifySecurityDescription')}
                        </p>
                        
                        <input
                            type="password"
                            placeholder={t('conflict.verifySecurityPlaceholder')}
                            className="w-full bg-bg-main border border-theme-subtle rounded-lg px-4 py-3 mb-4 focus:ring-2 focus:ring-theme-primary outline-none transition-all"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            autoFocus
                        />

                        {overrideError && (
                            <div className="p-3 bg-red-50 text-red-600 text-xs rounded-lg mb-4 border border-red-100">
                                {overrideError}
                            </div>
                        )}

                        <div className="flex gap-3">
                            <Button 
                                className="flex-1" 
                                variant="secondary" 
                                onClick={() => { setShowPasswordModal(false); setOverrideError(""); setPassword(""); }}
                            >
                                {t('common.cancel')}
                            </Button>
                            <Button 
                                className="flex-1" 
                                variant="primary" 
                                onClick={handleLocalOverride}
                                disabled={isOverriding}
                            >
                                {isOverriding ? t('common.processing') : t('common.confirm')}
                            </Button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
