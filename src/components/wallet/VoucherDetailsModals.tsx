// src/components/wallet/VoucherDetailsModals.tsx
import { useTranslation } from 'react-i18next';
import { contactService } from '../../services/contactService';
import type { ProtectActionFn } from '../../utils/fileIo';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import ContactDialog from '../contacts/ContactDialog';
import { ExportSigningRequestModal } from '../voucher/ExportSigningRequestModal';
import type { AppConfigDto, Contact, Voucher } from '../../types';

interface VoucherDetailsModalsProps {
    voucherId: string;
    voucher: Voucher;
    config: AppConfigDto | null;
    contacts: Contact[];
    protectAction: ProtectActionFn;
    showExportModal: boolean;
    onCloseExportModal: () => void;
    onConfigRefresh: (config: AppConfigDto) => void;
    onAskSaved: (savedPath: string) => void;
    pendingSigImport: { fileBytes: Uint8Array; standardToml: string } | null;
    sigContainerPassword: string;
    sigPasswordError: string;
    onSigContainerPasswordChange: (val: string) => void;
    onSigPasswordError: (err: string) => void;
    onExecuteSigAttachment: (
        fileBytes: Uint8Array,
        standardToml: string,
        pwd?: string,
    ) => Promise<void>;
    onCancelSigImport: () => void;
    showContactDialog: boolean;
    pendingContactDID: string | null;
    existingContact: Contact | null;
    onCloseContactDialog: () => void;
    onContactsUpdated: (contacts: Contact[]) => void;
    onRefreshDetails: () => Promise<void>;
    showRemoveSignatureModal: string | null;
    isRemovingSignature: boolean;
    onConfirmRemoveSignature: () => void;
    onCancelRemoveSignature: () => void;
}

export function VoucherDetailsModals({
    voucherId,
    voucher,
    config,
    contacts,
    protectAction,
    showExportModal,
    onCloseExportModal,
    onConfigRefresh,
    onAskSaved,
    pendingSigImport,
    sigContainerPassword,
    sigPasswordError,
    onSigContainerPasswordChange,
    onSigPasswordError,
    onExecuteSigAttachment,
    onCancelSigImport,
    showContactDialog,
    pendingContactDID,
    existingContact,
    onCloseContactDialog,
    onContactsUpdated,
    onRefreshDetails,
    showRemoveSignatureModal,
    isRemovingSignature,
    onConfirmRemoveSignature,
    onCancelRemoveSignature,
}: VoucherDetailsModalsProps) {
    const { t } = useTranslation();

    return (
        <>
            <ExportSigningRequestModal
                isOpen={showExportModal}
                voucherId={voucherId}
                onClose={onCloseExportModal}
                config={config}
                protectAction={protectAction}
                onConfigRefresh={onConfigRefresh}
                contacts={contacts}
                onSuccess={onAskSaved}
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
                                    onSigContainerPasswordChange(e.target.value);
                                    onSigPasswordError('');
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
                            onSigPasswordError(t('voucher.errorEnterPassword'));
                            return;
                        }
                        void onExecuteSigAttachment(
                            pendingSigImport.fileBytes,
                            pendingSigImport.standardToml,
                            sigContainerPassword,
                        );
                    }}
                    onCancel={onCancelSigImport}
                />
            )}

            <ContactDialog
                isOpen={showContactDialog}
                onClose={onCloseContactDialog}
                onSave={async (contact: Contact) => {
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(contact, pwd || undefined);
                        await contactService.getContacts().then(onContactsUpdated);
                        await onRefreshDetails();
                    });
                }}
                existingContact={existingContact}
                initialProfile={
                    pendingContactDID
                        ? voucher.signatures.find((s) => s.signerId === pendingContactDID)?.details
                        : voucher.creator
                }
                initialDid={pendingContactDID || voucher.creator.id}
            />

            <ConfirmationModal
                isOpen={showRemoveSignatureModal !== null}
                title={t('voucher.removeSignatureTitle')}
                description={t('voucher.removeSignatureConfirm')}
                confirmText={t('common.remove')}
                confirmVariant="danger"
                onConfirm={onConfirmRemoveSignature}
                onCancel={onCancelRemoveSignature}
                isProcessing={isRemovingSignature}
            />
        </>
    );
}
