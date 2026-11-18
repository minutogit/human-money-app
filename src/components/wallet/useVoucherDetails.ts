// src/components/wallet/useVoucherDetails.ts
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { voucherService } from '../../services/voucherService';
import { settingsService } from '../../services/settingsService';
import { profileService } from '../../services/profileService';
import { standardsService } from '../../services/standardsService';
import { contactService } from '../../services/contactService';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { parseStandardInfo } from '../../hooks/useParsedStandards';
import { openFileAsBytes } from '../../utils/fileIo';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';
import { APP_EVENTS, emitAppEvent } from '../../utils/appEvents';
import type {
    AppSettings,
    VoucherDetails,
    Contact,
    TrustStatus,
    PublicProfile,
    VoucherStandardDefinition,
    VoucherL2StatusDto,
} from '../../types';

export function useVoucherDetails(voucherId: string) {
    const { t, i18n } = useTranslation();
    const { navigate, goBack, appState } = useNavigation();
    const { protectAction } = useSession();

    const [details, setDetails] = useState<VoucherDetails | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState('');
    const [showJson, setShowJson] = useState(false);
    const [showExportModal, setShowExportModal] = useState(false);
    const [showContactDialog, setShowContactDialog] = useState(false);
    const [pendingContactDID, setPendingContactDID] = useState<string | null>(null);
    const [existingContact, setExistingContact] = useState<Contact | null>(null);
    const [showRemoveSignatureModal, setShowRemoveSignatureModal] = useState<string | null>(null);
    const [isRemovingSignature, setIsRemovingSignature] = useState(false);
    const [proofId, setProofId] = useState<string | null>(null);
    const [isFetchingProofId, setIsFetchingProofId] = useState(false);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>('clean');
    const [userProfile, setUserProfile] = useState<PublicProfile | null>(null);
    const [parsedStandards, setParsedStandards] = useState<Record<string, VoucherStandardDefinition>>({});
    const [contacts, setContacts] = useState<Contact[]>([]);
    const [ownUserId, setOwnUserId] = useState('');
    const [savedAskFilePath, setSavedAskFilePath] = useState<string | null>(null);
    const [signatureImportFeedback, setSignatureImportFeedback] = useState<{
        type: 'success' | 'error';
        msg: string;
    } | null>(null);
    const [pendingSigImport, setPendingSigImport] = useState<{
        fileBytes: Uint8Array;
        standardToml: string;
    } | null>(null);
    const [sigContainerPassword, setSigContainerPassword] = useState('');
    const [sigPasswordError, setSigPasswordError] = useState('');
    const [isCheckingL2, setIsCheckingL2] = useState(false);
    const [l2Status, setL2Status] = useState<VoucherL2StatusDto | null>(null);
    const [l2Error, setL2Error] = useState<string | null>(null);
    const [claimPanelOpen, setClaimPanelOpen] = useState(false);
    const [claimFeedback, setClaimFeedback] = useState<{
        type: 'success' | 'error';
        msg: string;
    } | null>(null);

    const voucher = details?.voucher;
    const isQuarantined = Boolean(details && details.status === 'quarantined');

    useEffect(() => {
        let active = true;
        if (voucher?.creator.id) {
            voucherService
                .checkReputation(voucher.creator.id)
                .then((res) => {
                    if (active) setTrustStatus(res);
                })
                .catch((e) => logger.error(`Reputation check error: ${stringifyError(e)}`));
        }
        return () => {
            active = false;
        };
    }, [voucher?.creator.id]);

    useEffect(() => {
        let active = true;
        async function fetchDetails() {
            setIsLoading(true);
            setErrorMsg('');
            try {
                const [result, currentSettings, profile, standards, uid] = await Promise.all([
                    voucherService.getDetails(voucherId),
                    settingsService.getSettings().catch(() => null),
                    profileService.getProfile().catch(() => null),
                    standardsService.getStandards().catch(() => []),
                    profileService.getUserId().catch(() => ''),
                ]);
                if (!active) return;
                setDetails(result);
                setSettings(currentSettings);
                setUserProfile(profile);
                setOwnUserId(uid || '');

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
        return () => {
            active = false;
        };
    }, [voucherId, t]);

    useEffect(() => {
        let active = true;
        contactService
            .getContacts()
            .then((res) => {
                if (active) setContacts(res);
            })
            .catch((e) => logger.error(`Failed to fetch contacts: ${stringifyError(e)}`));
        return () => {
            active = false;
        };
    }, []);

    useEffect(() => {
        let active = true;
        if (isQuarantined && !proofId && !isFetchingProofId) {
            setIsFetchingProofId(true);
            voucherService
                .getProofId(voucherId)
                .then((id) => {
                    if (active) setProofId(id);
                })
                .catch((e) =>
                    logger.error(
                        `Failed to fetch proof ID for voucher ${voucherId}: ${stringifyError(e)}`,
                    ),
                )
                .finally(() => {
                    if (active) setIsFetchingProofId(false);
                });
        }
        return () => {
            active = false;
        };
    }, [isQuarantined, proofId, isFetchingProofId, voucherId]);

    const refreshDetails = useCallback(async () => {
        try {
            const result = await voucherService.getDetails(voucherId);
            setDetails(result);
        } catch (e) {
            logger.error(`Failed to refresh voucher details: ${stringifyError(e)}`);
        }
    }, [voucherId]);

    const handleCheckL2Status = async () => {
        if (!details) return;
        setIsCheckingL2(true);
        setL2Error(null);
        try {
            const statusDto = await protectAction(async (pwd) => {
                return await settingsService.checkVoucherL2Status(
                    details.localInstanceId,
                    pwd || null,
                );
            });
            if (statusDto) {
                setL2Status(statusDto);
                if (statusDto.status === 'conflict') {
                    emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
                }
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

    const targetUuid = voucher?.voucherStandard.uuid || '';
    const findMatchingStandardToml = async (): Promise<string> => {
        const standardsList = await standardsService.getStandards();
        const matchingStd = standardsList.find((s) => parseStandardInfo(s).uuid === targetUuid);
        return matchingStd?.content || '';
    };

    const executeSignatureAttachment = async (
        fileBytes: Uint8Array,
        standardToml: string,
        containerPwd?: string,
    ) => {
        try {
            await protectAction(async (walletPassword) => {
                return await voucherService.processAndAttachSignature(
                    Array.from(fileBytes),
                    standardToml,
                    containerPwd || undefined,
                    walletPassword || undefined,
                );
            });
            const updatedDetails = await voucherService.getDetails(voucherId);
            setDetails(updatedDetails);
            setPendingSigImport(null);
            setSigContainerPassword('');
            setSigPasswordError('');
            setSignatureImportFeedback({
                type: 'success',
                msg:
                    updatedDetails.status === 'active'
                        ? t('voucher.allSignaturesComplete')
                        : t('voucher.signatureAttachedTitle'),
            });
        } catch (e) {
            const errStr = stringifyError(e);
            const isPasswordRequired =
                errStr.includes('Password required') ||
                errStr.toLowerCase().includes('decrypt') ||
                errStr.includes('SymmetricEncryption') ||
                errStr.includes('MacError');
            if (isPasswordRequired && !containerPwd) {
                setPendingSigImport({ fileBytes, standardToml });
                setSigPasswordError('');
            } else {
                setSignatureImportFeedback({
                    type: 'error',
                    msg: `${t('voucher.signatureAttachFailed')}: ${translateError(e, t)}`,
                });
            }
        }
    };

    const handleImportSignatureFile = async () => {
        try {
            setSignatureImportFeedback(null);
            const opened = await openFileAsBytes({
                settings,
                protectAction,
                filters: [
                    {
                        name: t('voucher.signRequest.signatureResponseFilter'),
                        extensions: ['sig', 'humocosig'],
                    },
                    { name: t('voucher.export.allFiles'), extensions: ['*'] },
                ],
            });
            if (!opened) return;
            const standardTomlContent = await findMatchingStandardToml();
            await executeSignatureAttachment(new Uint8Array(opened.bytes), standardTomlContent);
        } catch (e) {
            setSignatureImportFeedback({
                type: 'error',
                msg: `${t('voucher.signatureAttachFailed')}: ${translateError(e, t)}`,
            });
        }
    };

    const handleRemoveSignature = async () => {
        if (!showRemoveSignatureModal) return;
        setIsRemovingSignature(true);
        try {
            await protectAction(async (pwd) => {
                await voucherService.removeSignature(
                    voucherId,
                    showRemoveSignatureModal,
                    pwd || undefined,
                );
            });
            setShowRemoveSignatureModal(null);
            await refreshDetails();
        } catch (e) {
            logger.error(`Failed to remove signature: ${stringifyError(e)}`);
            setSignatureImportFeedback({
                type: 'error',
                msg: `${t('voucher.failedRemoveSignature')}: ${translateError(e, t)}`,
            });
        } finally {
            setIsRemovingSignature(false);
        }
    };

    return {
        i18n,
        navigate,
        goBack,
        appState,
        protectAction,
        details,
        settings,
        setSettings,
        isLoading,
        errorMsg,
        showJson,
        setShowJson,
        showExportModal,
        setShowExportModal,
        showContactDialog,
        setShowContactDialog,
        pendingContactDID,
        setPendingContactDID,
        existingContact,
        setExistingContact,
        showRemoveSignatureModal,
        setShowRemoveSignatureModal,
        isRemovingSignature,
        proofId,
        trustStatus,
        userProfile,
        parsedStandards,
        contacts,
        setContacts,
        ownUserId,
        savedAskFilePath,
        setSavedAskFilePath,
        signatureImportFeedback,
        setSignatureImportFeedback,
        pendingSigImport,
        setPendingSigImport,
        sigContainerPassword,
        setSigContainerPassword,
        sigPasswordError,
        setSigPasswordError,
        isCheckingL2,
        l2Status,
        l2Error,
        claimPanelOpen,
        setClaimPanelOpen,
        claimFeedback,
        setClaimFeedback,
        isQuarantined,
        refreshDetails,
        handleCheckL2Status,
        findMatchingStandardToml,
        executeSignatureAttachment,
        handleImportSignatureFile,
        handleRemoveSignature,
    };
}
