import { useState, useEffect } from "react";
import { voucherService } from "../services/voucherService";
import { settingsService } from "../services/settingsService";
import { profileService } from "../services/profileService";
import { standardsService } from "../services/standardsService";
import { contactService } from "../services/contactService";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { TrustBadge } from "./ui/TrustBadge";
import { HelpIcon } from "./ui/HelpIcon";
import { useSession } from "../context/SessionContext";
import { useTranslation } from "react-i18next";
import { AppSettings, VoucherDetails, Contact, TrustStatus, PublicProfile, VoucherStandardDefinition, VoucherL2StatusDto, DeadQuorumAdjudicationDto } from "../types";
import { getMissingProfileHint } from "../utils/signatureHints";
import { stringifyError, translateError } from "../utils/errorHelper";
import ContactDialog from "./ContactDialog";
import { PageLayout } from "./ui/PageLayout";
import { PageLoadingState, PageErrorState } from "./ui/PageState";
import { AlertBanner } from "./ui/AlertBanner";
import { InfoRow } from "./ui/InfoRow";
import { Advanced } from "./ui/Advanced";
import { AdvancedDisclosure } from "./ui/AdvancedDisclosure";
import { TechId } from "./ui/TechId";
import { 
    Shield, 
    History, 
    FileSignature, 
    AlertCircle, 
    AlertTriangle,
    ArrowLeftRight,
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
    const { t, i18n } = useTranslation();
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
    const [claimPanelOpen, setClaimPanelOpen] = useState(false);
    const [selectedGuarantorId, setSelectedGuarantorId] = useState("");
    const [claimAmount, setClaimAmount] = useState("");
    const [isClaimingGuarantor, setIsClaimingGuarantor] = useState(false);
    const [claimFeedback, setClaimFeedback] = useState<{ type: 'success' | 'error'; msg: string } | null>(null);
    const [swapOfferJson, setSwapOfferJson] = useState("");
    const [swapClaimJson, setSwapClaimJson] = useState("");
    const [deadAnchorHexInput, setDeadAnchorHexInput] = useState("");
    const [replAnchorHexInput, setReplAnchorHexInput] = useState("");
    const [swapSubmissionsText, setSwapSubmissionsText] = useState("");
    const [swapAdjudication, setSwapAdjudication] = useState<DeadQuorumAdjudicationDto | null>(null);
    const [swapError, setSwapError] = useState<string | null>(null);
    const [isRunningSwap, setIsRunningSwap] = useState(false);


    const voucher = details?.voucher;
    const isQuarantined = details && details.status === 'quarantined';

    useEffect(() => {
        let active = true;
        if (voucher?.creator.id) {
            voucherService.checkReputation(voucher.creator.id)
                .then(res => {
                    if (active) setTrustStatus(res);
                })
                .catch(e => logger.error(`Reputation check error: ${stringifyError(e)}`));
        }
        return () => { active = false; };
    }, [voucher?.creator.id]);

    useEffect(() => {
        let active = true;
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
                if (!active) return;
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
                if (!active) return;
                setParsedStandards(parsed);
            } catch (e) {
                if (!active) return;
                const msg = `${t('voucher.errorFetchingDetails')}: ${stringifyError(e)}`;
                logger.error(msg);
                setErrorMsg(msg);
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchDetails();
        return () => { active = false; };
    }, [voucherId, t]);

    useEffect(() => {
        let active = true;
        contactService.getContacts()
            .then(res => {
                if (active) setContacts(res);
            })
            .catch(e => logger.error(`Failed to fetch contacts: ${stringifyError(e)}`));
        return () => { active = false; };
    }, []);

    useEffect(() => {
        let active = true;
        if (isQuarantined && !proofId && !isFetchingProofId) {
            const fetchProofId = async () => {
                setIsFetchingProofId(true);
                try {
                    const id = await voucherService.getProofId(voucherId);
                    if (active) setProofId(id);
                } catch (e) {
                    logger.error(`Failed to fetch proof ID for voucher ${voucherId}: ${stringifyError(e)}`);
                } finally {
                    if (active) setIsFetchingProofId(false);
                }
            };
            fetchProofId();
        }
        return () => { active = false; };
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
                if (statusDto.status === 'spent' || statusDto.status === 'conflict') {
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

    if (isLoading) {
        return (
            <PageLayout title={t('voucher.detailsTitle')} onBack={goBack}>
                <PageLoadingState message={t('common.loading')} />
            </PageLayout>
        );
    }
    if (errorMsg || !details || !voucher) {
        return (
            <PageLayout title={t('voucher.detailsTitle')} onBack={goBack}>
                <PageErrorState message={errorMsg || t('voucher.notFound')} />
            </PageLayout>
        );
    }

    const matchedStandard = (() => {
        if (!voucher) return undefined;
        const targetUuid = voucher.voucherStandard.uuid;
        return parsedStandards[targetUuid] || Object.values(parsedStandards).find(s => s.immutable.identity.uuid === targetUuid);
    })();

    const currentLang = i18n.language ? i18n.language.split('-')[0] : 'de';
    const standardDescriptions = matchedStandard?.mutable?.i18n?.descriptions || {};
    const standardDescription = standardDescriptions[currentLang] || standardDescriptions['de'] || standardDescriptions['en'] || Object.values(standardDescriptions)[0] || '';
    const standardFootnotes = matchedStandard?.mutable?.i18n?.footnotes || {};
    const standardFootnote = standardFootnotes[currentLang] || standardFootnotes['de'] || standardFootnotes['en'] || Object.values(standardFootnotes)[0] || '';
    const allowPartialTransfers = matchedStandard?.immutable?.features?.allowPartialTransfers ?? false;

    const signatureHint = statusInfo?.id === 'incomplete' && userProfile
        ? (matchedStandard ? getMissingProfileHint(matchedStandard, userProfile, t) : null)
        : null;

    const guarantorProgress = (() => {
        if (statusInfo?.id !== 'incomplete' || !voucher) return null;
        const minRequired = matchedStandard?.immutable?.issuance?.additionalSignaturesRange?.[0] ?? 0;
        const customRules = matchedStandard?.immutable?.customRules || {};
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

    const guarantorSignatures = (voucher?.signatures || []).filter(
        (s) => s.role === 'guarantor' && s.signerId && s.signerId !== ownUserId
    );
    const canClaimAtGuarantor =
        statusInfo?.id === 'active' && !isOwnVoucher && guarantorSignatures.length > 0;

    const findMatchingStandardToml = async (): Promise<string> => {
        if (!voucher) return "";
        const targetUuid = voucher.voucherStandard.uuid;
        const standardsList = await standardsService.getStandards();
        const matchingStd = standardsList.find((s) => {
            const match = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
            return match && match[1] === targetUuid;
        });
        return matchingStd?.content || "";
    };

    const handleOpenClaimPanel = () => {
        if (!voucher || !details) return;
        setClaimFeedback(null);
        if (!selectedGuarantorId && guarantorSignatures.length > 0) {
            setSelectedGuarantorId(guarantorSignatures[0].signerId);
        }
        setClaimAmount(details.currentAmount ?? voucher.nominalValue.amount);
        setClaimPanelOpen((prev) => !prev);
    };

    const handleCreateGuarantorClaim = async () => {
        if (!voucher || !details) return;
        const targetGuarantor = selectedGuarantorId || guarantorSignatures[0]?.signerId;
        if (!targetGuarantor) return;
        setIsClaimingGuarantor(true);
        setClaimFeedback(null);
        try {
            const standardToml = await findMatchingStandardToml();
            const res = await protectAction(async (pwd) => {
                return await voucherService.createGuarantorClaimBundle({
                    localInstanceId: details.localInstanceId,
                    guarantorId: targetGuarantor,
                    amount: claimAmount.trim() || (details.currentAmount ?? voucher.nominalValue.amount),
                    standardToml,
                    notes: t('voucher.claimNoteDefault'),
                    password: pwd || null,
                });
            });
            if (res) {
                const filePath = await save({
                    defaultPath: settings?.lastUsedDirectory
                        ? `${settings.lastUsedDirectory}/claim-${voucher.voucherId.slice(0, 8)}.hmcbundle`
                        : `claim-${voucher.voucherId.slice(0, 8)}.hmcbundle`,
                    filters: [
                        { name: t('voucher.export.allFiles'), extensions: ['hmcbundle', '*'] },
                    ],
                });
                if (filePath) {
                    await writeFile(filePath, new Uint8Array(res.bundleData));
                }
                setClaimFeedback({ type: 'success', msg: t('voucher.claimBundleSaved') });
                setClaimPanelOpen(false);
                await refreshDetails();
            }
        } catch (e) {
            setClaimFeedback({ type: 'error', msg: translateError(e, t) });
        } finally {
            setIsClaimingGuarantor(false);
        }
    };

    const handleCreateSwapOffer = async () => {
        if (!deadAnchorHexInput.trim() || !replAnchorHexInput.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            const offer = await protectAction(async (pwd) => {
                return await voucherService.createDeadQuorumSwapOffer({
                    deadAnchorIdHex: deadAnchorHexInput.trim(),
                    replacementAnchorIdHex: replAnchorHexInput.trim(),
                    password: pwd || null,
                });
            });
            if (offer) setSwapOfferJson(offer);
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
    };

    const handleCreateSwapClaim = async () => {
        if (!details || !swapOfferJson.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            const claimJson = await protectAction(async (pwd) => {
                return await voucherService.createDeadQuorumSwapClaim({
                    offerJson: swapOfferJson.trim(),
                    localInstanceId: details.localInstanceId,
                    password: pwd || null,
                });
            });
            if (claimJson) setSwapClaimJson(claimJson);
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
    };

    const handleAdjudicateSwap = async () => {
        if (!swapOfferJson.trim() || !swapSubmissionsText.trim()) return;
        setIsRunningSwap(true);
        setSwapError(null);
        try {
            let submissions: string[];
            const raw = swapSubmissionsText.trim();
            if (raw.startsWith('[')) {
                const arr = JSON.parse(raw) as unknown[];
                submissions = arr.map((item) => (typeof item === 'string' ? item : JSON.stringify(item)));
            } else {
                submissions = raw.split(/\n---\n|\n(?=\{)/).map((s) => s.trim()).filter(Boolean);
            }
            const standardToml = await findMatchingStandardToml();
            const adj = await protectAction(async (pwd) => {
                return await voucherService.adjudicateDeadQuorumSwap({
                    offerJson: swapOfferJson.trim(),
                    submissionsJson: submissions,
                    standardToml,
                    password: pwd || null,
                });
            });
            if (adj) {
                setSwapAdjudication(adj);
                await refreshDetails();
            }
        } catch (e) {
            setSwapError(translateError(e, t));
        } finally {
            setIsRunningSwap(false);
        }
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
            setSignatureImportFeedback({ type: 'error', msg: `${t('voucher.failedRemoveSignature')}: ${translateError(e, t)}` });
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
                <>
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
                    {canClaimAtGuarantor && (
                        <Button
                            data-testid="claim-at-guarantor-btn"
                            variant="secondary"
                            size="sm"
                            onClick={handleOpenClaimPanel}
                            className="gap-2 text-xs font-bold bg-amber-50 text-amber-900 border border-amber-300 hover:bg-amber-100 shadow-sm"
                        >
                            <Shield size={15} />
                            {t('voucher.claimAtGuarantorBtn')}
                        </Button>
                    )}
                    {statusInfo?.id === 'active' && (
                        <Advanced>
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
                        </Advanced>
                    )}
                    {statusInfo?.id === 'incomplete' && (
                        <Button variant="primary" size="sm" onClick={() => setShowExportModal(true)} className="gap-2">
                            <FileSignature size={16} /> {t('voucher.signButton')}
                        </Button>
                    )}
                    <Advanced>
                        <button 
                            onClick={() => setShowJson(!showJson)}
                            className={`p-2 rounded-xl transition-all ${showJson ? 'bg-theme-secondary text-white' : 'bg-white text-theme-light hover:bg-white/80 border border-theme-subtle/50 shadow-sm'}`}
                            title={t('voucher.toggleJson')}
                            aria-label={t('voucher.toggleJson')}
                        >
                            <Json size={18} />
                        </button>
                    </Advanced>
                </>
            }
        >

            <div className="max-w-5xl mx-auto space-y-6">
                {signatureImportFeedback && (
                    <AlertBanner 
                        variant={signatureImportFeedback.type} 
                        onDismiss={() => setSignatureImportFeedback(null)}
                    >
                        {signatureImportFeedback.msg}
                    </AlertBanner>
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
                                    <h1 className="text-5xl font-black text-theme-primary tracking-tighter">
                                        {details.currentAmount ?? voucher.nominalValue.amount}
                                    </h1>
                                    {details.currentAmount && details.currentAmount !== voucher.nominalValue.amount && (
                                        <span className="text-sm font-bold text-theme-light">
                                            / {voucher.nominalValue.amount}
                                        </span>
                                    )}
                                    <span className="text-xl font-bold text-theme-light uppercase tracking-widest">{details.displayCurrency}</span>
                                </div>
                                <div className="flex flex-wrap items-center gap-2 mt-2">
                                    <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${statusInfo?.bgColor} ${statusInfo?.color}`}>{statusInfo?.label}</span>
                                    {details.isTestVoucher && <span className="px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest bg-rose-500 text-white shadow-lg shadow-rose-200">{t('voucher.testVoucher')}</span>}
                                    <TrustBadge trust={details.trust} size="md" />
                                    {(details.reducedGuarantorProtection || l2Status?.reducedGuarantorProtection) && (
                                        <span
                                            data-testid="reduced-guarantor-protection-badge"
                                            title={t('voucher.reducedGuarantorProtectionHint')}
                                            className="inline-flex items-center gap-1 px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-amber-100 text-amber-900 border border-amber-300"
                                        >
                                            <AlertTriangle size={12} />
                                            {t('voucher.reducedGuarantorProtectionBadge')}
                                        </span>
                                    )}
                                    {l2Status && (
                                        <Advanced>
                                            <span className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${
                                                l2Status.status === 'unspent'
                                                    ? 'bg-emerald-100 text-emerald-800 border border-emerald-200'
                                                    : l2Status.status === 'spent' || l2Status.status === 'conflict'
                                                    ? 'bg-rose-100 text-rose-800 border border-rose-200'
                                                    : l2Status.status === 'offline'
                                                    ? 'bg-amber-100 text-amber-800 border border-amber-200'
                                                    : 'bg-slate-100 text-slate-800 border border-slate-200'
                                            }`}>
                                                {l2Status.status === 'unspent' && t('voucher.l2StatusUnspent')}
                                                {l2Status.status === 'spent' && t('voucher.l2StatusSpent')}
                                                {l2Status.status === 'conflict' && t('voucher.statusQuarantined')}
                                                {l2Status.status === 'offline' && t('voucher.l2StatusOffline')}
                                                {l2Status.status === 'offline_only' && t('voucher.l2StatusOfflineOnly', 'Offline-only (N=0)')}
                                                {l2Status.status === 'no_servers' && t('voucher.l2StatusNoServers')}
                                            </span>
                                        </Advanced>
                                    )}
                                </div>
                            </div>
                        </div>
                        
                        <div className="flex flex-col gap-3 w-full md:w-auto">
                            {l2Error && (
                                <Advanced>
                                    <p className="text-[10px] font-bold text-rose-500 max-w-[200px] truncate">{l2Error}</p>
                                </Advanced>
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

                {claimFeedback && (
                    <AlertBanner
                        variant={claimFeedback.type}
                        onDismiss={() => setClaimFeedback(null)}
                    >
                        {claimFeedback.msg}
                    </AlertBanner>
                )}

                {claimPanelOpen && canClaimAtGuarantor && (
                    <Card className="border border-amber-200 bg-amber-50/50 shadow-premium">
                        <div className="space-y-4">
                            <div className="flex items-start justify-between gap-2">
                                <div>
                                    <h3 className="text-sm font-black text-amber-950 uppercase tracking-wider">
                                        {t('voucher.claimAtGuarantorTitle')}
                                    </h3>
                                    <p className="text-xs text-amber-900/80 mt-0.5">
                                        {t('voucher.claimAtGuarantorDesc')}
                                    </p>
                                </div>
                                <button
                                    type="button"
                                    onClick={() => setClaimPanelOpen(false)}
                                    className="p-1 text-amber-800 hover:bg-amber-100 rounded-lg"
                                    aria-label={t('common.close')}
                                >
                                    <X size={16} />
                                </button>
                            </div>
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                <div>
                                    <label className="block text-[10px] font-bold uppercase text-amber-950 mb-1">
                                        {t('voucher.claimGuarantorSelectLabel')}
                                    </label>
                                    <select
                                        value={selectedGuarantorId || guarantorSignatures[0]?.signerId || ""}
                                        onChange={(e) => setSelectedGuarantorId(e.target.value)}
                                        className="w-full px-3 py-2 text-xs font-mono rounded-xl border border-amber-300 bg-white"
                                    >
                                        {guarantorSignatures.map((sig) => (
                                            <option key={sig.signerId} value={sig.signerId}>
                                                {sig.details?.firstName || sig.details?.lastName
                                                    ? `${sig.details.firstName || ''} ${sig.details.lastName || ''}`.trim()
                                                    : sig.signerId.slice(0, 22)}
                                            </option>
                                        ))}
                                    </select>
                                </div>
                                <div>
                                    <label className="block text-[10px] font-bold uppercase text-amber-950 mb-1">
                                        {t('voucher.claimAmountLabel')} ({details.displayCurrency})
                                    </label>
                                    <input
                                        type="text"
                                        value={claimAmount}
                                        onChange={(e) => setClaimAmount(e.target.value)}
                                        disabled={!allowPartialTransfers}
                                        className="w-full px-3 py-2 text-xs font-bold rounded-xl border border-amber-300 bg-white disabled:opacity-70"
                                    />
                                </div>
                            </div>
                            <div className="flex justify-end gap-2">
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => setClaimPanelOpen(false)}
                                >
                                    {t('common.cancel')}
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={handleCreateGuarantorClaim}
                                    disabled={isClaimingGuarantor}
                                >
                                    {isClaimingGuarantor ? t('common.saving') : t('voucher.claimExportBtn')}
                                </Button>
                            </div>
                        </div>
                    </Card>
                )}

                {showJson && (
                    <Advanced>
                        <Card variant="glass" className="bg-slate-900 border-none shadow-2xl p-0 overflow-hidden">
                            <pre className="text-[11px] leading-relaxed text-indigo-300 font-mono p-6 overflow-x-auto selection:bg-indigo-500 selection:text-white max-h-[600px]">
                                {JSON.stringify(details.voucher, null, 2)}
                            </pre>
                        </Card>
                    </Advanced>
                )}

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
                            description={standardDescription}
                            footnote={standardFootnote}
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
                                    {(details.reducedGuarantorProtection || l2Status?.reducedGuarantorProtection) && (
                                        <span
                                            data-testid="reduced-guarantor-protection-badge"
                                            title={t('voucher.reducedGuarantorProtectionHint')}
                                            className="inline-flex items-center gap-1 px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-amber-100 text-amber-900 border border-amber-300"
                                        >
                                            <AlertTriangle size={12} />
                                            {t('voucher.reducedGuarantorProtectionBadge')}
                                        </span>
                                    )}
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
                                <Advanced>
                                    <InfoRow label={t('voucher.idLabel')} icon={Shield}>
                                        <TechId value={voucher.voucherId} kind="voucher" />
                                    </InfoRow>
                                </Advanced>
                                <InfoRow label={t('voucher.issueDateLabel')} icon={Clock}>{formatDateTime(voucher.creationDate)}</InfoRow>
                                <InfoRow label={t('voucher.validUntilLabel')} icon={AlertCircle}>{formatDateTime(voucher.validUntil || undefined)}</InfoRow>
                                <div className="pt-4 border-t border-theme-subtle/30 grid grid-cols-2 gap-4">
                                    <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                        <p className="text-[9px] font-black uppercase text-theme-light mb-1">{t('voucher.divisibleLabel')}</p>
                                        <p className="text-xs font-bold">{allowPartialTransfers ? t('voucher.divisibleYes') : t('voucher.divisibleNo')}</p>
                                    </div>
                                    <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                        <p className="text-[9px] font-black uppercase text-theme-light mb-1">{t('voucher.collateralLabel')}</p>
                                        <p className="text-xs font-bold">{voucher.collateral?.amount ? `${voucher.collateral.amount} ${voucher.collateral.abbreviation || voucher.collateral.unit}` : t('voucher.noCollateral')}</p>
                                    </div>
                                </div>
                            </div>
                        </Card>

                        <AdvancedDisclosure
                            title={t('voucher.deadQuorumSwapTitle')}
                            subtitle={t('voucher.deadQuorumSwapSubtitle')}
                            icon={<ArrowLeftRight size={15} className="text-theme-primary" />}
                        >
                            <div className="space-y-3 pt-1 text-xs">
                                {swapError && (
                                    <p className="text-[11px] font-medium text-rose-600">{swapError}</p>
                                )}
                                <div className="space-y-1.5">
                                    <p className="font-bold text-[11px] text-theme-secondary">
                                        {t('voucher.swapClaimSectionTitle')}
                                    </p>
                                    <textarea
                                        rows={2}
                                        value={swapOfferJson}
                                        onChange={(e) => setSwapOfferJson(e.target.value)}
                                        placeholder={t('voucher.swapOfferJsonPlaceholder')}
                                        className="w-full p-2 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                                    />
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={handleCreateSwapClaim}
                                        disabled={isRunningSwap || !swapOfferJson.trim() || statusInfo?.id !== 'active'}
                                    >
                                        {t('voucher.createSwapClaimBtn')}
                                    </Button>
                                    {swapClaimJson && (
                                        <textarea
                                            readOnly
                                            rows={3}
                                            value={swapClaimJson}
                                            className="w-full p-2 text-[10px] font-mono rounded-lg border border-emerald-300 bg-emerald-50/40"
                                        />
                                    )}
                                </div>

                                <div className="pt-2 border-t border-theme-subtle/40 space-y-2">
                                    <p className="font-bold text-[11px] text-theme-secondary">
                                        {t('voucher.swapAdjudicateSectionTitle')}
                                    </p>
                                    <input
                                        type="text"
                                        value={deadAnchorHexInput}
                                        onChange={(e) => setDeadAnchorHexInput(e.target.value)}
                                        placeholder={t('voucher.deadAnchorHexPlaceholder')}
                                        className="w-full px-2.5 py-1.5 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                                    />
                                    <input
                                        type="text"
                                        value={replAnchorHexInput}
                                        onChange={(e) => setReplAnchorHexInput(e.target.value)}
                                        placeholder={t('voucher.replAnchorHexPlaceholder')}
                                        className="w-full px-2.5 py-1.5 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                                    />
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={handleCreateSwapOffer}
                                        disabled={isRunningSwap || !deadAnchorHexInput.trim() || !replAnchorHexInput.trim()}
                                    >
                                        {t('voucher.createSwapOfferBtn')}
                                    </Button>
                                    <textarea
                                        rows={2}
                                        value={swapSubmissionsText}
                                        onChange={(e) => setSwapSubmissionsText(e.target.value)}
                                        placeholder={t('voucher.swapSubmissionsPlaceholder')}
                                        className="w-full p-2 text-[11px] font-mono rounded-lg border border-theme-subtle bg-bg-input"
                                    />
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={handleAdjudicateSwap}
                                        disabled={isRunningSwap || !swapOfferJson.trim() || !swapSubmissionsText.trim()}
                                    >
                                        {t('voucher.adjudicateSwapBtn')}
                                    </Button>
                                    {swapAdjudication && (
                                        <div className="p-2.5 rounded-lg border border-emerald-200 bg-emerald-50/50 text-[11px] space-y-1">
                                            <div>
                                                <span className="font-bold">{t('voucher.swapWinnerLabel')}: </span>
                                                <span className="font-mono">{swapAdjudication.winnerRecipientId || t('common.na')}</span>
                                            </div>
                                            <div>
                                                {t('voucher.swapOffendersExposed', { count: swapAdjudication.exposedOffenderDids.length })}
                                            </div>
                                        </div>
                                    )}
                                </div>
                            </div>
                        </AdvancedDisclosure>
                    </div>
                </div>
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