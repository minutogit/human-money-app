// src/components/marketplace/ImportCardModal.tsx
import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Download, FileText, CheckCircle2, AlertCircle, Info, Upload, Check, QrCode } from 'lucide-react';
import { ModalShell } from '../ui/ModalShell';
import { QrScannerModal } from '../ui/QrScannerModal';
import { Button } from '../ui/Button';
import { Textarea } from '../ui/Textarea';
import { marketService } from '../../services/marketService';
import { contactService } from '../../services/contactService';
import { profileService } from '../../services/profileService';
import { useSession } from '../../context/SessionContext';
import { openFileAsBytes } from '../../utils/fileIo';
import { logger } from '../../utils/log';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { isExactSameUserId, isSameRootIdentity, truncateUserId } from '../../utils/userIdHelper';
import { parseHumocoInput } from '../../utils/humocoUri';
import { notifySyncStatusUpdated } from '../../utils/syncEvents';
import { Contact, FrontendMyMarketProfile } from '../../types';

export interface ImportCardModalProps {
    isOpen: boolean;
    onClose: () => void;
    onImportSuccess: (did?: string | null) => void;
}

export function ImportCardModal({ isOpen, onClose, onImportSuccess }: ImportCardModalProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [payloadText, setPayloadText] = useState('');
    const [isImporting, setIsImporting] = useState(false);
    const [resultFeedback, setResultFeedback] = useState<{
        type: 'success' | 'warning' | 'error';
        title: string;
        message: string;
        did?: string | null;
    } | null>(null);

    const [myProfile, setMyProfile] = useState<FrontendMyMarketProfile | null>(null);
    const [partnerName, setPartnerName] = useState<string | null>(null);
    const [siblingContactName, setSiblingContactName] = useState<string | null>(null);
    const [selectedTrustLevel, setSelectedTrustLevel] = useState<number | null>(null);
    const [isSavingTrust, setIsSavingTrust] = useState(false);
    const [isQrOpen, setIsQrOpen] = useState(false);

    const fileInputRef = useRef<HTMLInputElement>(null);
    const hadExistingContactRef = useRef<boolean>(false);
    const hadSiblingContactRef = useRef<boolean>(false);
    const lastImportedPayloadRef = useRef<string | null>(null);

    useEffect(() => {
        let active = true;
        if (isOpen) {
            marketService.getMyMarketProfile().then((profile) => {
                if (active) setMyProfile(profile);
            }).catch(() => {});
        }
        return () => {
            active = false;
        };
    }, [isOpen]);

    const resetState = () => {
        setPayloadText('');
        setIsImporting(false);
        setResultFeedback(null);
        setSelectedTrustLevel(null);
        setIsSavingTrust(false);
        setPartnerName(null);
        setSiblingContactName(null);
        hadExistingContactRef.current = false;
        hadSiblingContactRef.current = false;
        lastImportedPayloadRef.current = null;
    };

    const handleClose = () => {
        resetState();
        onClose();
    };

    const handleSetTrustLevel = async (level: number) => {
        if (!resultFeedback?.did) return;
        setIsSavingTrust(true);
        try {
            const did = resultFeedback.did;
            const existingContacts = (await contactService.getContacts().catch(() => [])) || [];
            const existing = existingContacts.find(c => isExactSameUserId(c.did, did));

            if (level > 0) {
                let profile = existing?.profile;
                let syncs = existing?.syncs;
                if (!profile) {
                    const snap = await marketService.getMarketView().catch(() => null);
                    const people = snap?.people ?? [];
                    const p = people.find((x) => isExactSameUserId(x.recipientDid || x.did, did) || isExactSameUserId(x.did, did));
                    profile = p?.profile;
                    syncs = p?.cardSyncs;
                }

                const defaultProfile = {
                    id: did,
                    firstName: partnerName || t('contacts.defaultContactName', 'Kontakt'),
                };

                const contactToSave: Contact = {
                    ...(existing || {}),
                    did,
                    profile: existing?.profile ? { ...existing.profile, ...(profile || {}), id: did } : (profile || defaultProfile),
                    tags: existing?.tags || [],
                    notes: existing?.notes,
                    addedAt: existing?.addedAt || new Date().toISOString(),
                    trustLevel: level,
                    syncs: (existing?.syncs && existing.syncs.length > 0 ? existing.syncs : syncs) || [],
                };

                await protectAction(async (pwd) => {
                    await contactService.saveContact(contactToSave, pwd || undefined);
                });

                marketService.syncMarketChannels().catch(() => {});
            } else if (existing) {
                if (!hadExistingContactRef.current && !hadSiblingContactRef.current) {
                    await protectAction(async (pwd) => {
                        await contactService.deleteContact(did, pwd || undefined);
                        if (lastImportedPayloadRef.current) {
                            await marketService.importContactCard(lastImportedPayloadRef.current, pwd || undefined);
                        }
                    });
                } else {
                    const contactToSave: Contact = {
                        ...existing,
                        trustLevel: null,
                        desiredHops: null,
                    };
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(contactToSave, pwd || undefined);
                        if (lastImportedPayloadRef.current) {
                            await marketService.importContactCard(lastImportedPayloadRef.current, pwd || undefined);
                        }
                    });
                }
            }
            setSelectedTrustLevel(level);
        } catch (err) {
            logger.error(`Failed to set contact trust level: ${stringifyError(err)}`);
            setResultFeedback({
                type: 'error',
                title: t('contacts.trustLevelSaveError', 'Fehler beim Speichern der Vertrauensstufe'),
                message: translateError(err, t),
            });
        } finally {
            setIsSavingTrust(false);
        }
    };

    const processImportPayload = async (rawPayload: string) => {
        const trimmed = rawPayload.trim();
        if (!trimmed) return;

        setIsImporting(true);
        setResultFeedback(null);

        try {
            let effectivePayload = trimmed;
            const parsedInput = parseHumocoInput(trimmed);
            if (parsedInput?.kind === 'card') {
                effectivePayload = parsedInput.payload;
            } else if (parsedInput?.kind === 'contact' || parsedInput?.kind === 'did') {
                const parsedContact =
                    parsedInput.kind === 'contact' ? parsedInput.contact : { did: parsedInput.did };
                if (parsedContact.did) {
                    const ownUid = (await Promise.resolve(profileService.getUserId?.()).catch(() => '')) || '';
                    if (isExactSameUserId(parsedContact.did, ownUid)) {
                        setResultFeedback({
                            type: 'error',
                            title: t('marketplace.importInvalidTitle'),
                            message: t('contacts.selfActiveProfileHint'),
                        });
                        return;
                    }
                    const existingContacts = (await contactService.getContacts().catch(() => [])) || [];
                    const existing = existingContacts.find(c => isExactSameUserId(c.did, parsedContact.did));
                    const sibling = !existing
                        ? existingContacts.find(c => isSameRootIdentity(c.did, parsedContact.did!))
                        : undefined;
                    hadExistingContactRef.current = Boolean(existing);
                    hadSiblingContactRef.current = Boolean(sibling);
                    setSiblingContactName(
                        sibling
                            ? [sibling.profile.firstName, sibling.profile.lastName].filter(Boolean).join(' ') ||
                                  sibling.profile.organization ||
                                  truncateUserId(sibling.did)
                            : null,
                    );
                    setSelectedTrustLevel(existing?.trustLevel ?? sibling?.trustLevel ?? null);
                    lastImportedPayloadRef.current = null;
                    let firstName = existing?.profile.firstName;
                    let lastName = existing?.profile.lastName;
                    if (!firstName && parsedContact.name) {
                        const parts = parsedContact.name.trim().split(/\s+/);
                        if (parts.length > 1) {
                            firstName = parts.slice(0, -1).join(' ');
                            lastName = parts[parts.length - 1];
                        } else if (parts[0]) {
                            firstName = parts[0];
                        }
                    }
                    const mappedSyncs = (parsedContact.syncUrls || []).map((url) => ({
                        readUrl: '',
                        writeUrl: url,
                        dropPub: '',
                        status: 'unverified',
                        failCount: 0,
                    }));
                    const mergedSyncs = existing?.syncs && existing.syncs.length > 0 ? existing.syncs : mappedSyncs;
                    const contactToSave: Contact = {
                        ...(existing || {}),
                        did: parsedContact.did,
                        profile: {
                            ...(existing?.profile || {}),
                            id: parsedContact.did,
                            firstName: firstName || t('contacts.defaultContactName', 'Kontakt'),
                            lastName,
                        },
                        tags: existing?.tags || [],
                        notes: existing?.notes,
                        addedAt: existing?.addedAt || new Date().toISOString(),
                        trustLevel: existing?.trustLevel ?? sibling?.trustLevel ?? undefined,
                        syncs: mergedSyncs,
                    };
                    await protectAction(async (pwd) => {
                        await contactService.saveContact(contactToSave, pwd || undefined);
                    });
                    marketService.probeContactSyncs?.(false)?.then(() => notifySyncStatusUpdated())?.catch(() => {});
                    setPartnerName([firstName, lastName].filter(Boolean).join(' ') || null);
                    setResultFeedback({
                        type: 'success',
                        title: t('marketplace.importSuccessTitle'),
                        message: t('contacts.contactSavedSuccess', 'Kontakt gespeichert.'),
                        did: parsedContact.did,
                    });
                    onImportSuccess(parsedContact.did);
                    return;
                }
            }

            lastImportedPayloadRef.current = effectivePayload;
            const res = await protectAction(async (pwd) => {
                return await marketService.importContactCard(effectivePayload, pwd || undefined);
            });

            if (!res) {
                return;
            }

            if (res.status === 'accepted') {
                if (res.did) {
                    const existingContacts = (await contactService.getContacts().catch(() => [])) || [];
                    const existing = existingContacts.find((c) => isExactSameUserId(c.did, res.did!));
                    const sibling = !existing
                        ? existingContacts.find((c) => isSameRootIdentity(c.did, res.did!))
                        : undefined;
                    hadExistingContactRef.current = Boolean(existing);
                    hadSiblingContactRef.current = Boolean(sibling);
                    setSiblingContactName(
                        sibling
                            ? [sibling.profile.firstName, sibling.profile.lastName].filter(Boolean).join(' ') ||
                                  sibling.profile.organization ||
                                  truncateUserId(sibling.did)
                            : null,
                    );
                    setSelectedTrustLevel(existing?.trustLevel ?? sibling?.trustLevel ?? null);
                } else {
                    hadExistingContactRef.current = false;
                    hadSiblingContactRef.current = false;
                    setSiblingContactName(null);
                }
                setResultFeedback({
                    type: 'success',
                    title: res.replaced
                        ? t('marketplace.importUpdatedTitle')
                        : t('marketplace.importSuccessTitle'),
                    message: res.replaced
                        ? t('marketplace.importUpdatedDesc')
                        : t('marketplace.importSuccessDesc'),
                    did: res.did,
                });
                onImportSuccess(res.did);

                if (res.did) {
                    marketService.getMarketView().then(snap => {
                        const people = snap?.people ?? [];
                        const p = people.find((x) => isExactSameUserId(x.recipientDid || x.did, res.did!) || isExactSameUserId(x.did, res.did!));
                        if (p) {
                            const name = [p.profile.firstName, p.profile.lastName].filter(Boolean).join(' ') || p.profile.organization || p.profile.community;
                            if (name) setPartnerName(name);
                        }
                    }).catch(() => {});
                }
            } else if (res.status === 'ignored_stale') {
                setResultFeedback({
                    type: 'warning',
                    title: t('marketplace.importIgnoredTitle'),
                    message: t('marketplace.importIgnoredDesc'),
                });
            } else if (res.status === 'rejected_expired') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importExpiredTitle'),
                    message: t('marketplace.importExpiredDesc'),
                });
            } else if (res.status === 'rejected_hops') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importHopsTitle'),
                    message: t('marketplace.importHopsDesc'),
                });
            } else if (res.status === 'rejected_offender') {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importOffenderTitle'),
                    message: t('marketplace.importOffenderDesc'),
                });
            } else {
                setResultFeedback({
                    type: 'error',
                    title: t('marketplace.importInvalidTitle'),
                    message: res.status,
                });
            }
        } catch (err) {
            logger.error(`Error importing contact card: ${stringifyError(err)}`);
            setResultFeedback({
                type: 'error',
                title: t('marketplace.importInvalidTitle'),
                message: stringifyError(err),
            });
        } finally {
            setIsImporting(false);
        }
    };

    const handleFilePicker = async () => {
        try {
            const fileResult = await openFileAsBytes({
                filters: [
                    { name: t('marketplace.cardFilterName'), extensions: ['hmcard', 'json'] },
                    { name: t('common.allFiles', 'All Files'), extensions: ['*'] },
                ],
            });

            if (fileResult) {
                const text = new TextDecoder().decode(new Uint8Array(fileResult.bytes));
                setPayloadText(text);
                await processImportPayload(text);
            }
        } catch (err) {
            logger.info(`Tauri file dialog not available, falling back to input: ${stringifyError(err)}`);
            fileInputRef.current?.click();
        }
    };

    const handleFileInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        try {
            const text = await file.text();
            setPayloadText(text);
            await processImportPayload(text);
        } catch (err) {
            setResultFeedback({
                type: 'error',
                title: t('marketplace.importInvalidTitle'),
                message: stringifyError(err),
            });
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={handleClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <Download size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">
                            {t('marketplace.importModalTitle')}
                        </h2>
                        <p className="text-xs text-theme-light">
                            {t('marketplace.importModalSubtitle')}
                        </p>
                    </div>
                </div>
            }
            footer={
                resultFeedback?.type === 'success' ? (
                    <div className="flex justify-end w-full">
                        <Button variant="primary" size="sm" onClick={handleClose}>
                            {t('common.done', 'Fertig')}
                        </Button>
                    </div>
                ) : (
                    <div className="flex items-center justify-between w-full">
                        <Button variant="secondary" size="sm" onClick={handleClose}>
                            {t('common.close')}
                        </Button>

                        <Button
                            variant="primary"
                            size="sm"
                            onClick={() => processImportPayload(payloadText)}
                            disabled={isImporting || !payloadText.trim()}
                            className="gap-1.5 shadow-sm"
                        >
                            <Download size={14} />
                            <span>{isImporting ? t('common.processing') : t('marketplace.importActionBtn')}</span>
                        </Button>
                    </div>
                )
            }
        >
            <div className="space-y-4 pt-2">
                {resultFeedback?.type === 'success' ? (
                    /* Step 2: Confirmation, 1-Click Trust Level & Counter-Scan QR */
                    <div className="space-y-4 animate-in fade-in duration-200">
                        <div className="p-3.5 rounded-2xl border flex items-start gap-3 text-xs bg-emerald-50 text-emerald-900 border-emerald-200">
                            <CheckCircle2 size={18} className="text-emerald-600 shrink-0 mt-0.5" />
                            <div className="space-y-1">
                                <h4 className="font-bold">{resultFeedback.title}</h4>
                                <p className="leading-relaxed opacity-90">{resultFeedback.message}</p>
                            </div>
                        </div>

                        {/* 1-Click Trust Level Selection */}
                        <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-2.5">
                            {siblingContactName && (
                                <div
                                    data-testid="import-sibling-prefix-trust-notice"
                                    className="p-2.5 rounded-xl bg-sky-50 border border-sky-200 text-xs font-medium text-sky-900 flex items-start gap-2"
                                >
                                    <AlertCircle size={15} className="shrink-0 mt-0.5 text-sky-600" />
                                    <span>{t('contacts.siblingPrefixTrustHint', { name: siblingContactName })}</span>
                                </div>
                            )}
                            <span className="text-xs font-bold text-theme-secondary block">
                                {t('marketplace.trustLevelSelectTitle', 'In mein Adressbuch aufnehmen als:')}
                            </span>
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    type="button"
                                    variant={selectedTrustLevel === 2 ? 'primary' : 'outline'}
                                    size="sm"
                                    onClick={() => handleSetTrustLevel(2)}
                                    disabled={isSavingTrust}
                                    className="text-xs gap-1.5"
                                >
                                    {selectedTrustLevel === 2 && <Check size={13} />}
                                    <span>{t('marketplace.trustLevel2Btn', 'Vertraut (Stufe 2 – Empfohlen)')}</span>
                                </Button>
                                <Button
                                    type="button"
                                    variant={selectedTrustLevel === 1 ? 'primary' : 'outline'}
                                    size="sm"
                                    onClick={() => handleSetTrustLevel(1)}
                                    disabled={isSavingTrust}
                                    className="text-xs gap-1.5"
                                >
                                    {selectedTrustLevel === 1 && <Check size={13} />}
                                    <span>{t('marketplace.trustLevel1Btn', 'Bekannt (Stufe 1)')}</span>
                                </Button>
                                <Button
                                    type="button"
                                    variant={selectedTrustLevel === 0 ? 'secondary' : 'outline'}
                                    size="sm"
                                    onClick={() => handleSetTrustLevel(0)}
                                    disabled={isSavingTrust}
                                    className="text-xs text-theme-light gap-1.5"
                                >
                                    {selectedTrustLevel === 0 && <Check size={13} />}
                                    <span>{t('marketplace.trustLevelNoneBtn', 'Nur im Marktplatz merken')}</span>
                                </Button>
                            </div>
                        </div>

                        {/* Counter-Scan QR Section */}
                        {myProfile?.published && myProfile?.qrSvg && (
                            <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle text-center space-y-2">
                                <span className="text-xs font-bold text-theme-secondary block">
                                    {t('marketplace.counterScanTitle', {
                                        name: partnerName || (resultFeedback.did ? truncateUserId(resultFeedback.did) : 'deinem Gegenüber'),
                                        defaultValue: `Zeige ${partnerName || 'deinem Gegenüber'} kurz diesen Code für den beidseitigen Abschluss`,
                                    })}
                                </span>
                                <p className="text-[11px] text-theme-light">
                                    {t('marketplace.counterScanHint', 'Sobald dein Gegenüber deinen Code scannt, ist die Verbindung vollständig.')}
                                </p>
                                <img
                                    src={`data:image/svg+xml;utf8,${encodeURIComponent(myProfile.qrSvg)}`}
                                    alt="HuMoCo Counter QR Code"
                                    className="w-48 h-48 mx-auto rounded-xl p-2 bg-white shadow-xs border border-theme-subtle object-contain"
                                />
                            </div>
                        )}
                    </div>
                ) : (
                    /* Step 1: Input / File selection */
                    <div className="space-y-4">
                        {/* Error or Warning Banner */}
                        {resultFeedback && (
                            <div
                                className={`p-3 rounded-2xl border flex items-start gap-3 text-xs ${
                                    resultFeedback.type === 'warning'
                                        ? 'bg-amber-50 text-amber-900 border-amber-200'
                                        : 'bg-rose-50 text-rose-900 border-rose-200'
                                }`}
                            >
                                {resultFeedback.type === 'warning' ? (
                                    <Info size={16} className="text-amber-600 shrink-0 mt-0.5" />
                                ) : (
                                    <AlertCircle size={16} className="text-rose-600 shrink-0 mt-0.5" />
                                )}
                                <div className="space-y-1">
                                    <h4 className="font-bold">{resultFeedback.title}</h4>
                                    <p className="leading-relaxed opacity-90">{resultFeedback.message}</p>
                                </div>
                            </div>
                        )}

                        {/* File Upload & QR Option */}
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                            <input
                                ref={fileInputRef}
                                type="file"
                                accept=".hmcard,application/json"
                                onChange={handleFileInputChange}
                                className="hidden"
                            />
                            <Button
                                type="button"
                                variant="outline"
                                onClick={handleFilePicker}
                                disabled={isImporting}
                                className="w-full h-auto py-3 px-4 flex items-center justify-center gap-2 border-dashed border-2 border-theme-subtle hover:border-theme-primary/40 bg-bg-app rounded-2xl text-xs font-bold text-theme-secondary cursor-pointer"
                            >
                                <Upload size={16} className="text-theme-primary" />
                                <span>{t('marketplace.chooseCardFileBtn')}</span>
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => setIsQrOpen(true)}
                                disabled={isImporting}
                                data-testid="import-card-scan-qr-btn"
                                className="w-full h-auto py-3 px-4 flex items-center justify-center gap-2 border-dashed border-2 border-theme-subtle hover:border-theme-primary/40 bg-bg-app rounded-2xl text-xs font-bold text-theme-secondary cursor-pointer"
                            >
                                <QrCode size={16} className="text-theme-primary" />
                                <span>{t('common.scanQrCode')}</span>
                            </Button>
                        </div>

                        <div className="relative flex items-center justify-center">
                            <div className="border-t border-theme-subtle w-full"></div>
                            <span className="bg-white px-3 text-[10px] font-bold text-theme-light uppercase tracking-wider absolute">
                                {t('common.or')}
                            </span>
                        </div>

                        {/* Paste Link or JSON */}
                        <div className="space-y-1.5">
                            <label className="block text-[11px] font-bold text-theme-secondary flex items-center gap-1.5">
                                <FileText size={13} className="text-theme-primary" />
                                <span>{t('marketplace.pasteCardPayloadLabel')}</span>
                            </label>
                            <Textarea
                                value={payloadText}
                                onChange={(e) => setPayloadText(e.target.value)}
                                placeholder={t('marketplace.pasteCardPayloadPlaceholder')}
                                rows={4}
                                className="text-xs font-mono bg-bg-app"
                            />
                            <p className="text-[10px] text-theme-light">
                                {t('marketplace.pasteCardPayloadHint')}
                            </p>
                        </div>
                    </div>
                )}
            </div>

            <QrScannerModal
                isOpen={isQrOpen}
                onClose={() => setIsQrOpen(false)}
                onScan={(decoded) => {
                    setPayloadText(decoded);
                    void processImportPayload(decoded);
                }}
            />
        </ModalShell>
    );
}
