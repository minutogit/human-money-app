// src/components/transfer/receive/useReceiveWotImport.ts
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { wotService } from '../../../services/wotService';
import { contactService } from '../../../services/contactService';
import { logger } from '../../../utils/log';
import { stringifyError, translateError } from '../../../utils/errorHelper';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type { Contact, WotImportResultDto } from '../../../types';
import type { ResultModalState } from './useReceiveModals';

export function useReceiveWotImport({
    protectAction,
    clearSelection,
    setResultModal,
}: {
    protectAction: ProtectActionFn;
    clearSelection: () => void;
    setResultModal: (modal: ResultModalState | null) => void;
}) {
    const { t } = useTranslation();
    const [wotImportModalData, setWotImportModalData] = useState<WotImportResultDto | null>(null);
    const [selectedProviderLevel, setSelectedProviderLevel] = useState<number>(0);
    const [isSavingProviderTrust, setIsSavingProviderTrust] = useState(false);
    const [wotContactDialogOpen, setWotContactDialogOpen] = useState(false);
    const [wotExistingContact, setWotExistingContact] = useState<Contact | null>(null);

    const openWotImportResult = (result: WotImportResultDto) => {
        setWotImportModalData(result);
        setSelectedProviderLevel(result.existingTrustLevel ?? 0);
    };

    const handleCloseWotModal = () => {
        setWotImportModalData(null);
        clearSelection();
    };

    const handleSaveProviderTrust = async () => {
        if (!wotImportModalData?.senderDid) return;
        setIsSavingProviderTrust(true);
        try {
            await protectAction(async (pwd) => {
                await wotService.setProviderTrust(
                    wotImportModalData.senderDid,
                    selectedProviderLevel,
                    pwd || null,
                );
            });
            handleCloseWotModal();
        } catch (err) {
            logger.error('Failed to save provider trust: ' + stringifyError(err));
            setResultModal({
                isOpen: true,
                title: t('common.error'),
                content: (
                    <p className="text-sm font-medium text-rose-600">{translateError(err, t)}</p>
                ),
                confirmText: t('common.ok'),
            });
        } finally {
            setIsSavingProviderTrust(false);
        }
    };

    const handleOpenContactFromWot = async () => {
        if (!wotImportModalData) return;
        try {
            const allContacts = await contactService.getContacts();
            setWotExistingContact(
                allContacts.find((c) => c.did === wotImportModalData.senderDid) || null,
            );
        } catch {
            setWotExistingContact(null);
        }
        setWotContactDialogOpen(true);
    };

    const handleSaveContactFromWot = async (contact: Contact) => {
        await protectAction(async (pwd) => {
            await contactService.saveContact(contact, pwd || undefined);
        });
        setWotContactDialogOpen(false);
        setWotImportModalData(null);
        clearSelection();
    };

    return {
        wotImportModalData,
        openWotImportResult,
        selectedProviderLevel,
        setSelectedProviderLevel,
        isSavingProviderTrust,
        handleSaveProviderTrust,
        handleOpenContactFromWot,
        handleCloseWotModal,
        wotContactDialogOpen,
        setWotContactDialogOpen,
        wotExistingContact,
        handleSaveContactFromWot,
    };
}
