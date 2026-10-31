import { useState, useEffect } from "react";
import { voucherService } from "../services/voucherService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { TrustBadge } from "./ui/TrustBadge";
import { HelpIcon } from "./ui/HelpIcon";
import { useSession } from "../context/SessionContext";
import { useTranslation } from "react-i18next";
import { AppSettings, VoucherDetails, Contact, TrustStatus, PublicProfile, VoucherStandardDefinition, VoucherL2StatusDto } from "../types";
import { getMissingProfileHint } from "../utils/signatureHints";
import { stringifyError, translateError } from "../utils/errorHelper";
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
    ShieldAlert,
    Store,
    FolderOpen,
    X
} from "lucide-react";

// Modular Components
import { CreatorSection } from "./voucher/CreatorSection";
import { PolicySection } from "./voucher/PolicySection";
import { TimelineSection } from "./voucher/TimelineSection";
import { ExportSigningRequestModal } from "./voucher/ExportSigningRequestModal";
import { SignatureRequestBanner } from "./voucher/SignatureRequestBanner";

import { useNavigation } from "../context/NavigationContext";

export function VoucherDetailsView() {
    const { t } = useTranslation();
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
    const [ownUserId, setOwnUserId] = useState<string>("");
    const [savedAskFilePath, setSavedAskFilePath] = useState<string | null>(null);
    const [signatureImportFeedback, setSignatureImportFeedback] = useState<{ type: 'success' | 'error'; msg: string } | null>(null);
    const [pendingSigImport, setPendingSigImport] = useState<{ fileBytes: Uint8Array; standardToml: string } | null>(null);
    const [sigContainerPassword, setSigContainerPassword] = useState("");
    const [sigPasswordError, setSigPasswordError] = useState("");
    const [isCheckingL2, setIsCheckingL2] = useState(false);
    const [l2Status, setL2Status] = useState<VoucherL2StatusDto | null>(null);
    const [l2Error, setL2Error] = useState<string | null>(null);


    const voucher = details?.voucher;
    const isQuarantined = details && details.status === 'quarantined';

    useEffect(() => {
        if (voucher?.creator.id) {
            voucherService.checkReputation(voucher.creator.id)
                .then(setTrustStatus)
                .catch(e => logger.error(`Reputation check error: ${stringifyError(e)}`));
        }
    }, [voucher?.creator.id]);

    useEffect(() => {
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const [result, currentSettings, profile, standards, uid] = await Promise.all([
                    voucherService.getDetails(voucherId),
                    settingsService.getSettings().catch(() => null),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards().catch(() => []),
                    profileService.getUserId().catch(() => "")
                ]);
                setDetails(result);
                setSettings(currentSettings);
                setUserProfile(profile);
                setOwnUserId(uid || "");

                const parsed: Record<string, VoucherStandardDefinition> = {};
                for (const s of standards) {
                    try {
                        parsed[s.id] = await standardsService.parseStandard(s.content);
                    } catch (e) {
                        logger.error(`Failed to parse standard ${s.id}: ${stringifyError(e)}`);
                    }
                }
                setParsedStandards(parsed);
            } catch (e) {
                const msg = `${t('voucher.errorFetchingDetails')}: ${stringifyError(e)}`;
                logger.error(msg);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchDetails();
    }, [voucherId, t]);

    useEffect(() => {
        contactService.getContacts().then(setContacts).catch(e => logger.error(`Failed to fetch contacts: ${stringifyError(e)}`));
    }, []);

    useEffect(() => {
        if (isQuarantined && !proofId && !isFetchingProofId) {
            const fetchProofId = async () => {
                setIsFetchingProofId(true);
                try {
                    const id = await voucherService.getProofId(voucherId);
                    setProofId(id);
                } catch (e) {
                    logger.error(`Failed to fetch proof ID for voucher ${voucherId}: ${stringifyError(e)}`);
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
            logger.error(`Failed to refresh voucher details: ${stringifyError(e)}`);
        }
    };

    const handleCheckL2Status = async () => {
        if (!details) return;
        setIsCheckingL2(true);
        setL2Error(null);
        try {
            const statusDto = await protectAction(async (pwd) => {
                return await settingsService.checkVoucherL2Status(details.localInstanceId, pwd || null);
            });
            if (statusDto) {
                setL2Status(statusDto);
                if (statusDto.status === 'spent') {
                    await refreshDetails();
                }
            }
        } catch (e) {
            setL2Error(translateError(e, t));
        } finally {
            setIsCheckingL2(false);
        }
    };

    function getStatusInfo(details: VoucherDetails) {
        const { status } = details;
        if (status === "active") return { id: 'active', label: t('voucher.statusActive'), color: 'text-emerald-600', bgColor: 'bg-emerald-50', icon: CheckCircle2 };
        if (status === "archived") return { id: 'archived', label: t('voucher.statusArchived'), color: 'text-gray-600', bgColor: 'bg-gray-50', icon: History };
        if (status === "incomplete") return { id: 'incomplete', label: t('voucher.statusIncomplete'), color: 'text-amber-600', bgColor: 'bg-amber-50', icon: Clock };
        if (status === "quarantined") return { id: 'quarantined', label: t('voucher.statusQuarantined'), color: 'text-rose-600', bgColor: 'bg-rose-50', icon: ShieldAlert };
        if (status === "endorsed") return { id: 'endorsed', label: t('voucher.statusEndorsed'), color: 'text-indigo-600', bgColor: 'bg-indigo-50', icon: FileSignature };
        if (status === "expired") return { id: 'expired', label: t('voucher.statusExpired'), color: 'text-orange-600', bgColor: 'bg-orange-50', icon: Clock };
        return { id: 'unknown', label: t('voucher.statusUnknown'), color: 'text-gray-600', bgColor: 'bg-gray-50', icon: Info };
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : t('common.na');
    const statusInfo = details ? getStatusInfo(details) : null;
    const StatusIcon = statusInfo?.icon;

    if (isLoading) return <div className="text-center p-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">{t('voucher.loadingDetails')}</div>;
    if (errorMsg || !details || !voucher) return <div className="p-10 text-center text-rose-500 font-bold">{errorMsg || t('voucher.notFound')}</div>;

    const signatureHint = statusInfo?.id === 'incomplete' && userProfile
        ? (() => {
            const targetUuid = voucher.voucherStandard.uuid;
            const standard = parsedStandards[targetUuid] || Object.values(parsedStandards).find(s => s.immutable.identity.uuid === targetUuid);
            return standard ? getMissingProfileHint(standard, userProfile, t) : null;
        })() : null;

    const guarantorProgress = (() => {
        if (statusInfo?.id !== 'incomplete' || !voucher) return null;
        const targetUuid = voucher.voucherStandard.uuid;
        const standard = parsedStandards[targetUuid] || Object.values(parsedStandards).find(s => s.immutable.identity.uuid === targetUuid);
        const minRequired = standard?.immutable?.issuance?.additionalSignaturesRange?.[0] ?? 0;
        const customRules = standard?.immutable?.customRules || {};
        const hasGenderMale = Object.values(customRules).some(r => r.expression?.includes("gender == '1'"));
        const hasGenderFemale = Object.values(customRules).some(r => r.expression?.includes("gender == '2'"));

        const nonCreatorSigs = (voucher.signatures || []).filter(s => s.role !== 'creator');
        if (hasGenderMale && hasGenderFemale && minRequired === 2) {
            const maleCount = nonCreatorSigs.filter(s => s.details?.gender === '1').length;
            const femaleCount = nonCreatorSigs.filter(s => s.details?.gender === '2').length;
            if (maleCount >= 1 && femaleCount >= 1) {
                return t('voucher.guarantorStatusBothPresent');
            } else if (maleCount >= 1 && femaleCount === 0) {
                return t('voucher.guarantorStatusMaleDoneFemaleMissing');
            } else if (maleCount === 0 && femaleCount >= 1) {
                return t('voucher.guarantorStatusFemaleDoneMaleMissing');
            } else {
                return t('voucher.guarantorStatusBothMissing');
            }
        } else if (minRequired > 0) {
            return t('voucher.guarantorStatusCount', { current: nonCreatorSigs.length, required: minRequired });
        }
        return null;
    })();

    const isOwnVoucher = Boolean(voucher?.creator.id && ownUserId && voucher.creator.id === ownUserId);
    const canRedeemAtIssuer = statusInfo?.id === 'active' && !isOwnVoucher && Boolean(voucher?.creator.id);

    const handleRedeemAtIssuer = () => {
        if (!voucher || !details) return;
        navigate({
            view: 'send_vouchers',
            initialRecipientId: voucher.creator.id,
            initialVoucherId: details.localInstanceId,
            initialNote: t('voucher.redeemNoteDefault', { voucherId: voucher.voucherId.slice(0, 8) })
        });
    };

    const handleImportSignatureFile = async () => {
        try {
            setSignatureImportFeedback(null);
            const selectedPath = await open({
                filters: [
                    { name: t('voucher.signRequest.signatureResponseFilter'), extensions: ['sig', 'humocosig'] },
                    { name: t('voucher.export.allFiles'), extensions: ['*'] }
                ]
            });
            if (!selectedPath || typeof selectedPath !== 'string') return;

            const fileBytes = await readFile(selectedPath);
            const targetUuid = voucher.voucherStandard.uuid;
            const standardsList = await standardsService.getStandards();
            const matchingStd = standardsList.find(s => {
                const match = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                return match && match[1] === targetUuid;
            });
            const standardTomlContent = matchingStd?.content || "";

            await executeSignatureAttachment(new Uint8Array(fileBytes), standardTomlContent);
        } catch (e) {
            setSignatureImportFeedback({ type: 'error', msg: `${t('voucher.signatureAttachFailed')}: ${translateError(e, t)}` });
        }
    };

    const executeSignatureAttachment = async (fileBytes: Uint8Array, standardToml: string, containerPwd?: string) => {
        try {
            await protectAction(async (walletPassword) => {
                return await voucherService.processAndAttachSignature(
                    Array.from(fileBytes),
                    standardToml,
                    containerPwd || undefined,
                    walletPassword || undefined
                );
            });
            
            const updatedDetails = await voucherService.getDetails(voucherId);
            setDetails(updatedDetails);
            setPendingSigImport(null);
            setSigContainerPassword("");
            setSigPasswordError("");

            if (updatedDetails.status === 'active') {
                setSignatureImportFeedback({
                    type: 'success',
                    msg: t('voucher.allSignaturesComplete')
                });
            } else {
                setSignatureImportFeedback({
                    type: 'success',
                    msg: t('voucher.signatureAttachedTitle')
                });
            }
        } catch (e) {
            const errStr = stringifyError(e);
            const isPasswordRequired = errStr.includes("Password required") || errStr.toLowerCase().includes("decrypt") || errStr.includes("SymmetricEncryption") || errStr.includes("MacError");
            if (isPasswordRequired && !containerPwd) {
                setPendingSigImport({ fileBytes, standardToml });
                setSigPasswordError("");
            } else {
                setSignatureImportFeedback({ type: 'error', msg: `${t('voucher.signatureAttachFailed')}: ${translateError(e, t)}` });
            }
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
            logger.error(`Failed to remove signature: ${stringifyError(e)}`);
            alert(`${t('voucher.failedRemoveSignature')}: ${stringifyError(e)}`);
        } finally {
            setIsRemovingSignature(false);
        }
    };

    return (
        <PageLayout 
            title={t('voucher.detailsTitle')} 
            description={details.displayStandardName}
            onBack={goBack}
            actions={
                <div className="flex flex-wrap items-center justify-end gap-2">
                    {canRedeemAtIssuer && (
                        <Button 
                            variant="secondary" 
                            size="sm" 
                            onClick={handleRedeemAtIssuer} 
                            className="gap-2 text-xs font-bold bg-white text-theme-primary border border-theme-subtle hover:bg-theme-subtle/20 shadow-sm"
                        >
                            <Store size={15} />
                            {t('voucher.redeemAtIssuerBtn')}
                        </Button>
                    )}
                    {statusInfo?.id === 'active' && (
                        <Button 
                            variant="outline" 
                            size="sm" 
                            onClick={handleCheckL2Status} 
                            disabled={isCheckingL2} 
                            className="gap-1.5 text-xs font-bold"
                        >
                            {isCheckingL2 ? <Clock className="animate-spin" size={14} /> : <Shield size={14} />}
                            {isCheckingL2 ? t('voucher.checkingL2Status') : t('voucher.checkL2Status')}
                        </Button>
                    )}
                    {statusInfo?.id === 'incomplete' && (
                        <Button variant="primary" size="sm" onClick={() => setShowExportModal(true)} className="gap-2">
                            <FileSignature size={16} /> {t('voucher.signButton')}
                        </Button>
                    )}
                    <button 
                        onClick={() => setShowJson(!showJson)}
                        className={`p-2 rounded-xl transition-all ${showJson ? 'bg-theme-secondary text-white' : 'bg-white text-theme-light hover:bg-white/80 border border-theme-subtle/50 shadow-sm'}`}
                        title={t('voucher.toggleJson')}
                        aria-label={t('voucher.toggleJson')}
                    >
                        <Json size={18} />
                    </button>
                </div>
            }
        >

            <div className="max-w-5xl mx-auto space-y-6">
                {signatureImportFeedback && (
                    <div className={`p-4 rounded-2xl flex items-center justify-between gap-3 border shadow-sm animate-in zoom-in duration-300 ${
                        signatureImportFeedback.type === 'error'
                            ? 'bg-rose-50 border-rose-100 text-rose-800'
                            : 'bg-emerald-50 border-emerald-100 text-emerald-800'
                    }`}>
                        <div className="flex items-center gap-3">
                            {signatureImportFeedback.type === 'error' ? <ShieldAlert size={20} className="shrink-0 text-rose-600" /> : <CheckCircle2 size={20} className="shrink-0 text-emerald-600" />}
                            <p className="text-sm font-bold">{signatureImportFeedback.msg}</p>
                        </div>
                        <button
                            onClick={() => setSignatureImportFeedback(null)}
                            className="p-1 rounded-lg hover:bg-black/5"
                            title={t('common.close')}
                            aria-label={t('common.close')}
                        >
                            <X size={16} />
                        </button>
                    </div>
                )}

                {savedAskFilePath && (
                    <div className="p-4 bg-emerald-50 border border-emerald-200 rounded-2xl flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-emerald-900 animate-in fade-in duration-300">
                        <div className="flex items-start gap-3">
                            <CheckCircle2 size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                            <div>
                                <p className="font-bold text-sm text-emerald-950">{t('voucher.askSavedSuccessTitle')}</p>
                                <p className="text-xs text-emerald-800/90 mt-0.5 leading-relaxed">{t('voucher.askSavedSuccessDesc')}</p>
                            </div>
                        </div>
                        <div className="flex items-center gap-2 shrink-0">
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={() => revealItemInDir(savedAskFilePath).catch(() => {})}
                                className="bg-white hover:bg-white text-xs font-bold gap-1.5 shadow-sm"
                            >
                                <FolderOpen size={14} />
                                {t('common.showInFolder')}
                            </Button>
                            <button
                                onClick={() => setSavedAskFilePath(null)}
                                className="p-1.5 text-emerald-700 hover:text-emerald-900 rounded-lg hover:bg-emerald-100/50"
                                title={t('common.close')}
                                aria-label={t('common.close')}
                            >
                                <X size={16} />
                            </button>
                        </div>
                    </div>
                )}

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
                                <div className="flex flex-wrap items-center gap-2 mt-2">
                                    <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${statusInfo?.bgColor} ${statusInfo?.color}`}>{statusInfo?.label}</span>
                                    {details.isTestVoucher && <span className="px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest bg-rose-500 text-white shadow-lg shadow-rose-200">{t('voucher.testVoucher')}</span>}
                                    <TrustBadge trust={details.trust} size="md" />
                                    {l2Status && (
                                        <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${
                                            l2Status.status === 'unspent'
                                                ? 'bg-emerald-100 text-emerald-800 border border-emerald-200'
                                                : l2Status.status === 'spent'
                                                ? 'bg-rose-100 text-rose-800 border border-rose-200'
                                                : l2Status.status === 'offline'
                                                ? 'bg-amber-100 text-amber-800 border border-amber-200'
                                                : 'bg-slate-100 text-slate-800 border border-slate-200'
                                        }`}>
                                            {l2Status.status === 'unspent' && t('voucher.l2StatusUnspent')}
                                            {l2Status.status === 'spent' && t('voucher.l2StatusSpent')}
                                            {l2Status.status === 'offline' && t('voucher.l2StatusOffline')}
                                            {l2Status.status === 'no_servers' && t('voucher.l2StatusNoServers')}
                                        </span>
                                    )}
                                </div>
                            </div>
                        </div>
                        
                        <div className="flex flex-col gap-3 w-full md:w-auto">
                            {statusInfo?.id === 'active' && (
                                <Button
                                    variant="outline"
                                    size="sm"
                                    onClick={handleCheckL2Status}
                                    disabled={isCheckingL2}
                                    className="bg-white/60 hover:bg-white/80 border-theme-subtle text-xs font-bold gap-1.5"
                                >
                                    {isCheckingL2 ? <Clock className="animate-spin" size={14} /> : <Shield size={14} />}
                                    {isCheckingL2 ? t('voucher.checkingL2Status') : t('voucher.checkL2Status')}
                                </Button>
                            )}
                            {l2Error && (
                                <p className="text-[10px] font-bold text-rose-500 max-w-[200px] truncate">{l2Error}</p>
                            )}
                            {isQuarantined && (
                                <Button variant="secondary" size="sm" className="bg-white/20 text-white hover:bg-white/30 border-white/30 backdrop-blur-md" onClick={() => proofId && navigate({ view: 'conflict_details', proofId, previousView: appState })} disabled={!proofId}>
                                    <ShieldAlert size={16} className="mr-2" /> {t('voucher.viewProof')}
                                </Button>
                            )}
                            <div className="bg-white/20 backdrop-blur-md px-4 py-2 rounded-2xl border border-white/30">
                                <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-1">{t('voucher.standardLabel')}</p>
                                <p className="text-xs font-bold text-theme-secondary truncate max-w-[200px]">{details.displayStandardName}</p>
                            </div>
                        </div>
                    </div>
                    
                    {signatureHint && (
                        <div className="bg-blue-500/10 border-t border-blue-500/20 p-4 px-8 flex items-start gap-4">
                            <Info size={18} className="text-blue-500 mt-1 shrink-0" />
                            <p className="text-sm text-blue-900 font-medium leading-relaxed">
                                <span className="font-black uppercase text-[10px] tracking-widest block mb-1">{t('voucher.signatureHint')}</span>
                                {signatureHint}
                            </p>
                        </div>
                    )}
                </Card>

                {statusInfo?.id === 'incomplete' && (
                    <SignatureRequestBanner 
                        onAction={() => setShowExportModal(true)}
                        onImportSignature={handleImportSignatureFile}
                        progressText={guarantorProgress}
                    />
                )}

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
                                voucherStatus={details.status}
                            />
                        </div>

                        <div className="space-y-6">
                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center justify-between w-full">
                                    <div className="flex items-center gap-2">
                                        <Shield size={16} className="text-theme-primary" />
                                        <span className="font-black text-xs uppercase tracking-widest">{t('voucher.trustCardTitle')}</span>
                                    </div>
                                    <HelpIcon topic="voucherTrust" size={13} />
                                </div>
                            }>
                                <div className="space-y-4">
                                    <div>
                                        <TrustBadge trust={details.trust} size="md" />
                                    </div>
                                    {!details.trust?.isOwnVoucher && !details.trust?.blockedByOffender && (
                                        <div className="pt-3 border-t border-theme-subtle/30 text-xs">
                                            {details.trust && details.trust.shortestHops != null && details.trust.trustPercent > 0 ? (
                                                <div>
                                                    <p className="text-[10px] font-black uppercase text-theme-light mb-1.5">{t('voucher.trustPathLabel')}</p>
                                                    <p className="font-medium text-theme-secondary flex items-center flex-wrap gap-1 leading-relaxed">
                                                        {[
                                                            t('voucher.trustPathYou'),
                                                            ...(details.trust.bestPathNames || []),
                                                            `${voucher.creator.firstName || ''} ${voucher.creator.lastName || ''}`.trim() || t('voucher.trustPathIssuer')
                                                        ].join(' → ')}
                                                    </p>
                                                </div>
                                            ) : (
                                                <p className="text-theme-light italic text-xs leading-relaxed">
                                                    {t('voucher.trustNoPathHint')}
                                                </p>
                                            )}
                                        </div>
                                    )}
                                </div>
                            </Card>

                            <Card className="border-none shadow-premium" header={
                                <div className="flex items-center gap-2">
                                    <Info size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('voucher.detailsHeader')}</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <InfoRow label={t('voucher.idLabel')} isMono icon={Shield}>{voucher.voucherId}</InfoRow>
                                    <InfoRow label={t('voucher.issueDateLabel')} icon={Clock}>{formatDateTime(voucher.creationDate)}</InfoRow>
                                    <InfoRow label={t('voucher.validUntilLabel')} icon={AlertCircle}>{formatDateTime(voucher.validUntil || undefined)}</InfoRow>
                                    <div className="pt-4 border-t border-theme-subtle/30 grid grid-cols-2 gap-4">
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">{t('voucher.divisibleLabel')}</p>
                                            <p className="text-xs font-bold">{voucher.voucherStandard.template.allowPartialTransfers ? t('voucher.divisibleYes') : t('voucher.divisibleNo')}</p>
                                        </div>
                                        <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                            <p className="text-[9px] font-black uppercase text-theme-light mb-1">{t('voucher.collateralLabel')}</p>
                                            <p className="text-xs font-bold">{voucher.collateral?.amount ? `${voucher.collateral.amount} ${voucher.collateral.abbreviation || voucher.collateral.unit}` : t('voucher.noCollateral')}</p>
                                        </div>
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>
                )}
            </div>

            <ExportSigningRequestModal 
                isOpen={showExportModal}
                voucherId={voucherId}
                onClose={() => setShowExportModal(false)}
                settings={settings}
                protectAction={protectAction}
                onSettingsRefresh={(newSettings) => setSettings(newSettings)}
                contacts={contacts}
                onSuccess={(savedPath) => setSavedAskFilePath(savedPath)}
            />

            {pendingSigImport && (
                <ConfirmationModal
                    isOpen={true}
                    title={t('voucher.enterSignaturePasswordTitle')}
                    description={
                        <div className="space-y-4 pt-2">
                            <p className="text-xs text-theme-secondary font-medium">
                                {t('voucher.enterSignaturePasswordDesc')}
                            </p>
                            <input
                                type="password"
                                value={sigContainerPassword}
                                onChange={(e) => {
                                    setSigContainerPassword(e.target.value);
                                    setSigPasswordError("");
                                }}
                                placeholder={t('auth.passwordPlaceholder')}
                                className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-sm font-bold"
                                autoFocus
                            />
                            {sigPasswordError && (
                                <p className="text-rose-500 text-xs font-bold">{sigPasswordError}</p>
                            )}
                        </div>
                    }
                    confirmText={t('common.confirm')}
                    onConfirm={() => {
                        if (!sigContainerPassword) {
                            setSigPasswordError(t('voucher.errorEnterPassword'));
                            return;
                        }
                        executeSignatureAttachment(
                            pendingSigImport.fileBytes,
                            pendingSigImport.standardToml,
                            sigContainerPassword
                        );
                    }}
                    onCancel={() => {
                        setPendingSigImport(null);
                        setSigContainerPassword("");
                        setSigPasswordError("");
                    }}
                />
            )}
            
            <ContactDialog
                isOpen={showContactDialog}
                onClose={() => { setShowContactDialog(false); setPendingContactDID(null); setExistingContact(null); }}
                onSave={async (contact: Contact) => {
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(contact, pwd || undefined);
                        await contactService.getContacts().then(setContacts);
                        await refreshDetails();
                    });
                }}
                existingContact={existingContact}
                initialProfile={pendingContactDID ? voucher.signatures.find(s => s.signerId === pendingContactDID)?.details : voucher.creator}
                initialDid={pendingContactDID || voucher.creator.id}
            />

            <ConfirmationModal
                isOpen={showRemoveSignatureModal !== null}
                title={t('voucher.removeSignatureTitle')}
                description={t('voucher.removeSignatureConfirm')}
                confirmText={t('common.remove')}
                confirmVariant="danger"
                onConfirm={handleRemoveSignature}
                onCancel={() => setShowRemoveSignatureModal(null)}
                isProcessing={isRemovingSignature}
            />
        </PageLayout>
    );
}