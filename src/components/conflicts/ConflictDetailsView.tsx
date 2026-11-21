import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { integrityService } from "../../services/integrityService";
import { contactService } from "../../services/contactService";
import { useSession } from "../../context/SessionContext";
import { logger } from "../../utils/log";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { TrustBadge } from "../ui/TrustBadge";
import { PageLayout } from "../ui/PageLayout";
import { AlertBanner } from "../ui/AlertBanner";
import { Advanced } from "../ui/Advanced";
import { TechId } from "../ui/TechId";
import { PageLoadingState, PageEmptyState, PageErrorState } from "../ui/PageState";
import { FullProofDetails, Contact } from "../../types";
import { translateError, stringifyError } from "../../utils/errorHelper";
import {
    ShieldAlert,
    CheckCircle2,
    Download,
    Upload,
    FolderOpen,
    FileSignature,
    Shield,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

const MAX_ENDORSEMENT_NOTES_BYTES = 512;

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
    const [localNote, setLocalNote] = useState("");

    const [showEndorseForm, setShowEndorseForm] = useState(false);
    const [endorseNote, setEndorseNote] = useState("");
    const [endorseAlsoOverride, setEndorseAlsoOverride] = useState(true);
    const [isEndorsing, setIsEndorsing] = useState(false);
    const [justSignedBanner, setJustSignedBanner] = useState(false);

    const [isImportingUpdated, setIsImportingUpdated] = useState(false);
    const [importFeedback, setImportFeedback] = useState<{
        variant: "success" | "info";
        message: string;
    } | null>(null);

    const { protectAction } = useSession();
    const [matchedContact, setMatchedContact] = useState<Contact | null>(null);
    const [trustRevoked, setTrustRevoked] = useState(false);
    const [isRevoking, setIsRevoking] = useState(false);

    useEffect(() => {
        let active = true;
        if (!details?.proof?.offenderId) return;
        const offenderId = details.proof.offenderId;
        const offenderBare = offenderId.split('@').pop();

        contactService.getContacts()
            .then(contacts => {
                if (!active) return;
                const found = contacts.find(c => {
                    if (c.trustLevel == null || c.trustLevel < 0) return false;
                    const contactBare = c.did.split('@').pop();
                    return c.did === offenderId || (offenderBare && contactBare === offenderBare);
                });
                setMatchedContact(found || null);
            })
            .catch(e => {
                if (active) logger.error(`Failed to load contacts for offender check: ${stringifyError(e)}`);
            });
        return () => {
            active = false;
        };
    }, [details?.proof?.offenderId]);

    const handleRevokeContactTrust = async () => {
        if (!matchedContact) return;
        setIsRevoking(true);
        setErrorMsg("");
        try {
            await protectAction(async (pwd) => {
                await contactService.saveContact({
                    ...matchedContact,
                    trustLevel: null,
                    desiredHops: null,
                }, pwd || undefined);
            });
            setTrustRevoked(true);
        } catch (e) {
            logger.error(`Failed to revoke contact trust: ${stringifyError(e)}`);
            setErrorMsg(translateError(e, t));
        } finally {
            setIsRevoking(false);
        }
    };

    useEffect(() => {
        let active = true;
        async function fetchProof() {
            setIsLoading(true);
            try {
                logger.info(`Fetching double-spend proof details: ${proofId}`);
                const result = await integrityService.getProofOfDoubleSpend(proofId);
                if (active) setDetails(result);
            } catch (e) {
                if (active) {
                    const msg = t('conflict.fetchDetailsFailed', { error: translateError(e, t) });
                    logger.error(`Failed to fetch double-spend proof: ${stringifyError(e)}`);
                    setErrorMsg(msg);
                }
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchProof();
        return () => {
            active = false;
        };
    }, [proofId, t]);

    const handleLocalOverride = async (enable: boolean) => {
        setIsOverriding(true);
        setErrorMsg("");
        try {
            await protectAction(
                async (pwd) => {
                    await integrityService.setConflictLocalOverride({
                        proofId,
                        isOverridden: enable,
                        resolutionNote: enable ? localNote.trim() || undefined : undefined,
                        password: pwd || undefined,
                    });
                    const updated = await integrityService.getProofOfDoubleSpend(proofId);
                    setDetails(updated);
                    if (enable) {
                        setLocalNote("");
                    }
                },
                { requireFullAccess: true }
            );
        } catch (e) {
            setErrorMsg(translateError(e, t));
        } finally {
            setIsOverriding(false);
        }
    };

    const handleEndorseResolution = async () => {
        if (!details) return;
        const noteBytes = new TextEncoder().encode(endorseNote).length;
        if (noteBytes > MAX_ENDORSEMENT_NOTES_BYTES) return;
        setIsEndorsing(true);
        setErrorMsg("");
        try {
            const updated = await protectAction(
                async (pwd) => {
                    return await integrityService.endorseConflictResolution({
                        proofId,
                        notes: endorseNote.trim() || undefined,
                        alsoSetLocalOverride: !details.localOverride && endorseAlsoOverride,
                        password: pwd || undefined,
                    });
                },
                { requireFullAccess: true }
            );
            if (updated) {
                setDetails(updated);
                setShowEndorseForm(false);
                setEndorseNote("");
                setJustSignedBanner(true);
            }
        } catch (e) {
            logger.error(`Failed to endorse conflict resolution: ${stringifyError(e)}`);
            setErrorMsg(translateError(e, t));
        } finally {
            setIsEndorsing(false);
        }
    };

    const [isExporting, setIsExporting] = useState(false);
    const [exportedProofPath, setExportedProofPath] = useState<string | null>(null);

    const handleExportProof = async () => {
        setIsExporting(true);
        setErrorMsg("");
        try {
            const bytes = await integrityService.exportDoubleSpendProof(proofId);
            const filePath = await save({
                defaultPath: `double-spend-${proofId.slice(0, 8)}.hmcproof`,
                filters: [
                    { name: 'Double Spend Proof (.hmcproof)', extensions: ['hmcproof'] },
                    { name: t('voucher.export.allFiles'), extensions: ['*'] }
                ]
            });
            if (filePath) {
                await writeFile(filePath, new Uint8Array(bytes));
                setExportedProofPath(filePath);
            }
        } catch (e) {
            logger.error(`Failed to export double-spend proof: ${stringifyError(e)}`);
            setErrorMsg(t('conflict.exportProofFailed', { error: translateError(e, t) }));
        } finally {
            setIsExporting(false);
        }
    };

    const handleImportUpdatedProof = async () => {
        setIsImportingUpdated(true);
        setErrorMsg("");
        setImportFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                filters: [
                    { name: t('conflict.filterName'), extensions: ['hmcproof'] },
                    { name: t('voucher.export.allFiles'), extensions: ['*'] }
                ]
            });
            if (!selected || typeof selected !== 'string') return;
            const fileBytes = await readFile(selected);
            const res = await protectAction(async (pwd) => {
                return await integrityService.importDoubleSpendProof(
                    Array.from(fileBytes),
                    pwd || undefined
                );
            });
            if (res) {
                const updated = await integrityService.getProofOfDoubleSpend(proofId);
                setDetails(updated);
                if (res.addedEndorsements > 0) {
                    setImportFeedback({
                        variant: "success",
                        message: t('conflict.endorsementsMergedCount', { count: res.addedEndorsements }),
                    });
                } else {
                    setImportFeedback({
                        variant: "info",
                        message: t('conflict.endorsementsAlreadyUpToDate'),
                    });
                }
            }
        } catch (e) {
            logger.error(`Failed to import updated double-spend proof: ${stringifyError(e)}`);
            setErrorMsg(t('conflict.importProofFailed', { error: translateError(e, t) }));
        } finally {
            setIsImportingUpdated(false);
        }
    };

    if (isLoading) {
        return (
            <PageLayout title={t('conflict.title')} onBack={onBack} maxWidth="4xl">
                <PageLoadingState message={t('conflict.loadingDetails')} />
            </PageLayout>
        );
    }
    if (errorMsg && !details) {
        return (
            <PageLayout title={t('conflict.title')} onBack={onBack} maxWidth="4xl">
                <PageErrorState message={t('conflict.errorPrefix', { error: errorMsg })} />
            </PageLayout>
        );
    }
    if (!details) {
        return (
            <PageLayout title={t('conflict.title')} onBack={onBack} maxWidth="4xl">
                <PageEmptyState title={t('conflict.proofNotFound')} />
            </PageLayout>
        );
    }

    const { proof, localOverride, conflictRole, canEndorse } = details;
    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    // Sort conflicting transactions by timestamp (earliest first) so the winner is index 0
    const sortedTransactions = [...proof.conflictingTransactions].sort((a, b) => {
        const timeA = a.tTime ? new Date(a.tTime).getTime() : 0;
        const timeB = b.tTime ? new Date(b.tTime).getTime() : 0;
        return timeA - timeB;
    });

    const resolutions = proof.resolutions ?? [];
    const trustedEndorsementCount = resolutions.filter(
        (r) =>
            !r.isOwnIdentity &&
            r.wotTrust &&
            !r.wotTrust.blockedByOffender &&
            r.wotTrust.trustPercent > 0
    ).length;
    const endorseNoteBytes = new TextEncoder().encode(endorseNote).length;
    const endorseNoteTooLong = endorseNoteBytes > MAX_ENDORSEMENT_NOTES_BYTES;

    return (
        <PageLayout
            title={
                <span className="inline-flex items-center gap-2">
                    <span>{conflictRole === 'victim' ? '🛑' : '👁️'}</span>
                    <span>{conflictRole === 'victim' ? t('conflict.criticalConflictTitle') : t('conflict.observationReportTitle')}</span>
                </span>
            }
            onBack={onBack}
            maxWidth="4xl"
            actions={
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={handleImportUpdatedProof}
                        disabled={isImportingUpdated}
                        className="gap-2 text-xs font-bold"
                    >
                        <Upload size={16} />
                        {isImportingUpdated ? t('common.loading') : t('conflict.importUpdatedProofBtn')}
                    </Button>
                    <Button
                        variant="outline"
                        size="sm"
                        onClick={handleExportProof}
                        disabled={isExporting}
                        className="gap-2 text-xs font-bold"
                    >
                        <Download size={16} />
                        {isExporting ? t('common.loading') : t('conflict.exportProofBtn')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-6 animate-in fade-in duration-500 pb-12">
                {errorMsg && (
                    <AlertBanner variant="error" onDismiss={() => setErrorMsg('')}>
                        {errorMsg}
                    </AlertBanner>
                )}

                {importFeedback && (
                    <AlertBanner
                        variant={importFeedback.variant}
                        onDismiss={() => setImportFeedback(null)}
                    >
                        {importFeedback.message}
                    </AlertBanner>
                )}

                {justSignedBanner && (
                    <AlertBanner
                        variant="success"
                        title={t('conflict.signResolutionSuccessTitle')}
                        onDismiss={() => setJustSignedBanner(false)}
                        action={
                            <Button
                                size="sm"
                                variant="secondary"
                                onClick={handleExportProof}
                                disabled={isExporting}
                                className="gap-1.5 text-xs font-bold"
                            >
                                <Download size={14} />
                                {t('conflict.shareUpdatedProofBtn')}
                            </Button>
                        }
                    >
                        {t('conflict.signResolutionSuccessDesc')}
                    </AlertBanner>
                )}

                {exportedProofPath && (
                    <AlertBanner
                        variant="success"
                        title={t('conflict.proofExportedTitle')}
                        action={
                            <Button
                                size="sm"
                                variant="secondary"
                                onClick={() => revealItemInDir(exportedProofPath).catch(() => {})}
                                className="gap-1.5 text-xs font-bold"
                            >
                                <FolderOpen size={14} />
                                {t('transfer.success.revealInFolder')}
                            </Button>
                        }
                    >
                        <span className="font-mono text-xs">{exportedProofPath}</span>
                    </AlertBanner>
                )}

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

            <div
                data-testid="conflict-three-questions"
                className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6 grid grid-cols-1 md:grid-cols-3 gap-4"
            >
                <div className="space-y-1">
                    <h3 className="text-xs font-bold uppercase tracking-wider text-theme-primary">
                        {t('conflict.qWhatHappenedTitle')}
                    </h3>
                    <p className="text-sm text-theme-secondary leading-relaxed">
                        {conflictRole === 'victim' ? t('conflict.qWhatHappenedVictim') : t('conflict.qWhatHappenedWitness')}
                    </p>
                </div>
                <div className="space-y-1">
                    <h3 className="text-xs font-bold uppercase tracking-wider text-theme-primary">
                        {t('conflict.qVoucherValidTitle')}
                    </h3>
                    <p className="text-sm text-theme-secondary leading-relaxed">
                        {conflictRole === 'victim' ? t('conflict.qVoucherValidVictim') : t('conflict.qVoucherValidWitness')}
                    </p>
                </div>
                <div className="space-y-1">
                    <h3 className="text-xs font-bold uppercase tracking-wider text-theme-primary">
                        {t('conflict.qWhatToDoTitle')}
                    </h3>
                    <p className="text-sm text-theme-secondary leading-relaxed">
                        {conflictRole === 'victim' ? t('conflict.qWhatToDoVictim') : t('conflict.qWhatToDoWitness')}
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-6">
                    <div className="bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-sm p-6">
                        <div className="flex flex-wrap items-center justify-between gap-2 mb-4 border-b border-theme-subtle pb-2">
                            <h2 className="text-lg font-bold text-theme-primary">{t('conflict.offenderInfoTitle')}</h2>
                            <Advanced>
                                {proof.isCryptographicallyAttributed ? (
                                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-[10px] font-black uppercase tracking-wider bg-emerald-100 text-emerald-800 border border-emerald-200">
                                        {t('conflict.expert.sstProvenIdentity')}
                                    </span>
                                ) : (
                                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-[10px] font-black uppercase tracking-wider bg-amber-100 text-amber-800 border border-amber-200">
                                        {t('conflict.expert.ephemeralUnverifiedHint')}
                                    </span>
                                )}
                            </Advanced>
                        </div>
                        <div className="space-y-4">
                            <div className="min-w-0">
                                <p className="text-xs font-semibold text-theme-light uppercase tracking-wider mb-1">{t('conflict.offenderIdLabel')}</p>
                                <TechId value={proof.offenderId} kind="did" />
                            </div>
                            {!proof.isCryptographicallyAttributed && proof.suspectedIdentity && (
                                <div className="min-w-0" data-testid="conflict-suspected-identity">
                                    <p className="text-xs font-semibold text-theme-light uppercase tracking-wider mb-1">
                                        {t('conflict.suspectedIdentityLabel')}
                                    </p>
                                    <TechId value={proof.suspectedIdentity} kind="did" />
                                </div>
                            )}
                            <Advanced>
                                {proof.isCryptographicallyAttributed ? (
                                    proof.hasVictimWitness && (
                                        <p className="text-xs font-medium text-emerald-700 bg-emerald-50/60 p-2.5 rounded-lg border border-emerald-100">
                                            {t((proof.victimWitnessCount ?? 0) > 1 ? 'conflict.witnessDisclosedPlural' : 'conflict.witnessDisclosed', { count: proof.victimWitnessCount ?? 1 })}
                                        </p>
                                    )
                                ) : (
                                    <p className="text-xs font-medium text-amber-700 bg-amber-50/60 p-2.5 rounded-lg border border-amber-100">
                                        {t('conflict.expert.unverifiedHintWarning')}
                                    </p>
                                )}
                            </Advanced>
                            <p className="text-sm text-theme-light leading-relaxed italic">
                                {t('conflict.offenderDescription')}
                            </p>
                            {matchedContact && (
                                <div className="mt-4 p-4 bg-rose-50 border-2 border-rose-200 rounded-2xl space-y-3">
                                    <div className="flex items-center gap-2 text-rose-900 font-bold text-sm">
                                        <ShieldAlert size={18} className="text-rose-600 shrink-0" />
                                        <span>{t('conflict.revokeContactTrustTitle')}</span>
                                    </div>
                                    <p className="text-xs text-rose-800 leading-relaxed font-medium">
                                        {t('conflict.revokeContactTrustDesc')}
                                    </p>
                                    {!trustRevoked ? (
                                        <Button
                                            variant="danger"
                                            size="sm"
                                            onClick={handleRevokeContactTrust}
                                            disabled={isRevoking}
                                            className="w-full font-bold"
                                        >
                                            {isRevoking ? t('common.processing') : t('conflict.revokeContactTrustBtn')}
                                        </Button>
                                    ) : (
                                        <div className="p-2.5 bg-emerald-100 text-emerald-800 border border-emerald-200 rounded-xl text-xs font-bold flex items-center gap-2">
                                            <CheckCircle2 size={16} className="text-emerald-600 shrink-0" />
                                            <span>{t('conflict.revokeContactTrustDone')}</span>
                                        </div>
                                    )}
                                </div>
                            )}
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
                                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold uppercase ${localOverride ? 'bg-blue-100 text-blue-800' : 'bg-red-100 text-red-800'}`}>
                                        {localOverride ? t('conflict.statusLocallySettled') : t('conflict.statusUnresolved')}
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
                            <Advanced>
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase mb-1">{t('conflict.proofIdLabel')}</p>
                                    <TechId value={proof.proofId} kind="proof" />
                                </div>
                            </Advanced>
                        </div>
                    </div>
                </div>

                <Advanced>
                    <div data-testid="conflict-evidence-section" className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6">
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
                                        <p className="break-all"><strong>{t('conflict.evidenceId')}</strong> {tx.tId}</p>
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
                </Advanced>
            </div>

            {/* Subjective WoT Network Resolution Notes */}
            <div
                data-testid="conflict-endorsements-section"
                className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6 space-y-4"
            >
                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-theme-subtle pb-3">
                    <div>
                        <h2 className="text-lg font-bold text-theme-primary">
                            {t('conflict.resolutionNotesTitle', { count: resolutions.length })}
                        </h2>
                        <p className="text-xs text-theme-light mt-1">
                            {t('conflict.resolutionNotesDisclaimer')}
                        </p>
                    </div>
                    {canEndorse && !showEndorseForm && (
                        <Button
                            size="sm"
                            variant="primary"
                            onClick={() => {
                                setEndorseAlsoOverride(!localOverride);
                                setShowEndorseForm(true);
                            }}
                            className="gap-1.5 font-bold"
                        >
                            <FileSignature size={15} />
                            {t('conflict.signResolutionBtn')}
                        </Button>
                    )}
                </div>

                {canEndorse && showEndorseForm && (
                    <div
                        data-testid="conflict-endorse-form"
                        className="p-4 rounded-xl border border-theme-subtle bg-bg-card-alternate space-y-3"
                    >
                        <p className="text-xs text-theme-secondary font-medium">
                            {t('conflict.signResolutionImmutableHint')}
                        </p>
                        <div>
                            <Textarea
                                rows={3}
                                placeholder={t('conflict.signResolutionNotePlaceholder')}
                                value={endorseNote}
                                onChange={(e) => setEndorseNote(e.target.value)}
                            />
                            <div
                                className={`text-[11px] mt-1 text-right font-mono ${
                                    endorseNoteTooLong ? 'text-rose-600 font-bold' : 'text-theme-light'
                                }`}
                            >
                                {t('conflict.resolutionNotesBytesCounter', {
                                    bytes: endorseNoteBytes,
                                    max: MAX_ENDORSEMENT_NOTES_BYTES,
                                })}
                            </div>
                        </div>
                        {!localOverride && (
                            <label className="flex items-center gap-2 text-xs text-theme-primary font-medium cursor-pointer select-none">
                                <input
                                    type="checkbox"
                                    checked={endorseAlsoOverride}
                                    onChange={(e) => setEndorseAlsoOverride(e.target.checked)}
                                    className="rounded border-theme-subtle"
                                />
                                <span>{t('conflict.signAlsoLocalOverrideLabel')}</span>
                            </label>
                        )}
                        <div className="flex flex-wrap gap-2 justify-end">
                            <Button
                                size="sm"
                                variant="secondary"
                                onClick={() => {
                                    setShowEndorseForm(false);
                                    setEndorseNote("");
                                }}
                                disabled={isEndorsing}
                            >
                                {t('common.cancel')}
                            </Button>
                            <Button
                                size="sm"
                                variant="primary"
                                onClick={handleEndorseResolution}
                                disabled={isEndorsing || endorseNoteTooLong}
                                className="gap-1.5 font-bold"
                            >
                                <FileSignature size={14} />
                                {isEndorsing ? t('common.processing') : t('conflict.signResolutionSubmitBtn')}
                            </Button>
                        </div>
                    </div>
                )}

                {resolutions.length === 0 ? (
                    <p className="text-sm text-theme-light italic">
                        {t('conflict.noResolutionNotesYet')}
                    </p>
                ) : (
                    <div className="space-y-3">
                        {resolutions.map((res) => {
                            const hasPositiveWot =
                                Boolean(res.wotTrust) &&
                                (res.wotTrust!.blockedByOffender || res.wotTrust!.trustPercent > 0);
                            const primaryLabel = res.isOwnIdentity
                                ? res.endorserName
                                    ? `${t('conflict.endorserYou')} (${res.endorserName})`
                                    : t('conflict.endorserYou')
                                : res.endorserName;

                            return (
                                <div
                                    key={res.endorsementId}
                                    data-testid="conflict-endorsement-item"
                                    className={`p-3.5 rounded-lg border shadow-sm text-sm space-y-1.5 ${
                                        res.isOwnIdentity
                                            ? 'bg-sky-50/50 border-sky-200'
                                            : hasPositiveWot && !res.wotTrust?.blockedByOffender
                                                ? 'bg-emerald-50/40 border-emerald-200'
                                                : 'bg-bg-card-alternate border-theme-subtle opacity-85'
                                    }`}
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <div className="flex flex-wrap items-center gap-2 min-w-0">
                                            {primaryLabel ? (
                                                <span className="font-bold text-theme-primary">
                                                    {primaryLabel}
                                                </span>
                                            ) : (
                                                <TechId value={res.endorserId} kind="did" />
                                            )}

                                            {res.isOwnIdentity ? (
                                                <span className="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold bg-sky-100 text-sky-800 border border-sky-200">
                                                    {t('conflict.endorserOwnBadge')}
                                                </span>
                                            ) : hasPositiveWot ? (
                                                <TrustBadge trust={res.wotTrust} size="sm" />
                                            ) : (
                                                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-100 text-slate-600 border border-slate-200">
                                                    <Shield size={12} />
                                                    {t('conflict.endorserUnknownBadge')}
                                                </span>
                                            )}
                                        </div>
                                        <p className="text-xs text-theme-light">
                                            {formatDateTime(res.resolutionTimestamp)}
                                        </p>
                                    </div>

                                    {res.notes && (
                                        <p className="text-xs italic text-theme-secondary">
                                            "{res.notes}"
                                        </p>
                                    )}

                                    {primaryLabel && (
                                        <Advanced>
                                            <div className="pt-1">
                                                <TechId value={res.endorserId} kind="did" />
                                            </div>
                                        </Advanced>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>

            {/* Manual Local Override (Always visible, subjective decision) */}
            <div
                data-testid="conflict-local-override-section"
                className="bg-bg-card rounded-lg border border-theme-subtle p-6 shadow-sm space-y-4"
            >
                <div className="text-center space-y-2">
                    <h3 className="text-lg font-bold text-theme-primary">{t('conflict.resolutionTitle')}</h3>
                    <p className="text-sm text-theme-light max-w-2xl mx-auto">
                        {localOverride ? t('conflict.localOverrideActiveDesc') : t('conflict.resolutionDescription')}
                    </p>
                </div>

                {!localOverride ? (
                    <div className="max-w-xl mx-auto space-y-4">
                        <AlertBanner
                            data-testid="conflict-local-override-warning"
                            variant={trustedEndorsementCount === 0 ? "warning" : "info"}
                        >
                            <div className="space-y-1">
                                {trustedEndorsementCount === 0 && (
                                    <p className="font-bold">
                                        {t('conflict.localOverrideNoTrustedWarning')}
                                    </p>
                                )}
                                <p>{t('conflict.localOverrideScopeWarning')}</p>
                            </div>
                        </AlertBanner>
                        <Textarea
                            rows={3}
                            placeholder={t('conflict.resolutionPlaceholder')}
                            value={localNote}
                            onChange={(e) => setLocalNote(e.target.value)}
                        />
                        <Button
                            size="lg"
                            variant="secondary"
                            className="w-full"
                            onClick={() => handleLocalOverride(true)}
                            disabled={isOverriding}
                        >
                            {isOverriding ? t('common.processing') : t('conflict.markSettledButton')}
                        </Button>
                    </div>
                ) : (
                    <div className="max-w-md mx-auto">
                        <Button
                            size="md"
                            variant="outline"
                            className="w-full"
                            onClick={() => handleLocalOverride(false)}
                            disabled={isOverriding}
                        >
                            {isOverriding ? t('common.processing') : t('conflict.revokeLocalOverrideBtn')}
                        </Button>
                    </div>
                )}
            </div>

            <div className="flex justify-center pt-4">
                <Button size="lg" onClick={onBack} variant="outline" className="px-12">{t('conflict.closeReportButton')}</Button>
            </div>
            </div>
        </PageLayout>
    );
}
