// src/components/wallet/VoucherDetailsView.tsx
import { useTranslation } from 'react-i18next';
import {
    Shield,
    History,
    FileSignature,
    AlertCircle,
    AlertTriangle,
    Info,
    CheckCircle2,
    Clock,
    ShieldAlert,
} from 'lucide-react';
import type { VoucherStatus } from '../../types';
import { useNavigation } from '../../context/NavigationContext';
import { getMissingProfileHint } from '../../utils/signatureHints';
import { isExactSameUserId, isSameRootIdentity } from '../../utils/userIdHelper';
import { Card } from '../ui/Card';
import { TrustBadge } from '../ui/TrustBadge';
import { HelpIcon } from '../ui/HelpIcon';
import { PageLayout } from '../ui/PageLayout';
import { PageLoadingState, PageErrorState } from '../ui/PageState';
import { AlertBanner } from '../ui/AlertBanner';
import { InfoRow } from '../ui/InfoRow';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { CreatorSection } from '../voucher/CreatorSection';
import { PolicySection } from '../voucher/PolicySection';
import { TimelineSection } from '../voucher/TimelineSection';
import { GuarantorClaimPanel } from './GuarantorClaimPanel';
import {
    VoucherActionsPanel,
    VoucherHeaderActions,
    type VoucherStatusInfo,
} from './VoucherActionsPanel';
import { useVoucherDetails } from './useVoucherDetails';
import { VoucherDetailsModals } from './VoucherDetailsModals';

function getStatusInfo(status: VoucherStatus, t: (key: string) => string): VoucherStatusInfo {
    const statusKey = typeof status === 'string' ? status : Object.keys(status)[0] || 'unknown';
    switch (statusKey) {
        case 'active':
            return { id: 'active', label: t('voucher.statusActive'), color: 'text-emerald-600', bgColor: 'bg-emerald-50', icon: CheckCircle2 };
        case 'archived':
            return { id: 'archived', label: t('voucher.statusArchived'), color: 'text-gray-600', bgColor: 'bg-gray-50', icon: History };
        case 'incomplete':
            return { id: 'incomplete', label: t('voucher.statusIncomplete'), color: 'text-amber-600', bgColor: 'bg-amber-50', icon: Clock };
        case 'quarantined':
            return { id: 'quarantined', label: t('voucher.statusQuarantined'), color: 'text-rose-600', bgColor: 'bg-rose-50', icon: ShieldAlert };
        case 'endorsed':
            return { id: 'endorsed', label: t('voucher.statusEndorsed'), color: 'text-indigo-600', bgColor: 'bg-indigo-50', icon: FileSignature };
        case 'expired':
            return { id: 'expired', label: t('voucher.statusExpired'), color: 'text-orange-600', bgColor: 'bg-orange-50', icon: Clock };
        default:
            return { id: 'unknown', label: t('voucher.statusUnknown'), color: 'text-gray-600', bgColor: 'bg-gray-50', icon: Info };
    }
}

export function VoucherDetailsView() {
    const { t } = useTranslation();
    const { appState } = useNavigation();
    const voucherId = appState.view === 'voucher_details' ? appState.voucherId : '';
    const state = useVoucherDetails(voucherId);

    if (state.isLoading) {
        return (
            <PageLayout title={t('voucher.detailsTitle')} onBack={state.goBack}>
                <PageLoadingState message={t('common.loading')} />
            </PageLayout>
        );
    }

    if (state.errorMsg || !state.details || !state.details.voucher) {
        return (
            <PageLayout title={t('voucher.detailsTitle')} onBack={state.goBack}>
                <PageErrorState message={state.errorMsg || t('voucher.notFound')} />
            </PageLayout>
        );
    }

    const { details } = state;
    const voucher = details.voucher;
    const statusInfo = getStatusInfo(details.status, t);
    const targetUuid = voucher.voucherStandard.uuid;
    const matchedStandard =
        state.parsedStandards[targetUuid] ||
        Object.values(state.parsedStandards).find((s) => s.immutable.identity.uuid === targetUuid);

    const currentLang = state.i18n.language ? state.i18n.language.split('-')[0] : 'de';
    const descs = matchedStandard?.mutable?.i18n?.descriptions || {};
    const standardDescription =
        descs[currentLang] || descs['de'] || descs['en'] || Object.values(descs)[0] || '';
    const footnotes = matchedStandard?.mutable?.i18n?.footnotes || {};
    const standardFootnote =
        footnotes[currentLang] || footnotes['de'] || footnotes['en'] || Object.values(footnotes)[0] || '';
    const allowPartialTransfers = matchedStandard?.immutable?.features?.allowPartialTransfers ?? false;

    const signatureHint =
        statusInfo.id === 'incomplete' && state.userProfile && matchedStandard
            ? getMissingProfileHint(matchedStandard, state.userProfile, t)
            : null;

    const guarantorProgress = (() => {
        if (statusInfo.id !== 'incomplete') return null;
        const minRequired = matchedStandard?.immutable?.issuance?.additionalSignaturesRange?.[0] ?? 0;
        const customRules = matchedStandard?.immutable?.customRules || {};
        const hasMale = Object.values(customRules).some((r) => r.expression?.includes("gender == '1'"));
        const hasFemale = Object.values(customRules).some((r) => r.expression?.includes("gender == '2'"));
        const nonCreatorSigs = (voucher.signatures || []).filter((s) => s.role !== 'creator');
        if (hasMale && hasFemale && minRequired === 2) {
            const m = nonCreatorSigs.filter((s) => s.details?.gender === '1').length;
            const f = nonCreatorSigs.filter((s) => s.details?.gender === '2').length;
            if (m >= 1 && f >= 1) return t('voucher.guarantorStatusBothPresent');
            if (m >= 1) return t('voucher.guarantorStatusMaleDoneFemaleMissing');
            if (f >= 1) return t('voucher.guarantorStatusFemaleDoneMaleMissing');
            return t('voucher.guarantorStatusBothMissing');
        }
        return minRequired > 0
            ? t('voucher.guarantorStatusCount', { current: nonCreatorSigs.length, required: minRequired })
            : null;
    })();

    const isOwnVoucher = Boolean(
        voucher.creator.id && state.ownUserId && voucher.creator.id === state.ownUserId,
    );
    const canRedeemAtIssuer = statusInfo.id === 'active' && !isOwnVoucher && Boolean(voucher.creator.id);
    const guarantorSignatures = (voucher.signatures || []).filter(
        (s) => s.role === 'guarantor' && s.signerId && s.signerId !== state.ownUserId,
    );
    const canClaimAtGuarantor =
        statusInfo.id === 'active' && !isOwnVoucher && guarantorSignatures.length > 0;
    const formatDateTime = (iso?: string) => (iso ? new Date(iso).toLocaleString() : t('common.na'));

    return (
        <PageLayout
            title={t('voucher.detailsTitle')}
            description={details.displayStandardName}
            onBack={state.goBack}
            actions={
                <VoucherHeaderActions
                    statusId={statusInfo.id}
                    canRedeemAtIssuer={canRedeemAtIssuer}
                    canClaimAtGuarantor={canClaimAtGuarantor}
                    isCheckingL2={state.isCheckingL2}
                    showJson={state.showJson}
                    onRedeemAtIssuer={() =>
                        state.navigate({
                            view: 'send_vouchers',
                            initialRecipientId: voucher.creator.id,
                            initialVoucherId: details.localInstanceId,
                            initialNote: t('voucher.redeemNoteDefault', {
                                voucherId: voucher.voucherId.slice(0, 8),
                            }),
                        })
                    }
                    onToggleClaimPanel={() => {
                        state.setClaimFeedback(null);
                        state.setClaimPanelOpen((prev) => !prev);
                    }}
                    onCheckL2Status={state.handleCheckL2Status}
                    onOpenExportModal={() => state.setShowExportModal(true)}
                    onToggleJson={() => state.setShowJson((prev) => !prev)}
                />
            }
        >
            <div className="max-w-5xl mx-auto space-y-6">
                <VoucherActionsPanel
                    details={details}
                    statusInfo={statusInfo}
                    isQuarantined={state.isQuarantined}
                    proofId={state.proofId}
                    signatureImportFeedback={state.signatureImportFeedback}
                    onDismissSignatureFeedback={() => state.setSignatureImportFeedback(null)}
                    savedAskFilePath={state.savedAskFilePath}
                    onDismissSavedAskPath={() => state.setSavedAskFilePath(null)}
                    l2Status={state.l2Status}
                    l2Error={state.l2Error}
                    signatureHint={signatureHint}
                    guarantorProgress={guarantorProgress}
                    showJson={state.showJson}
                    onViewProof={(id) =>
                        state.navigate({ view: 'conflict_details', proofId: id, previousView: appState })
                    }
                    onOpenExportModal={() => state.setShowExportModal(true)}
                    onImportSignatureFile={state.handleImportSignatureFile}
                />

                {state.claimFeedback && (
                    <AlertBanner
                        variant={state.claimFeedback.type}
                        onDismiss={() => state.setClaimFeedback(null)}
                    >
                        {state.claimFeedback.msg}
                    </AlertBanner>
                )}

                {state.claimPanelOpen && canClaimAtGuarantor && (
                    <GuarantorClaimPanel
                        details={details}
                        guarantorSignatures={guarantorSignatures}
                        allowPartialTransfers={allowPartialTransfers}
                        config={state.config}
                        protectAction={state.protectAction}
                        findMatchingStandardToml={state.findMatchingStandardToml}
                        onClose={() => state.setClaimPanelOpen(false)}
                        onFeedback={state.setClaimFeedback}
                        onRefreshDetails={state.refreshDetails}
                    />
                )}

                <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                    <div className="lg:col-span-2 space-y-6">
                        <CreatorSection
                            creator={voucher.creator}
                            trustStatus={state.trustStatus}
                            isContact={state.contacts.some(
                                (c) =>
                                    isExactSameUserId(c.did, voucher.creator.id) ||
                                    isSameRootIdentity(c.did, voucher.creator.id),
                            )}
                            onManageContact={() => {
                                state.setPendingContactDID(voucher.creator.id);
                                state.setExistingContact(
                                    state.contacts.find((c) => isExactSameUserId(c.did, voucher.creator.id)) || null,
                                );
                                state.setShowContactDialog(true);
                            }}
                        />
                        <PolicySection description={standardDescription} footnote={standardFootnote} />
                        <TimelineSection
                            signatures={voucher.signatures}
                            transactions={voucher.transactions}
                            displayCurrency={details.displayCurrency}
                            onRemoveSignature={state.setShowRemoveSignatureModal}
                            voucherStatus={details.status}
                        />
                    </div>

                    <div className="space-y-6">
                        <Card
                            className="border-none shadow-premium"
                            header={
                                <div className="flex items-center justify-between w-full">
                                    <div className="flex items-center gap-2">
                                        <Shield size={16} className="text-theme-primary" />
                                        <span className="font-black text-xs uppercase tracking-widest">
                                            {t('voucher.trustCardTitle')}
                                        </span>
                                    </div>
                                    <HelpIcon topic="voucherTrust" size={13} />
                                </div>
                            }
                        >
                            <div className="space-y-4">
                                <div>
                                    <TrustBadge trust={details.trust} size="md" />
                                    {(details.reducedGuarantorProtection ||
                                        state.l2Status?.reducedGuarantorProtection) && (
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
                                        {details.trust &&
                                        details.trust.shortestHops != null &&
                                        details.trust.trustPercent > 0 ? (
                                            <div>
                                                <p className="text-[10px] font-black uppercase text-theme-light mb-1.5">
                                                    {t('voucher.trustPathLabel')}
                                                </p>
                                                <p className="font-medium text-theme-secondary flex items-center flex-wrap gap-1 leading-relaxed">
                                                    {[
                                                        t('voucher.trustPathYou'),
                                                        ...(details.trust.bestPathNames || []),
                                                        `${voucher.creator.firstName || ''} ${voucher.creator.lastName || ''}`.trim() ||
                                                            t('voucher.trustPathIssuer'),
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

                        <Card
                            className="border-none shadow-premium"
                            header={
                                <div className="flex items-center gap-2">
                                    <Info size={16} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">
                                        {t('voucher.detailsHeader')}
                                    </span>
                                </div>
                            }
                        >
                            <div className="space-y-6">
                                <Advanced>
                                    <InfoRow label={t('voucher.idLabel')} icon={Shield}>
                                        <TechId value={voucher.voucherId} kind="voucher" />
                                    </InfoRow>
                                </Advanced>
                                <InfoRow label={t('voucher.issueDateLabel')} icon={Clock}>
                                    {formatDateTime(voucher.creationDate)}
                                </InfoRow>
                                <InfoRow label={t('voucher.validUntilLabel')} icon={AlertCircle}>
                                    {formatDateTime(voucher.validUntil || undefined)}
                                </InfoRow>
                                <div className="pt-4 border-t border-theme-subtle/30 grid grid-cols-2 gap-4">
                                    <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                        <p className="text-[9px] font-black uppercase text-theme-light mb-1">
                                            {t('voucher.divisibleLabel')}
                                        </p>
                                        <p className="text-xs font-bold">
                                            {allowPartialTransfers
                                                ? t('voucher.divisibleYes')
                                                : t('voucher.divisibleNo')}
                                        </p>
                                    </div>
                                    <div className="text-center p-3 bg-theme-subtle/10 rounded-2xl">
                                        <p className="text-[9px] font-black uppercase text-theme-light mb-1">
                                            {t('voucher.collateralLabel')}
                                        </p>
                                        <p className="text-xs font-bold">
                                            {voucher.collateral?.amount
                                                ? `${voucher.collateral.amount} ${voucher.collateral.abbreviation || voucher.collateral.unit}`
                                                : t('voucher.noCollateral')}
                                        </p>
                                    </div>
                                </div>
                            </div>
                        </Card>

                        {Boolean(voucher.layer2Anchor) &&
                            (state.l2Status?.status === 'offline' || state.l2Status?.status === 'no_servers') && (
                                <AlertBanner
                                    variant="info"
                                    icon={<AlertCircle size={18} className="text-sky-600" />}
                                    title={t(
                                        'voucher.l2UnreachableTitle',
                                        'Protection committee currently unreachable'
                                    )}
                                >
                                    <p className="text-xs leading-relaxed">
                                        {t(
                                            'voucher.l2UnreachableDesc',
                                            'The security services for this voucher cannot currently be reached. Transfers still work offline using cryptographic signature traps, though instant online double-spend prevention is paused until connectivity returns.'
                                        )}
                                    </p>
                                </AlertBanner>
                            )}
                    </div>
                </div>
            </div>

            <VoucherDetailsModals
                voucherId={voucherId}
                voucher={voucher}
                config={state.config}
                contacts={state.contacts}
                protectAction={state.protectAction}
                showExportModal={state.showExportModal}
                onCloseExportModal={() => state.setShowExportModal(false)}
                onConfigRefresh={state.setConfig}
                onAskSaved={state.setSavedAskFilePath}
                pendingSigImport={state.pendingSigImport}
                sigContainerPassword={state.sigContainerPassword}
                sigPasswordError={state.sigPasswordError}
                onSigContainerPasswordChange={state.setSigContainerPassword}
                onSigPasswordError={state.setSigPasswordError}
                onExecuteSigAttachment={state.executeSignatureAttachment}
                onCancelSigImport={() => {
                    state.setPendingSigImport(null);
                    state.setSigContainerPassword('');
                    state.setSigPasswordError('');
                }}
                showContactDialog={state.showContactDialog}
                pendingContactDID={state.pendingContactDID}
                existingContact={state.existingContact}
                onCloseContactDialog={() => {
                    state.setShowContactDialog(false);
                    state.setPendingContactDID(null);
                    state.setExistingContact(null);
                }}
                onContactsUpdated={state.setContacts}
                onRefreshDetails={state.refreshDetails}
                showRemoveSignatureModal={state.showRemoveSignatureModal}
                isRemovingSignature={state.isRemovingSignature}
                onConfirmRemoveSignature={state.handleRemoveSignature}
                onCancelRemoveSignature={() => state.setShowRemoveSignatureModal(null)}
            />
        </PageLayout>
    );
}
