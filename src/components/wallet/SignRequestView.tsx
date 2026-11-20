import { useState, useEffect, useMemo } from 'react';
import { voucherService } from '../../services/voucherService';
import { postService } from '../../services/postService';
import { settingsService } from '../../services/settingsService';
import { triggerAutoBackup } from '../../sync/syncBridge';
import { standardsService } from '../../services/standardsService';
import { profileService } from '../../services/profileService';
import { wotService } from '../../services/wotService';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { logger } from '../../utils/log';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { translateRuleMessage, translateGentleHint } from '../../utils/signatureHints';
import { VoucherDetails, VoucherStandardInfo, AppConfigDto, SignatureImpact, VoucherTrustDto, PublicProfile, VoucherStandardDefinition, GuarantorAnchorCheckDto } from '../../types';
import { saveBytesToFile } from '../../utils/fileIo';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { PageLayout } from '../ui/PageLayout';
import { AlertBanner } from '../ui/AlertBanner';
import { TrustBadge } from '../ui/TrustBadge';
import { useTranslation } from 'react-i18next';
import { useSession } from '../../context/SessionContext';
import { 
    PenTool, 
    ShieldCheck, 
    ShieldAlert, 
    Info, 
    Lightbulb, 
    CheckCircle2, 
    XCircle,
    User,
    Calendar,
    UserCheck,
    FileText,
    ArrowRight,
    Lock,
    AlertTriangle,
    AlertCircle,
    FolderOpen
} from 'lucide-react';

interface SignRequestViewProps {
    voucherData: VoucherDetails;
    requestMsgId?: string;
    senderDid?: string;
    anchorManifestJson?: string;
    onBack: () => void;
}

export function SignRequestView({ voucherData, requestMsgId, senderDid, anchorManifestJson, onBack }: SignRequestViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [allowedRoles, setAllowedRoles] = useState<string[]>([]);
    const [selectedRole, setSelectedRole] = useState<string>('');
    const [includeDetails, setIncludeDetails] = useState(true);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [feedbackType, setFeedbackType] = useState<'error' | 'success' | null>(null);
    const [isSigning, setIsSigning] = useState(false);
    const [config, setConfig] = useState<AppConfigDto | null>(null);
    const [standardContent, setStandardContent] = useState<string | null>(null);
    const [parsedStandard, setParsedStandard] = useState<VoucherStandardDefinition | null>(null);
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [impact, setImpact] = useState<SignatureImpact | null>(null);
    const [isImpactLoading, setIsImpactLoading] = useState(false);
    const [previewTrust, setPreviewTrust] = useState<VoucherTrustDto | null>(null);
    const [savedSigFilePath, setSavedSigFilePath] = useState<string | null>(null);
    const [postSentSuccess, setPostSentSuccess] = useState(false);
    const [cachedSigBundle, setCachedSigBundle] = useState<{ key: string; bytes: number[] } | null>(null);
    const [selectedGender, setSelectedGender] = useState<string>("1");
    const [isSavingGender, setIsSavingGender] = useState(false);
    const [standardsLoaded, setStandardsLoaded] = useState(false);
    const [guarantorCheck, setGuarantorCheck] = useState<GuarantorAnchorCheckDto | null>(null);
    const [isGuarantorCheckLoading, setIsGuarantorCheckLoading] = useState(false);
    const [guarantorCheckError, setGuarantorCheckError] = useState<string | null>(null);
    const [isTrustingServers, setIsTrustingServers] = useState(false);
    const [discoveredManifestJson, setDiscoveredManifestJson] = useState<string | null>(null);

    const effectiveManifestJson =
        anchorManifestJson ?? voucherData.anchorManifestJson ?? discoveredManifestJson;




    useEffect(() => {
        let active = true;
        const creatorId = voucherData.voucher?.creator?.id;
        if (!creatorId) return;

        const guarantorIds = (voucherData.voucher.signatures || [])
            .filter(s => s.role && s.role.toLowerCase() === 'guarantor')
            .map(s => s.signerId);

        wotService.evaluateVoucherTrustPreview(creatorId, guarantorIds)
            .then(res => {
                if (active) setPreviewTrust(res);
            })
            .catch(e => logger.error(`Failed to evaluate trust preview: ${stringifyError(e)}`));
        return () => { active = false; };
    }, [voucherData.voucher]);

    useEffect(() => {
        let active = true;
        async function fetchData() {
            try {
                logger.info("SignRequestView opened, fetching standards and roles.");
                const [standards, currentConfig, profile] = await Promise.all([
                    standardsService.getStandards(),
                    settingsService.getConfig().catch(() => null),
                    profileService.getProfile().catch(() => null)
                ]);
                if (!active) return;
                setConfig(currentConfig);
                setUserProfile(profile);
                if (profile?.gender === '1' || profile?.gender === '2') {
                    setSelectedGender(profile.gender);
                }

                const matchingStandard = standards.find((s: VoucherStandardInfo) => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    return uuidMatch && uuidMatch[1] === voucherData.voucher.voucherStandard.uuid;
                });

                if (matchingStandard) {
                    setStandardContent(matchingStandard.content);
                    try {
                        const parsed = await standardsService.parseStandard(matchingStandard.content);
                        if (!active) return;
                        setParsedStandard(parsed);
                        
                        // Extract roles from allowedSignatureRoles
                        const roles = parsed.immutable.issuance?.allowedSignatureRoles || [];
                        setAllowedRoles(roles);
                        if (roles.length === 1) setSelectedRole(roles[0]);
                    } catch (e) {
                        if (!active) return;
                        setFeedbackMsg(t('voucher.signRequest.parseFailed', { error: translateError(e, t) }));
                        setFeedbackType('error');
                    }
                } else {
                    setSelectedRole((prev) => prev || 'guarantor');
                }
            } catch (e) {
                if (!active) return;
                setFeedbackMsg(t('voucher.signRequest.initError', { error: translateError(e, t) }));
                setFeedbackType('error');
            } finally {
                if (active) setStandardsLoaded(true);
            }
        }
        fetchData();
        return () => { active = false; };
    }, [voucherData.voucher.voucherStandard.uuid, t]);

    const requiresGender = useMemo(() => {
        if (!parsedStandard) return false;
        const customRules = parsedStandard.immutable?.customRules || {};
        return Object.values(customRules).some(r => r.expression?.includes('details.gender'));
    }, [parsedStandard]);

    const hasValidGender = userProfile?.gender === '1' || userProfile?.gender === '2';

    const handleSaveGenderToProfile = async () => {
        setIsSavingGender(true);
        try {
            await protectAction(async (pwd) => {
                const updated: PublicProfile = {
                    ...(userProfile || { id: "" }),
                    gender: selectedGender
                };
                await profileService.saveProfile(updated, pwd || undefined);
                setUserProfile(updated);
            }, { requireFullAccess: true });
            if (selectedRole && standardContent) {
                setIsImpactLoading(true);
                standardsService.evaluateSignatureSuitability({
                    voucher: voucherData.voucher,
                    role: selectedRole,
                    standardTomlContent: standardContent
                }).then(setImpact).finally(() => setIsImpactLoading(false));
            }
        } catch (e) {
            logger.error(`Failed to save gender to profile: ${stringifyError(e)}`);
            setFeedbackMsg(translateError(e, t));
            setFeedbackType('error');
        } finally {
            setIsSavingGender(false);
        }
    };

    useEffect(() => {
        let active = true;
        if (!selectedRole || !standardContent) {
            setImpact(null);
            setGuarantorCheck(null);
            setIsGuarantorCheckLoading(false);
            setGuarantorCheckError(null);
            return;
        }

        async function fetchImpact() {
            setIsImpactLoading(true);
            try {
                const impactResult = await standardsService.evaluateSignatureSuitability({
                    voucher: voucherData.voucher,
                    role: selectedRole,
                    standardTomlContent: standardContent as string
                });
                if (!active) return;
                setImpact(impactResult);
            } catch (e) {
                if (!active) return;
                logger.error(`Failed to evaluate impact: ${stringifyError(e)}`);
                setImpact(null);
            } finally {
                if (active) setIsImpactLoading(false);
            }
        }

        fetchImpact();

        if (selectedRole.toLowerCase() === 'guarantor') {
            setGuarantorCheck(null);
            setGuarantorCheckError(null);
            setIsGuarantorCheckLoading(true);
            const runAnchorCheck = async () => {
                let manifestToUse = effectiveManifestJson ?? null;
                if (!manifestToUse && voucherData.voucher.layer2Anchor && typeof settingsService?.discoverAnchorManifest === 'function') {
                    const discovered = await settingsService
                        .discoverAnchorManifest({
                            anchorIdHex: voucherData.voucher.layer2Anchor,
                            creatorDid: voucherData.voucher?.creator?.id ?? null,
                            guarantorDids: [],
                        })
                        .catch(() => null);
                    if (active && discovered?.manifestJson) {
                        manifestToUse = discovered.manifestJson;
                        setDiscoveredManifestJson(discovered.manifestJson);
                    }
                }
                const res = await voucherService.evaluateGuarantorAnchorCheck({
                    voucher: voucherData.voucher,
                    standardTomlContent: standardContent!,
                    manifestJson: manifestToUse,
                });
                if (!active) return;
                if (res && typeof res.required === 'boolean') {
                    setGuarantorCheck(res);
                } else {
                    setGuarantorCheck(null);
                    setGuarantorCheckError(t('voucher.signRequest.guarantorAnchorCheckFailed', 'L2-Anker-Prüfung konnte nicht ausgewertet werden.'));
                }
            };
            runAnchorCheck()
                .catch((err) => {
                    if (active) {
                        setGuarantorCheck(null);
                        setGuarantorCheckError(
                            translateError(err, t) ||
                            t('voucher.signRequest.guarantorAnchorCheckFailed', 'L2-Anker-Prüfung fehlgeschlagen.')
                        );
                    }
                })
                .finally(() => {
                    if (active) setIsGuarantorCheckLoading(false);
                });
        } else {
            setGuarantorCheck(null);
            setIsGuarantorCheckLoading(false);
            setGuarantorCheckError(null);
        }

        return () => { active = false; };
    }, [selectedRole, standardContent, voucherData, effectiveManifestJson, t]);

    const isGuarantorAnchorBlocked =
        selectedRole.toLowerCase() === 'guarantor' &&
        (isGuarantorCheckLoading ||
            Boolean(guarantorCheckError) ||
            (Boolean(standardContent) && !guarantorCheck) ||
            (!standardContent && Boolean(voucherData.voucher.layer2Anchor)) ||
            Boolean(guarantorCheck?.required && !guarantorCheck.passed));

    async function handleSign() {
        if (!selectedRole) {
            setFeedbackMsg(t('voucher.signRequest.pleaseSelectRole'));
            setFeedbackType('error');
            return;
        }

        setIsSigning(true);
        setFeedbackMsg('');
        setFeedbackType(null);
        try {
            const targetCreatorDid = senderDid || voucherData.voucher?.creator?.id;
            const bundleKey = `${voucherData.voucher.voucherId}:${selectedRole}:${includeDetails}`;
            const recipient = senderDid || voucherData.voucher?.creator?.id;
            const needsPostReply = Boolean((senderDid || requestMsgId) && recipient);
            let bundleBytes: number[] | undefined =
                cachedSigBundle?.key === bundleKey ? cachedSigBundle.bytes : undefined;
            let capturedPassword: string | undefined;
            let postDeposited = false;

            if (!bundleBytes || needsPostReply) {
                const outcome = await protectAction(async (password) => {
                    capturedPassword = password || undefined;
                    let signedBytes = bundleBytes;
                    if (!signedBytes) {
                        const generatedBytes = await voucherService.createDetachedSignatureResponseBundle({
                            voucher: voucherData.voucher,
                            role: selectedRole,
                            includeDetails: includeDetails,
                            config: { type: "TargetDid", value: [targetCreatorDid!, "TrialDecryption"] },
                            password: password || undefined,
                            anchorManifestJson: effectiveManifestJson ?? null,
                        });
                        if (Array.isArray(generatedBytes)) {
                            signedBytes = generatedBytes;
                            setCachedSigBundle({ key: bundleKey, bytes: generatedBytes });
                        }
                    }
                    if (!signedBytes) {
                        return { bytes: undefined as number[] | undefined, postDeposited: false };
                    }
                    let delivered = false;
                    if (needsPostReply && recipient) {
                        try {
                            const res = await postService.replyDetachedSignatureViaPost(
                                requestMsgId || undefined,
                                recipient,
                                signedBytes,
                                password || undefined
                            );
                            const deliveredCount = res?.recipientDepositedTo ?? res?.depositedTo ?? 0;
                            if (deliveredCount > 0) {
                                delivered = true;
                            }
                        } catch (err) {
                            logger.warn(`Failed to reply detached signature via post: ${stringifyError(err)}`);
                        }
                    }
                    return { bytes: signedBytes, postDeposited: delivered };
                }, { requireFullAccess: true });

                if (!outcome) return;
                bundleBytes = outcome.bytes;
                postDeposited = outcome.postDeposited;
            }

            if (!bundleBytes) return;
            if (postDeposited) {
                triggerAutoBackup();
                setPostSentSuccess(true);
            }

            // Asymmetry Rule (§6.2): If not deposited to any sync endpoint (Bob has NO writable sync endpoint or network error)
            // OR pure offline file flow, immediately offer the 1-click .sig file save dialog:
            if (!postDeposited) {
                const saveResult = await saveBytesToFile(bundleBytes, {
                    defaultFilename: `signature-${voucherData.voucher.voucherId.slice(0, 8)}.sig`,
                    filters: [
                        { name: t('voucher.signRequest.signatureResponseFilter'), extensions: ['sig'] },
                        { name: t('voucher.export.allFiles'), extensions: ['*'] },
                    ],
                    uiPreferences: config?.uiPreferences,
                    protectAction,
                });

                if (saveResult) {
                    if (saveResult.path) {
                        if (config) {
                            settingsService.getConfig().then(setConfig).catch(() => {});
                        }
                        setSavedSigFilePath(saveResult.path);
                    } else if (saveResult.downloaded) {
                        setFeedbackMsg(t('voucher.signRequest.signatureSavedTitle'));
                        setFeedbackType('success');
                    }
                    if (requestMsgId) {
                        if (capturedPassword !== undefined) {
                            await postService
                                .dismissPendingPostItem('sign_request', requestMsgId, capturedPassword)
                                .catch(() => {});
                        } else {
                            await protectAction(async (pwd) => {
                                await postService.dismissPendingPostItem(
                                    'sign_request',
                                    requestMsgId,
                                    pwd || undefined
                                );
                            }).catch(() => {});
                        }
                    }
                }
            }
        } catch (e) {
            setFeedbackMsg(t('voucher.signRequest.signFailed', { error: translateError(e, t) }));
            setFeedbackType('error');
        } finally {
            setIsSigning(false);
        }
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : t('common.na');

    if (postSentSuccess) {
        return (
            <PageLayout 
                title={t('voucher.signRequest.postSentTitle')} 
                description={t('voucher.signRequest.postSentDesc')} 
                onBack={onBack}
            >
                <div className="max-w-xl mx-auto space-y-6 py-12 text-center animate-in fade-in">
                    <div className="w-20 h-20 bg-emerald-100 text-emerald-600 rounded-3xl mx-auto flex items-center justify-center shadow-premium-sm border border-emerald-200">
                        <CheckCircle2 size={40} />
                    </div>
                    <div className="space-y-2">
                        <h2 className="text-2xl font-black text-theme-primary">
                            {t('voucher.signRequest.postSentTitle')}
                        </h2>
                        <p className="text-sm font-medium text-theme-secondary max-w-md mx-auto leading-relaxed">
                            {t('voucher.signRequest.postSentDesc')}
                        </p>
                    </div>
                    <div className="pt-4">
                        <Button onClick={onBack} className="flex items-center justify-center gap-2 mx-auto">
                            {t('common.done')}
                        </Button>
                    </div>
                </div>
            </PageLayout>
        );
    }

    if (savedSigFilePath) {
        return (
            <PageLayout 
                title={t('voucher.signRequest.signatureSavedTitle')} 
                description={t('voucher.signRequest.signatureSavedDesc')} 
                onBack={onBack}
            >
                <div className="max-w-xl mx-auto space-y-6 py-12 text-center">
                    <div className="w-20 h-20 bg-emerald-100 text-emerald-600 rounded-3xl mx-auto flex items-center justify-center shadow-premium-sm border border-emerald-200">
                        <CheckCircle2 size={40} />
                    </div>
                    <div className="space-y-2">
                        <h2 className="text-2xl font-black text-theme-primary">
                            {t('voucher.signRequest.signatureSavedTitle')}
                        </h2>
                        <p className="text-sm font-medium text-theme-secondary max-w-md mx-auto leading-relaxed">
                            {t('voucher.signRequest.signatureSavedDesc')}
                        </p>
                    </div>
                    <div className="p-4 bg-white/60 border border-theme-subtle rounded-2xl break-all font-mono text-xs text-theme-secondary">
                        {savedSigFilePath}
                    </div>
                    <div className="flex flex-col sm:flex-row gap-3 justify-center pt-4">
                        <Button
                            variant="secondary"
                            onClick={() => revealItemInDir(savedSigFilePath).catch(() => {})}
                            className="flex items-center justify-center gap-2"
                        >
                            <FolderOpen size={18} />
                            {t('transfer.success.revealInFolder')}
                        </Button>
                        <Button
                            onClick={onBack}
                            className="flex items-center justify-center gap-2"
                        >
                            {t('common.done')}
                        </Button>
                    </div>
                </div>
            </PageLayout>
        );
    }

    return (
        <PageLayout 
            title={t('voucher.signRequest.title')} 
            description={t('voucher.signRequest.description')} 
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto space-y-8 pb-10">
                {feedbackMsg && (
                    <AlertBanner variant={feedbackType || undefined}>{feedbackMsg}</AlertBanner>
                )}

                <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                    {/* Voucher Summary */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <FileText size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.detailsTitle')}</span>
                        </div>
                    }>
                        <div className="space-y-4">
                            <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                                <div className="flex items-center justify-between mb-1">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-light">{t('voucher.signRequest.value')}</span>
                                    <div className="bg-emerald-500 text-white px-2 py-0.5 rounded text-[9px] font-black uppercase tracking-widest">{t('voucher.signRequest.validAsset')}</div>
                                </div>
                                <p className="text-2xl font-black text-theme-primary tracking-tight">
                                    {voucherData.voucher.nominalValue.amount} <span className="text-theme-secondary opacity-60 font-medium">{voucherData.displayCurrency}</span>
                                </p>
                            </div>
                            
                            <div className="space-y-3 px-1">
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><ShieldCheck size={14}/> {t('voucher.standardLabel')}</span>
                                    <span className="text-theme-secondary font-bold">{voucherData.displayStandardName}</span>
                                </div>
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><User size={14}/> {t('voucher.creatorHeader')}</span>
                                    <span className="text-theme-secondary font-bold">{voucherData.voucher.creator.firstName} {voucherData.voucher.creator.lastName}</span>
                                </div>
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><Calendar size={14}/> {t('voucher.signRequest.created')}</span>
                                    <span className="text-theme-secondary font-bold">{formatDateTime(voucherData.voucher.creationDate)}</span>
                                </div>
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><ShieldCheck size={14}/> {t('voucher.signRequest.requesterTrustLabel')}</span>
                                    <TrustBadge trust={previewTrust} size="sm" />
                                </div>
                            </div>
                        </div>
                    </Card>

                    {/* Role Selection */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <UserCheck size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.signRequest.signatureRole')}</span>
                        </div>
                    }>
                        <div className="space-y-6">
                            <div className="space-y-3">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.signRequest.selectRole')}</label>
                                {!standardsLoaded ? (
                                    <div className="py-2 animate-pulse flex items-center gap-2">
                                        <div className="h-4 w-4 bg-theme-subtle rounded-full"></div>
                                        <div className="h-4 w-32 bg-theme-subtle rounded-full"></div>
                                    </div>
                                ) : allowedRoles.length === 0 ? (
                                    <div className="space-y-3">
                                        <p className="text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded-xl p-3 leading-relaxed">
                                            {t('voucher.unknownStandardRoleHint')}
                                        </p>
                                        <select
                                            value={selectedRole || 'guarantor'}
                                            onChange={(e) => setSelectedRole(e.target.value)}
                                            className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                        >
                                            <option value="guarantor">
                                                {t('voucher.role.guarantor', { defaultValue: 'guarantor' })}
                                            </option>
                                        </select>
                                    </div>
                                ) : allowedRoles.length === 1 ? (
                                    <div className="p-4 bg-white border-2 border-theme-primary rounded-2xl flex items-center justify-between shadow-sm">
                                        <span className="font-black text-theme-primary tracking-tight">
                                            {t(`voucher.role.${selectedRole}`, { defaultValue: selectedRole })}
                                        </span>
                                        <CheckCircle2 size={18} className="text-theme-primary" />
                                    </div>
                                ) : (
                                    <select
                                        value={selectedRole}
                                        onChange={(e) => setSelectedRole(e.target.value)}
                                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                    >
                                        <option value="">{t('voucher.signRequest.chooseRole')}</option>
                                        {allowedRoles.map(role => (
                                            <option key={role} value={role}>
                                                {t(`voucher.role.${role}`, { defaultValue: role })}
                                            </option>
                                        ))}
                                    </select>
                                )}
                            </div>

                            <label className="flex items-center gap-4 p-4 bg-white/40 border border-theme-subtle rounded-2xl cursor-pointer group hover:border-theme-primary/40 transition-all shadow-sm">
                                <input
                                    type="checkbox"
                                    checked={includeDetails}
                                    onChange={(e) => setIncludeDetails(e.target.checked)}
                                    className="w-6 h-6 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
                                />
                                <div>
                                    <span className="block text-sm font-bold text-theme-secondary group-hover:text-theme-primary transition-colors">{t('voucher.signRequest.discloseIdentity')}</span>
                                    <span className="block text-[10px] text-theme-light font-medium">{t('voucher.signRequest.embedProfileData')}</span>
                                </div>
                            </label>

                            {/* Schutz 1: Wenn requiresGender && !includeDetails */}
                            {requiresGender && !includeDetails && (
                                <div className="p-4 bg-amber-50 border border-amber-200 rounded-2xl flex items-start gap-3">
                                    <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                                    <p className="text-xs font-bold text-amber-800 leading-relaxed">
                                        {t('voucher.signRequest.warnDetailsRequiredForGender')}
                                    </p>
                                </div>
                            )}

                            {/* Schutz 2: Wenn requiresGender && includeDetails && !hasValidGender */}
                            {requiresGender && includeDetails && !hasValidGender && (
                                <div className="p-4 bg-amber-50 border border-amber-200 rounded-2xl space-y-3">
                                    <div className="flex items-start gap-3">
                                        <AlertCircle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                                        <div>
                                            <p className="text-xs font-black uppercase tracking-wider text-amber-900">
                                                {t('voucher.signRequest.genderRequiredTitle')}
                                            </p>
                                            <p className="text-xs font-medium text-amber-800 mt-0.5">
                                                {t('voucher.signRequest.genderRequiredDesc')}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="flex items-center gap-3 pt-1">
                                        <select
                                            value={selectedGender}
                                            onChange={(e) => setSelectedGender(e.target.value)}
                                            className="flex-1 bg-white border border-amber-300 rounded-xl px-3 py-2 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-amber-400 outline-none"
                                        >
                                            <option value="1">{t('profile.genderMale', { defaultValue: 'Männlich' })}</option>
                                            <option value="2">{t('profile.genderFemale', { defaultValue: 'Weiblich' })}</option>
                                        </select>
                                        <Button
                                            size="sm"
                                            onClick={handleSaveGenderToProfile}
                                            disabled={isSavingGender}
                                            className="text-xs shrink-0"
                                        >
                                            {isSavingGender ? t('common.loading') : t('voucher.signRequest.saveGenderAndVerify')}
                                        </Button>
                                    </div>
                                </div>
                            )}

                            {selectedRole.toLowerCase() === 'guarantor' && (
                                <div className="p-4 bg-amber-50/70 border border-amber-200 rounded-2xl flex items-start gap-3">
                                    <Info size={18} className="text-amber-700 shrink-0 mt-0.5" />
                                    <p className="text-xs font-medium text-amber-900 leading-relaxed">
                                        {t('voucher.signRequest.guarantorHumanHint').replace(/guarantor/gi, (m) => m[0] + '\u200B' + m.slice(1))}
                                    </p>
                                </div>
                            )}

                            {selectedRole.toLowerCase() === 'guarantor' && isGuarantorCheckLoading && (
                                <div
                                    data-testid="guarantor-anchor-check-loading"
                                    className="p-4 rounded-2xl border border-theme-subtle bg-theme-subtle/10 flex items-center gap-2.5 text-xs font-semibold text-theme-secondary"
                                >
                                    <ShieldCheck size={16} className="text-theme-primary animate-pulse shrink-0" />
                                    <span>{t('voucher.signRequest.guarantorAnchorChecking', 'L2-Anker-Prüfung läuft…')}</span>
                                </div>
                            )}

                            {selectedRole.toLowerCase() === 'guarantor' && !isGuarantorCheckLoading && guarantorCheckError && (
                                <div
                                    data-testid="guarantor-anchor-check-error"
                                    className="p-4 rounded-2xl border border-rose-200 bg-rose-50 text-rose-900 flex items-start gap-2.5 text-xs font-semibold"
                                >
                                    <ShieldAlert size={16} className="text-rose-600 shrink-0 mt-0.5" />
                                    <span>{guarantorCheckError}</span>
                                </div>
                            )}

                            {selectedRole.toLowerCase() === 'guarantor' && guarantorCheck?.required && (
                                <div
                                    data-testid="guarantor-anchor-check"
                                    className={`p-4 rounded-2xl border space-y-2 ${
                                        guarantorCheck.passed
                                            ? 'bg-emerald-50 border-emerald-200 text-emerald-900'
                                            : 'bg-rose-50 border-rose-200 text-rose-900'
                                    }`}
                                >
                                    <div className="flex items-center justify-between gap-2">
                                        <span className="text-xs font-black uppercase tracking-wider flex items-center gap-1.5">
                                            {guarantorCheck.passed ? (
                                                <ShieldCheck size={16} className="text-emerald-600" />
                                            ) : (
                                                <ShieldAlert size={16} className="text-rose-600" />
                                            )}
                                            {t('voucher.signRequest.guarantorAnchorTitle', 'L2 Anchor Verification')}
                                        </span>
                                        <span className="text-[11px] font-mono font-bold">
                                            Sybil: {guarantorCheck.sybilIndependentCount}/{guarantorCheck.threshold}
                                        </span>
                                    </div>
                                    <div className="flex flex-wrap gap-2 text-[11px]">
                                        <span>Anchor: {guarantorCheck.hasLayer2Anchor ? '✓' : '✕'}</span>
                                        {guarantorCheck.retentionOk !== null && guarantorCheck.retentionOk !== undefined && (
                                            <span>Retention: {guarantorCheck.retentionOk ? '✓' : '✕'}</span>
                                        )}
                                        <span>
                                            {t('voucher.signRequest.serversCount', '{{count}} Services', {
                                                count: guarantorCheck.serverCount,
                                            })}
                                        </span>
                                    </div>
                                    {guarantorCheck.passed && guarantorCheck.untrustedServersCount > 0 && (
                                        <div className="mt-2 p-3 rounded-xl bg-amber-50 border border-amber-200 text-amber-900 space-y-2">
                                            <p className="text-[11px] font-semibold">
                                                {t(
                                                    'voucher.signRequest.untrustedServersHint',
                                                    '{{count}} protection services in this voucher are not yet marked as trusted for Sync-Locks.',
                                                    { count: guarantorCheck.untrustedServersCount }
                                                )}
                                            </p>
                                            <ul className="text-[10px] font-mono space-y-0.5 text-amber-800">
                                                {guarantorCheck.servers
                                                    .filter((s) => !s.trusted)
                                                    .map((s) => (
                                                        <li key={s.lockPub} className="truncate">
                                                            • {s.writeUrl}
                                                        </li>
                                                    ))}
                                            </ul>
                                            <Button
                                                type="button"
                                                size="xs"
                                                variant="primary"
                                                disabled={isTrustingServers || isSigning}
                                                onClick={async () => {
                                                    const untrusted = guarantorCheck.servers.filter((s) => !s.trusted);
                                                    if (untrusted.length === 0) return;
                                                    setIsTrustingServers(true);
                                                    const succeededAdminPubs = new Set<string>();
                                                    try {
                                                        const trustedAll = await protectAction(
                                                            async (password) => {
                                                                for (const s of untrusted) {
                                                                    await settingsService.setLockServerTrust(
                                                                        s.adminPub,
                                                                        true,
                                                                        password ?? null
                                                                    );
                                                                    succeededAdminPubs.add(s.adminPub);
                                                                }
                                                                return true;
                                                            },
                                                            { requireFullAccess: true }
                                                        );
                                                        if (trustedAll) {
                                                            await handleSign();
                                                        }
                                                    } catch (e) {
                                                        setFeedbackMsg(stringifyError(e));
                                                        setFeedbackType('error');
                                                    } finally {
                                                        if (succeededAdminPubs.size > 0) {
                                                            setGuarantorCheck((prev) => {
                                                                if (!prev) return prev;
                                                                const updatedServers = prev.servers.map((s) =>
                                                                    succeededAdminPubs.has(s.adminPub)
                                                                        ? { ...s, trusted: true }
                                                                        : s
                                                                );
                                                                return {
                                                                    ...prev,
                                                                    untrustedServersCount: updatedServers.filter((s) => !s.trusted).length,
                                                                    servers: updatedServers,
                                                                };
                                                            });
                                                        }
                                                        setIsTrustingServers(false);
                                                    }
                                                }}
                                            >
                                                {t(
                                                    'voucher.signRequest.trustAndSignBtn',
                                                    'Trust these services & sign'
                                                )}
                                            </Button>
                                        </div>
                                    )}
                                    {guarantorCheck.issues.length > 0 && (
                                        <ul className="list-disc list-inside text-[11px] font-semibold space-y-0.5">
                                            {guarantorCheck.issues.map((issue) => (
                                                <li key={issue}>{t(`voucher.signRequest.anchorIssue.${issue}`, issue)}</li>
                                            ))}
                                        </ul>
                                    )}
                                </div>
                            )}
                        </div>
                    </Card>
                </div>

                {/* Impact Analysis */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <PenTool size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.signRequest.impactEvaluation')}</span>
                    </div>
                }>
                    <div className="min-h-[120px] flex flex-col justify-center">
                        {isImpactLoading ? (
                            <div className="flex flex-col items-center gap-3 animate-pulse">
                                <div className="w-10 h-10 bg-theme-subtle/20 rounded-full border border-theme-subtle/30 flex items-center justify-center">
                                    <ArrowRight className="text-theme-light animate-bounce" size={20} />
                                </div>
                                <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">{t('voucher.signRequest.checkingValidity')}</p>
                            </div>
                        ) : impact ? (
                            <div className="space-y-4">
                                {!impact.isAllowedRole && (
                                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-start gap-3">
                                        <XCircle size={20} className="text-rose-500 shrink-0" />
                                        <p className="text-sm font-bold text-rose-800">{t('voucher.signRequest.standardViolation')}</p>
                                    </div>
                                )}
                                
                                {impact.fatalConflicts.length > 0 && (
                                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl space-y-2">
                                        <div className="flex items-center gap-2 text-rose-600 font-black text-[10px] uppercase tracking-widest">
                                            <ShieldAlert size={14} /> {t('voucher.signRequest.profileConflicts')}
                                        </div>
                                        <ul className="space-y-1.5">
                                            {impact.fatalConflicts.map((conflict, i) => (
                                                <li key={i} className="text-sm font-bold text-rose-800 flex items-start gap-2">
                                                    <span className="text-rose-400 mt-1.5 flex-shrink-0 w-1 h-1 bg-rose-400 rounded-full"></span>
                                                    {translateRuleMessage(conflict, t)}
                                                </li>
                                            ))}
                                        </ul>
                                    </div>
                                )}

                                {impact.resolvedRules.length > 0 && (
                                    <div className="p-4 bg-emerald-50 border border-emerald-100 rounded-2xl space-y-2">
                                        <div className="flex items-center gap-2 text-emerald-600 font-black text-[10px] uppercase tracking-widest">
                                            <CheckCircle2 size={14} /> {t('voucher.signRequest.rulesFulfilled')}
                                        </div>
                                        <ul className="space-y-1.5">
                                            {impact.resolvedRules.map((rule, i) => (
                                                <li key={i} className="text-sm font-bold text-emerald-800 flex items-start gap-2">
                                                    <span className="text-emerald-400 mt-1.5 flex-shrink-0 w-1 h-1 bg-emerald-400 rounded-full"></span>
                                                    {translateRuleMessage(rule, t)}
                                                </li>
                                            ))}
                                        </ul>
                                    </div>
                                )}

                                {impact.gentleHints.length > 0 && impact.fatalConflicts.length === 0 && (
                                    <div className="space-y-2">
                                        {impact.gentleHints.map((hint, i) => (
                                            <div key={i} className="p-4 bg-blue-50/50 border border-blue-100 rounded-2xl flex items-start gap-3">
                                                <Lightbulb size={20} className="text-blue-500 shrink-0" />
                                                <p className="text-sm font-bold text-blue-800">{translateGentleHint(hint, t)}</p>
                                            </div>
                                        ))}
                                    </div>
                                )}

                                {impact.isAllowedRole && impact.fatalConflicts.length === 0 && impact.resolvedRules.length === 0 && impact.gentleHints.length === 0 && (
                                    <div className="p-4 bg-gray-50 border border-gray-100 rounded-2xl flex items-start gap-3">
                                        <Info size={20} className="text-gray-400 shrink-0" />
                                        <p className="text-sm font-bold text-gray-600">{t('voucher.signRequest.signatureValid')}</p>
                                    </div>
                                )}
                            </div>
                        ) : selectedRole ? (
                            <div className="text-center py-4">
                                <p className="text-sm font-bold text-theme-light italic">{t('voucher.signRequest.impactUnavailable')}</p>
                            </div>
                        ) : (
                            <div className="text-center py-4">
                                <p className="text-sm font-bold text-theme-light/60 italic">{t('voucher.signRequest.selectRoleForImpact')}</p>
                            </div>
                        )}
                    </div>
                </Card>

                <div className="flex flex-col gap-4">
                    <Button
                        size="lg"
                        onClick={handleSign}
                        disabled={
                            !selectedRole ||
                            isSigning ||
                            isGuarantorAnchorBlocked ||
                            (impact !== null && (!impact.isAllowedRole || impact.fatalConflicts.length > 0))
                        }
                        className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3 disabled:opacity-30 disabled:grayscale transition-all"
                    >
                        {isSigning ? <ShieldCheck className="animate-pulse" size={24} /> : <PenTool size={24} />}
                        {isSigning ? t('voucher.signRequest.signing') : t('voucher.signRequest.signNow')}
                    </Button>
                    <p className="text-[10px] font-bold text-theme-light text-center flex items-center justify-center gap-2">
                        <Lock size={12} />
                        {t('voucher.signRequest.localSignatureNotice')}
                    </p>
                </div>
            </div>
        </PageLayout>
    );
}
