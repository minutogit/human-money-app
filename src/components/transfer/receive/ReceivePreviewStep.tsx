// src/components/transfer/receive/ReceivePreviewStep.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Lock, CheckCircle2 } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import { ToleranceConfirmationModal } from './ToleranceConfirmationModal';
import { ModalShell } from '../../ui/ModalShell';
import { Advanced } from '../../ui/Advanced';
import { TechId } from '../../ui/TechId';
import { ImportStandardModal } from '../../standards/ImportStandardModal';
import ContactDialog from '../../contacts/ContactDialog';
import type {
    Contact,
    ToleranceReceiveConfirmation,
    VoucherStandardDefinition,
    WotImportResultDto,
} from '../../../types';
import type { PendingPasswordImportState, ResultModalState } from './useReceiveFlow';

interface ReceivePreviewStepProps {
    showConfirm: boolean;
    pendingPasswordImport: PendingPasswordImportState | null;
    bundlePassword: string;
    onBundlePasswordChange: (val: string) => void;
    onConfirmPassword: () => void;
    onCancelPassword: () => void;
    isProcessing: boolean;
    toleranceModal: ToleranceReceiveConfirmation | null;
    confirmText: string;
    onConfirmTextChange: (val: string) => void;
    onConfirmTolerance: () => void;
    onCancelTolerance: () => void;
    resultModal: ResultModalState | null;
    onSetResultModal: (modal: ResultModalState | null) => void;
    onConfirmResult: () => void;
    onDismissResult: () => void;
    importStandardModalData: {
        containerBytes: number[];
        definition?: VoucherStandardDefinition;
    } | null;
    onCloseImportStandard: () => void;
    onRefreshStandards: () => void;
    wotImportModalData: WotImportResultDto | null;
    selectedProviderLevel: number;
    onSelectProviderLevel: (level: number) => void;
    isSavingProviderTrust: boolean;
    onSaveProviderTrust: () => void;
    onOpenContactFromWot: () => void;
    onCloseWotModal: () => void;
    wotContactDialogOpen: boolean;
    onCloseWotContactDialog: () => void;
    wotExistingContact: Contact | null;
    onSaveContactFromWot: (contact: Contact) => Promise<void>;
}

export function ReceivePreviewStep({
    showConfirm,
    pendingPasswordImport,
    bundlePassword,
    onBundlePasswordChange,
    onConfirmPassword,
    onCancelPassword,
    isProcessing,
    toleranceModal,
    confirmText,
    onConfirmTextChange,
    onConfirmTolerance,
    onCancelTolerance,
    resultModal,
    onSetResultModal,
    onConfirmResult,
    onDismissResult,
    importStandardModalData,
    onCloseImportStandard,
    onRefreshStandards,
    wotImportModalData,
    selectedProviderLevel,
    onSelectProviderLevel,
    isSavingProviderTrust,
    onSaveProviderTrust,
    onOpenContactFromWot,
    onCloseWotModal,
    wotContactDialogOpen,
    onCloseWotContactDialog,
    wotExistingContact,
    onSaveContactFromWot,
}: ReceivePreviewStepProps) {
    const { t } = useTranslation();

    const isProvider = wotImportModalData?.isProviderFilter;
    const senderProfile = wotImportModalData?.senderProfile;
    const senderDisplayName = isProvider
        ? wotImportModalData?.providerName || wotImportModalData?.senderDid || ''
        : senderProfile
          ? senderProfile.firstName || senderProfile.lastName
              ? `${senderProfile.firstName || ''} ${senderProfile.lastName || ''}`.trim()
              : senderProfile.organization || wotImportModalData?.senderDid || ''
          : wotImportModalData?.senderDid || '';

    const passwordModalTitle =
        pendingPasswordImport?.type === 'transfer'
            ? t('transfer.importFile.importModalTitleTransfer')
            : pendingPasswordImport?.type === 'ask'
              ? t('transfer.importFile.importModalTitleAsk')
              : pendingPasswordImport?.type === 'sig'
                ? t('transfer.importFile.importModalTitleSig')
                : t('transfer.wotImport.title');

    return (
        <>
            <ConfirmationModal
                isOpen={showConfirm && !!pendingPasswordImport}
                title={passwordModalTitle}
                description={
                    <div className="space-y-6 pt-2">
                        <div className="p-4 bg-rose-500/5 rounded-2xl border border-rose-500/10 flex items-center gap-4">
                            <div className="p-2 bg-white rounded-xl shadow-sm text-rose-500">
                                <Lock size={24} />
                            </div>
                            <div>
                                <p className="text-sm font-bold text-theme-secondary truncate max-w-[200px]">
                                    {pendingPasswordImport?.name}
                                </p>
                                <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                                    {t('transfer.importFile.typeLabel', {
                                        fileType: pendingPasswordImport?.type,
                                    })}
                                </p>
                            </div>
                        </div>

                        <div className="space-y-3">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                                <Lock size={12} /> {t('transfer.importFile.passwordLabel')}
                            </label>
                            <Input
                                type="password"
                                value={bundlePassword}
                                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                                    onBundlePasswordChange(e.target.value)
                                }
                                placeholder={t('transfer.importFile.passwordPlaceholder')}
                                autoFocus
                            />
                            <p className="text-[10px] text-theme-light font-medium italic">
                                {t('transfer.importFile.passwordHint')}
                            </p>
                        </div>
                    </div>
                }
                confirmText={t('transfer.importFile.confirmImport')}
                onConfirm={onConfirmPassword}
                onCancel={onCancelPassword}
                isProcessing={isProcessing}
            />

            {toleranceModal && (
                <ToleranceConfirmationModal
                    isOpen={true}
                    zone={toleranceModal.type}
                    message={toleranceModal.message}
                    confirmText={confirmText}
                    onConfirmTextChange={onConfirmTextChange}
                    onConfirm={onConfirmTolerance}
                    onCancel={onCancelTolerance}
                    isProcessing={isProcessing}
                />
            )}

            {resultModal && (
                <ConfirmationModal
                    isOpen={resultModal.isOpen}
                    title={resultModal.title}
                    description={resultModal.content}
                    confirmText={resultModal.confirmText}
                    cancelText={t('transfer.importFile.dismiss')}
                    onConfirm={onConfirmResult}
                    onCancel={onDismissResult}
                />
            )}

            {importStandardModalData && (
                <ImportStandardModal
                    isOpen={true}
                    containerBytes={importStandardModalData.containerBytes}
                    initialDefinition={importStandardModalData.definition}
                    onClose={onCloseImportStandard}
                    onSuccess={(importedId) => {
                        onSetResultModal({
                            isOpen: true,
                            title: t('standards.importSuccess'),
                            content: (
                                <div className="space-y-3">
                                    <p className="text-xs font-bold text-theme-primary">
                                        {t('standards.importSuccess')}
                                    </p>
                                    <Advanced>
                                        <p className="text-[10px] font-mono text-theme-light">
                                            Standard ID: {importedId}
                                        </p>
                                    </Advanced>
                                </div>
                            ),
                            confirmText: t('common.ok'),
                        });
                        onRefreshStandards();
                    }}
                />
            )}

            <ModalShell
                isOpen={!!wotImportModalData}
                onClose={onCloseWotModal}
                maxWidth="md"
                title={
                    <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-2xl bg-emerald-50 text-emerald-600 flex items-center justify-center shrink-0 border border-emerald-100">
                            <CheckCircle2 size={20} />
                        </div>
                        <h3 className="text-lg font-black text-theme-secondary tracking-tight">
                            {wotImportModalData?.isProviderFilter
                                ? t('transfer.wotImport.providerTitle')
                                : t('transfer.wotImport.title')}
                        </h3>
                    </div>
                }
                description={
                    wotImportModalData?.isProviderFilter
                        ? t('transfer.wotImport.providerDescription', { name: senderDisplayName })
                        : t('transfer.wotImport.fromSender', { name: senderDisplayName })
                }
                footer={
                    <div className="flex gap-3">
                        <button
                            type="button"
                            onClick={onCloseWotModal}
                            className="flex-1 py-2.5 px-4 rounded-xl border border-theme-subtle text-theme-light font-bold text-xs hover:bg-bg-app transition-colors cursor-pointer"
                        >
                            {t('common.close')}
                        </button>
                        {wotImportModalData?.isProviderFilter ? (
                            <Button
                                type="button"
                                onClick={onSaveProviderTrust}
                                disabled={isSavingProviderTrust}
                                className="flex-[2] py-2.5 px-4 rounded-xl text-xs font-bold"
                            >
                                {isSavingProviderTrust
                                    ? t('common.loading')
                                    : t('contacts.saveContact')}
                            </Button>
                        ) : (
                            <Button
                                type="button"
                                onClick={onOpenContactFromWot}
                                className="flex-[2] py-2.5 px-4 rounded-xl text-xs font-bold"
                            >
                                {wotImportModalData?.isKnownContact
                                    ? t('transfer.wotImport.updateContactAction')
                                    : t('transfer.wotImport.rateContactAction')}
                            </Button>
                        )}
                    </div>
                }
            >
                {wotImportModalData && (
                    <div className="space-y-4">
                        <div className="p-4 bg-white rounded-2xl border border-theme-subtle/60 shadow-sm space-y-2">
                            <p className="text-sm font-semibold text-theme-secondary">
                                {wotImportModalData.isProviderFilter
                                    ? t('transfer.wotImport.providerStats', {
                                          count: wotImportModalData.providerMemberCount ?? 0,
                                      })
                                    : t('transfer.wotImport.stats', {
                                          imported: wotImportModalData.imported,
                                          updated: wotImportModalData.updated,
                                      })}
                            </p>
                            <TechId value={wotImportModalData.senderDid} kind="did" />
                        </div>

                        {wotImportModalData.isProviderFilter && (
                            <div className="space-y-2 pt-1">
                                <label className="block text-xs font-bold text-theme-light uppercase tracking-widest">
                                    {t('contacts.trustSectionLabel')}
                                </label>
                                <div className="grid grid-cols-4 gap-2">
                                    {[
                                        { level: 0, labelKey: 'contacts.providerTrustInactive' },
                                        { level: 1, labelKey: 'contacts.trustLevel1' },
                                        { level: 2, labelKey: 'contacts.trustLevel2' },
                                        { level: 3, labelKey: 'contacts.trustLevel3' },
                                    ].map((opt) => (
                                        <button
                                            key={opt.level}
                                            type="button"
                                            onClick={() => onSelectProviderLevel(opt.level)}
                                            className={`py-2 px-1.5 rounded-xl text-xs font-bold border text-center transition-all cursor-pointer ${
                                                selectedProviderLevel === opt.level
                                                    ? 'bg-theme-secondary border-theme-secondary text-white shadow-sm'
                                                    : 'bg-white border-theme-subtle text-theme-light hover:border-theme-primary/30'
                                            }`}
                                        >
                                            {t(opt.labelKey)}
                                        </button>
                                    ))}
                                </div>
                            </div>
                        )}
                    </div>
                )}
            </ModalShell>

            {wotContactDialogOpen && (
                <ContactDialog
                    isOpen={wotContactDialogOpen}
                    onClose={onCloseWotContactDialog}
                    onSave={onSaveContactFromWot}
                    existingContact={wotExistingContact}
                    initialDid={wotImportModalData?.senderDid}
                    initialProfile={wotImportModalData?.senderProfile}
                />
            )}
        </>
    );
}
