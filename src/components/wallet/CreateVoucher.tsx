import { useState, useEffect, FormEvent, useRef } from "react";
import { useTranslation } from "react-i18next";
import { voucherService } from "../../services/voucherService";
import { Button } from "../ui/Button";
import { useSession } from "../../context/SessionContext";
import { protectCommandOptions } from "../../services/commandPolicy";
import { ConfirmationModal } from "../ui/ConfirmationModal";
import { ModalShell } from "../ui/ModalShell";
import { isBackendError, translateError } from "../../utils/errorHelper";
import { PageLayout } from "../ui/PageLayout";
import { AlertBanner } from "../ui/AlertBanner";
import { useOptionalNavigation } from "../../context/NavigationContext";
import { 
    ShieldAlert, 
    Info, 
    Clock,
    AlertCircle,
    Lock, 
    PlusCircle,
    WifiOff
} from "lucide-react";

import type { VoucherDetails, ExpectedL2Committee } from "../../types";
import { useVoucherCreation, standardRequiresCreatorGender } from "../../hooks/useVoucherCreation";
import { VoucherBasicsForm } from "../voucher/VoucherBasicsForm";
import { CreatorIdentityForm } from "../voucher/CreatorIdentityForm";

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const navigation = useOptionalNavigation();
    const {
        standards,
        selectedStandardId,
        parsedStandard,
        isParsingStandard,
        amount,
        validityValue,
        validityUnit,
        nonRedeemable,
        allowOfflineOnly,
        identity,
        isLoading,
        coordWarning,
        errors,
        lockOverview,
        refreshLockOverview,
        setAmount,
        setValidityValue,
        setValidityUnit,
        setNonRedeemable,
        setAllowOfflineOnly,
        setIdentity,
        handleStandardChange,
        handleLoadProfile,
        handleCoordBlur,
        validate,
        prepareVoucherData,
        setIsLoading
    } = useVoucherCreation();

    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);
    const [showConfirm, setShowConfirm] = useState(false);
    const [showTestVoucherWarning, setShowTestVoucherWarning] = useState(false);
    const [showL2UnavailableModal, setShowL2UnavailableModal] = useState(false);
    const [l2CommitteeChangedInfo, setL2CommitteeChangedInfo] = useState<{
        expectedThreshold: string;
        expectedCount: string;
        actualThreshold: string;
        actualCount: string;
        actualCommittee: ExpectedL2Committee;
    } | null>(null);

    const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        return () => {
            if (timerRef.current) {
                clearTimeout(timerRef.current);
            }
        };
    }, []);

    const fieldLabels: Record<string, string> = {
        standard: t('voucher.create.voucherType'),
        amount: t('voucher.create.amount'),
        firstName: t('voucher.create.firstName'),
        lastName: t('voucher.create.lastName'),
        gender: t('profile.gender'),
        offlineOnlyConfirm: t('voucher.basics.l2ConfirmOfflineOnlyShort'),
    };

    // Helper to generate missing field names string
    const getMissingFieldNames = (errors: Record<string, boolean>): string => {
        return Object.keys(errors)
            .filter(key => errors[key])
            .map(key => fieldLabels[key] || key)
            .join(", ");
    };

    // Refs für Fokus-Management
    const standardRef = useRef<HTMLSelectElement>(null);
    const amountRef = useRef<HTMLInputElement>(null);
    const firstNameRef = useRef<HTMLInputElement>(null);
    const lastNameRef = useRef<HTMLInputElement>(null);
    const genderRef = useRef<HTMLSelectElement>(null);
    const offlineOnlyConfirmRef = useRef<HTMLInputElement>(null);

    const requiresGender = standardRequiresCreatorGender(parsedStandard);

    const handleCreateClick = (event: FormEvent) => {
        event.preventDefault();
        if (isLoading || isParsingStandard) return;
        
        const currentErrors = validate();
        if (Object.keys(currentErrors).length > 0) {
            // Fokus auf das erste fehlerhafte Feld setzen und dorthin scrollen
            if (currentErrors.standard) {
                standardRef.current?.focus();
                standardRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.amount) {
                amountRef.current?.focus();
                amountRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.offlineOnlyConfirm) {
                offlineOnlyConfirmRef.current?.focus();
                offlineOnlyConfirmRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.firstName) {
                firstNameRef.current?.focus();
                firstNameRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.lastName) {
                lastNameRef.current?.focus();
                lastNameRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.gender) {
                genderRef.current?.focus();
                genderRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            }

            return;
        }

        setShowConfirm(true);
    };

    async function executeCreation(
        forceAllowOfflineOnly = false,
        forceReprobe = false,
        expectedCommitteeOverride?: ExpectedL2Committee | null,
    ) {
        const selectedStandard = standards.find(s => s.id === selectedStandardId);
        if (!selectedStandard || isParsingStandard) {
            return;
        }
        setIsLoading(true);
        setShowL2UnavailableModal(false);
        setL2CommitteeChangedInfo(null);
        const voucherData = prepareVoucherData(
            forceAllowOfflineOnly ? true : undefined,
            {
                forceReprobe,
                explicitOfflineOnly: forceAllowOfflineOnly ? true : allowOfflineOnly,
                expectedCommitteeOverride,
            },
        );
        
        try {
            let created: VoucherDetails | undefined;
            await protectAction(async (password) => {
                created = await voucherService.create(selectedStandard.content, voucherData, password || undefined);
            }, protectCommandOptions('create_new_voucher'));
            setFeedback({type: 'success', msg: t('voucher.create.success')});
            timerRef.current = setTimeout(() => {
                if (created && String(created.status).toLowerCase() === 'incomplete' && navigation) {
                    navigation.navigate({ view: 'voucher_details', voucherId: created.localInstanceId });
                } else {
                    onVoucherCreated();
                }
            }, 800);
        } catch (e) {
            if (!forceAllowOfflineOnly && isBackendError(e) && e.code === 'error.voucher.l2AnchorUnavailable') {
                setShowConfirm(false);
                setShowL2UnavailableModal(true);
            } else if (!forceAllowOfflineOnly && isBackendError(e) && e.code === 'error.voucher.l2CommitteeChanged') {
                setShowConfirm(false);
                void refreshLockOverview();
                const details = e.details ?? {};
                const actualPubs = String(details.actualLockPubs ?? '')
                    .split(',')
                    .map((s) => s.trim())
                    .filter(Boolean);
                const actualThreshold = Math.max(1, parseInt(String(details.actualThreshold ?? '1'), 10) || 1);
                const actualSource = String(details.actualSource ?? 'live');
                setL2CommitteeChangedInfo({
                    expectedThreshold: String(details.expectedThreshold ?? '0'),
                    expectedCount: String(details.expectedCount ?? '0'),
                    actualThreshold: String(actualThreshold),
                    actualCount: String(details.actualCount ?? actualPubs.length),
                    actualCommittee: {
                        lockPubs: actualPubs,
                        threshold: actualThreshold,
                        source: actualSource,
                    },
                });
            } else {
                setFeedback({type: 'error', msg: t('voucher.create.failed', { error: translateError(e, t) })});
                setShowConfirm(false);
            }
        } finally {
            setIsLoading(false);
        }
    }

    return (
        <PageLayout 
            title={t('voucher.create')} 
            description={t('voucher.create.description')} 
            onBack={onCancel}
        >
            <div className="max-w-5xl mx-auto space-y-8 pb-20">
                {feedback && (
                    <AlertBanner variant={feedback.type}>{feedback.msg}</AlertBanner>
                )}

                <form onSubmit={handleCreateClick} className="space-y-8">
                    <VoucherBasicsForm 
                        standards={standards}
                        selectedStandardId={selectedStandardId}
                        onStandardChange={handleStandardChange}
                        amount={amount}
                        onAmountChange={setAmount}
                        validityValue={validityValue}
                        onValidityValueChange={setValidityValue}
                        validityUnit={validityUnit}
                        onValidityUnitChange={setValidityUnit}
                        nonRedeemable={nonRedeemable}
                        onNonRedeemableChange={(val) => {
                            if (val) setNonRedeemable(true);
                            else setShowTestVoucherWarning(true);
                        }}
                        allowOfflineOnly={allowOfflineOnly}
                        onAllowOfflineOnlyChange={setAllowOfflineOnly}
                        offlineOnlyConfirmRef={offlineOnlyConfirmRef}
                        isLoading={isLoading}
                        errors={errors}
                        standardRef={standardRef}
                        amountRef={amountRef}
                        parsedStandard={parsedStandard}
                        onManageStandards={navigation ? () => navigation.navigateToSettingsSection('standards') : undefined}
                        lockOverview={lockOverview}
                    />

                    <CreatorIdentityForm 
                        identity={identity}
                        onIdentityChange={setIdentity}
                        onLoadProfile={handleLoadProfile}
                        onCoordBlur={handleCoordBlur}
                        coordWarning={coordWarning}
                        errors={errors}
                        firstNameRef={firstNameRef}
                        lastNameRef={lastNameRef}
                        requiresGender={requiresGender}
                        genderRef={genderRef}
                    />

                    <AlertBanner 
                        variant="warning" 
                        icon={<Info size={20} className="text-amber-600" />} 
                        title={t('voucher.create.publicDataNotice')}
                    >
                        {t('voucher.create.publicDataNoticeDesc')}
                    </AlertBanner>

                    <div className="flex flex-col items-center gap-4 py-4">
                        {Object.values(errors).some(v => v) && (
                            <div className="flex items-center gap-2 text-rose-600 bg-rose-50 px-4 py-2 rounded-xl border border-rose-100 animate-in fade-in slide-in-from-bottom-2 duration-300">
                                <AlertCircle size={16} />
                                <span className="text-xs font-bold">{t('voucher.create.missingPrefix')}{getMissingFieldNames(errors)}</span>
                            </div>
                        )}
                        <Button type="submit" disabled={isLoading} className="w-full sm:w-auto sm:min-w-[320px] py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            {isLoading ? <Clock className="animate-spin" size={24} /> : <PlusCircle size={24} />}
                            {isLoading ? t('voucher.create.creating') : t('voucher.create')}
                        </Button>
                            <p className="text-[10px] font-bold text-theme-light flex items-center gap-2">
                                <Lock size={12} /> {t('voucher.create.nextStepSign')}
                            </p>
                    </div>
                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title={t('voucher.create.confirmTitle')}
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-6 bg-theme-primary/5 rounded-[32px] border border-theme-primary/20 text-center">
                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light mb-2">{t('voucher.create.confirmNewLabel')}</p>
                            <p className="text-3xl font-black text-theme-primary tracking-tighter">
                                {(() => {
                                    if (!parsedStandard) return amount;
                                    const abbreviation = parsedStandard.immutable.identity.abbreviation;
                                    const unit = parsedStandard.immutable.blueprint.unit;
                                    let displayUnit = abbreviation || unit || t('voucher.create.units');
                                    if (nonRedeemable && !displayUnit.startsWith("TEST-")) displayUnit = `TEST-${displayUnit}`;
                                    return `${amount} ${displayUnit}`;
                                })()}
                            </p>
                        </div>
                        <p className="text-sm font-medium text-theme-secondary leading-relaxed">
                            {t('voucher.create.confirmDescription')}
                        </p>
                    </div>
                }
                confirmText={t('voucher.create')}
                onConfirm={() => void executeCreation(false)}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isLoading}
            />

            <ModalShell
                isOpen={showL2UnavailableModal}
                onClose={isLoading ? undefined : () => setShowL2UnavailableModal(false)}
                closeOnBackdrop={false}
                maxWidth="md"
                title={
                    <h2 className="text-xl font-bold text-theme-primary mb-2">
                        {t('voucher.create.l2UnavailableTitle')}
                    </h2>
                }
                description={
                    <div className="space-y-3 pt-1">
                        <div className="p-4 bg-amber-50 border border-amber-200 rounded-2xl flex items-start gap-3">
                            <WifiOff size={20} className="text-amber-600 shrink-0 mt-0.5" />
                            <p className="text-sm font-medium text-amber-900 leading-relaxed">
                                {t('voucher.create.l2UnavailableDesc')}
                            </p>
                        </div>
                    </div>
                }
                footer={
                    <div className="flex flex-col sm:flex-row gap-2">
                        <Button
                            variant="secondary"
                            onClick={() => setShowL2UnavailableModal(false)}
                            disabled={isLoading}
                            className="sm:flex-1"
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            variant="primary"
                            data-testid="l2-unavailable-retry-btn"
                            onClick={() => void executeCreation(false, true)}
                            disabled={isLoading}
                            className="sm:flex-1"
                        >
                            {t('common.retry')}
                        </Button>
                        <Button
                            variant="danger"
                            data-testid="l2-unavailable-offline-btn"
                            onClick={() => void executeCreation(true, false)}
                            disabled={isLoading}
                            className="sm:flex-1"
                        >
                            {t('voucher.create.issueOfflineOnly')}
                        </Button>
                    </div>
                }
            />

            <ConfirmationModal
                isOpen={l2CommitteeChangedInfo !== null}
                title={t('voucher.create.l2CommitteeChangedTitle')}
                description={
                    l2CommitteeChangedInfo ? (
                        <p className="text-sm font-medium text-theme-secondary leading-relaxed">
                            {t('voucher.create.l2CommitteeChangedDesc', {
                                expectedThreshold: l2CommitteeChangedInfo.expectedThreshold,
                                expectedCount: l2CommitteeChangedInfo.expectedCount,
                                actualThreshold: l2CommitteeChangedInfo.actualThreshold,
                                actualCount: l2CommitteeChangedInfo.actualCount,
                            })}
                        </p>
                    ) : null
                }
                confirmText={t('voucher.create.l2CommitteeChangedAccept')}
                cancelText={t('common.cancel')}
                isProcessing={isLoading}
                onConfirm={() => {
                    if (!l2CommitteeChangedInfo) return;
                    void executeCreation(false, false, l2CommitteeChangedInfo.actualCommittee);
                }}
                onCancel={() => setL2CommitteeChangedInfo(null)}
            />

            <ConfirmationModal
                isOpen={showTestVoucherWarning}
                title={t('common.warning')}
                confirmVariant="danger"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-5 bg-rose-50 border border-rose-100 rounded-3xl flex items-start gap-4">
                            <ShieldAlert size={24} className="text-rose-500 mt-1 shrink-0" />
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                {t('voucher.create.testWarningDesc')}
                            </p>
                        </div>
                        <p className="text-sm font-bold text-theme-secondary">{t('voucher.create.testWarningProceed')}</p>
                    </div>
                }
                confirmText={t('voucher.create.createRealVoucher')}
                cancelText={t('voucher.create.backToSafety')}
                onConfirm={() => { setNonRedeemable(false); setShowTestVoucherWarning(false); }}
                onCancel={() => { setNonRedeemable(true); setShowTestVoucherWarning(false); }}
            />
        </PageLayout>
    );
}
