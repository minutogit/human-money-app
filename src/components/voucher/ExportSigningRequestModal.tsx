import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { voucherService, SigningRequestConfig } from "../../services/voucherService";
import { postService } from "../../services/postService";
import { marketService } from "../../services/marketService";
import { settingsService } from "../../services/settingsService";
import { contactService } from "../../services/contactService";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../../utils/log";
import { translateError } from "../../utils/errorHelper";
import { ConfirmationModal } from "../ui/ConfirmationModal";
import { Button } from "../ui/Button";
import { AppConfigDto, Contact } from "../../types";
import { updateLastUsedDirectory } from "../../utils/settingsUtils";
import { normalizeUserIdInput, parseHumocoInput } from "../../utils/humocoUri";
import { RefreshCw, UploadCloud, CheckCircle2 } from "lucide-react";

interface ExportSigningRequestModalProps {
    isOpen: boolean;
    voucherId: string;
    onClose: () => void;
    config: AppConfigDto | null;
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>;
    onConfigRefresh: (config: AppConfigDto) => void;
    contacts?: Contact[];
    onSuccess?: (savedFilePath: string) => void;
}

export function ExportSigningRequestModal({
    isOpen,
    voucherId,
    onClose,
    config,
    protectAction,
    onConfigRefresh,
    contacts,
    onSuccess
}: ExportSigningRequestModalProps) {
    const { t } = useTranslation();
    const [recipientId, setRecipientId] = useState("");
    const [encryptToDid, setEncryptToDid] = useState(true);
    const [protectWithPassword, setProtectWithPassword] = useState(false);
    const [exportPassword, setExportPassword] = useState("");
    const [exportPasswordConfirm, setExportPasswordConfirm] = useState("");
    const [showExportPassword, setShowExportPassword] = useState(false);
    const [isExporting, setIsExporting] = useState(false);
    const [isSendingViaPost, setIsSendingViaPost] = useState(false);
    const [isResolvingCard, setIsResolvingCard] = useState(false);
    const [postSuccessMsg, setPostSuccessMsg] = useState("");
    const [exportError, setExportError] = useState("");
    const [availableContacts, setAvailableContacts] = useState<Contact[]>(contacts || []);
    const isMountedRef = useRef(true);
    const cardResolveGenRef = useRef(0);

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
            cardResolveGenRef.current += 1;
        };
    }, []);

    useEffect(() => {
        if (!isOpen) {
            cardResolveGenRef.current += 1;
            setIsResolvingCard(false);
        }
    }, [isOpen]);

    useEffect(() => {
        if (contacts && contacts.length > 0) {
            setAvailableContacts(contacts);
        } else if (isOpen) {
            contactService.getContacts()
                .then(setAvailableContacts)
                .catch(() => {});
        }
    }, [contacts, isOpen]);

    const resolveTargetRecipientId = async (raw: string): Promise<string | null> => {
        const parsed = parseHumocoInput(raw);
        if (parsed?.kind === 'card') {
            const resolved = await marketService.resolveCardRecipient(parsed.payload);
            return resolved.recipientId;
        }
        return normalizeUserIdInput(raw) ?? (raw.trim() || null);
    };

    const handleExportSigningRequest = async () => {
        if (!voucherId || isResolvingCard) return;
        setIsExporting(true);
        setExportError("");
        try {
            let requestConfig: SigningRequestConfig;
            if (encryptToDid) {
                const normalizedDid = await resolveTargetRecipientId(recipientId);
                if (!normalizedDid) {
                    setExportError(t('voucher.errorEnterRecipientDid'));
                    setIsExporting(false);
                    return;
                }
                requestConfig = { type: "TargetDid", value: [normalizedDid, "TrialDecryption"] };
            } else if (protectWithPassword) {
                if (!exportPassword) {
                    setExportError(t('voucher.errorEnterPassword'));
                    setIsExporting(false);
                    return;
                }
                if (!exportPasswordConfirm) {
                    setExportError(t('voucher.export.errorNoConfirm'));
                    setIsExporting(false);
                    return;
                }
                if (exportPassword !== exportPasswordConfirm) {
                    setExportError(t('voucher.export.errorPasswordMismatch'));
                    setIsExporting(false);
                    return;
                }
                requestConfig = { type: "Password", value: exportPassword };
            } else {
                requestConfig = { type: "Cleartext" };
            }

            const bundleBytes = await protectAction(async (pwd) => {
                return await voucherService.createSigningRequest(voucherId, requestConfig, pwd || undefined);
            });
            if (!bundleBytes) return;

            const filePath = await save({
                defaultPath: config?.uiPreferences.lastUsedDirectory
                    ? `${config.uiPreferences.lastUsedDirectory}/signature-request-${voucherId.slice(0, 8)}.ask`
                    : `signature-request-${voucherId.slice(0, 8)}.ask`,
                filters: [
                    { name: t('voucher.export.fileFilter'), extensions: ['ask'] },
                    { name: t('voucher.export.allFiles'), extensions: ['*'] }
                ]
            });

            if (filePath) {
                const uint8Array = new Uint8Array(bundleBytes);
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, uint8Array);

                if (config) {
                    await updateLastUsedDirectory(filePath, config.uiPreferences, protectAction);
                    const freshConfig = await settingsService.getConfig().catch(() => null);
                    if (freshConfig) {
                        onConfigRefresh(freshConfig);
                    }
                }

                if (onSuccess) {
                    onSuccess(filePath);
                }

                resetAndClose();
            }
        } catch (e) {
            const msg = t('voucher.export.failed', { error: translateError(e, t) });
            logger.error(msg);
            setExportError(msg);
        } finally {
            setIsExporting(false);
        }
    };

    const handleSendSigningRequestViaPost = async () => {
        if (!voucherId || isResolvingCard) return;
        let normalizedDid: string | null = null;
        try {
            normalizedDid = await resolveTargetRecipientId(recipientId);
        } catch (e) {
            setExportError(translateError(e, t));
            return;
        }
        if (!normalizedDid) {
            setExportError(t('voucher.errorEnterRecipientDid'));
            return;
        }
        setIsSendingViaPost(true);
        setExportError("");
        setPostSuccessMsg("");
        try {
            const config: SigningRequestConfig = {
                type: "TargetDid",
                value: [normalizedDid, "TrialDecryption"]
            };
            const bundleBytes = await protectAction(async (pwd) => {
                return await voucherService.createSigningRequest(voucherId, config, pwd || undefined);
            });
            if (!bundleBytes) return;
            const res = await protectAction(async (pwd) => {
                return await postService.sendSigningRequestViaPost(
                    normalizedDid,
                    bundleBytes,
                    pwd || undefined,
                );
            });
            const recipientDelivered = res?.recipientDepositedTo ?? 0;
            if (recipientDelivered > 0) {
                setPostSuccessMsg(t('voucher.signRequest.sentViaPostSuccess', { count: recipientDelivered }));
                setTimeout(() => {
                    resetAndClose();
                }, 1600);
            } else if (res && res.errors && res.errors.length > 0) {
                setExportError(res.errors.join('; '));
            } else {
                setExportError(t('voucher.signRequest.sentViaPostZeroSyncs'));
            }
        } catch (e) {
            setExportError(translateError(e, t));
        } finally {
            setIsSendingViaPost(false);
        }
    };

    const resetAndClose = () => {
        cardResolveGenRef.current += 1;
        setIsResolvingCard(false);
        setRecipientId("");
        setExportPassword("");
        setExportPasswordConfirm("");
        setExportError("");
        setPostSuccessMsg("");
        onClose();
    };

    return (
        <ConfirmationModal
            isOpen={isOpen}
            title={t('voucher.export.title')}
            description={
                <div className="space-y-6 pt-2">
                    <div className="flex items-center gap-3 p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20">
                        <input
                            type="checkbox"
                            id="wallet-encryptToDid"
                            checked={encryptToDid}
                            onChange={(e) => setEncryptToDid(e.target.checked)}
                            className="h-5 w-5 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
                        />
                        <label htmlFor="wallet-encryptToDid" className="text-sm font-bold text-theme-secondary">
                            {t('voucher.export.encryptForContact')}
                        </label>
                    </div>

                    {encryptToDid ? (
                        <div className="space-y-3 animate-in slide-in-from-top-2 duration-300">
                            {availableContacts.length > 0 && (
                                <div className="space-y-1">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                                        {t('voucher.export.selectFromContacts')}
                                    </label>
                                    <select
                                        className="w-full px-4 py-2.5 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-xs font-semibold"
                                        value={availableContacts.some(c => c.did === recipientId) ? recipientId : ""}
                                        onChange={(e) => {
                                            if (e.target.value) {
                                                cardResolveGenRef.current += 1;
                                                setIsResolvingCard(false);
                                                setExportError("");
                                                setRecipientId(e.target.value);
                                            }
                                        }}
                                    >
                                        <option value="">{t('voucher.export.chooseContactPlaceholder')}</option>
                                        {availableContacts.map(c => {
                                            const name = [c.profile?.firstName, c.profile?.lastName].filter(Boolean).join(" ") || c.profile?.organization;
                                            return (
                                                <option key={c.did} value={c.did}>
                                                    {name ? `${name} (${c.did.slice(0, 15)}...)` : c.did}
                                                </option>
                                            );
                                        })}
                                    </select>
                                </div>
                            )}
                            <div className="space-y-1">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">{t('voucher.export.signerDid')}</label>
                                <input
                                    type="text"
                                    value={recipientId}
                                    onChange={(e) => {
                                        const rawVal = e.target.value;
                                        const parsed = parseHumocoInput(rawVal);
                                        if (parsed?.kind === 'card') {
                                            const resolveGen = ++cardResolveGenRef.current;
                                            setRecipientId(rawVal);
                                            setIsResolvingCard(true);
                                            setExportError("");
                                            void marketService
                                                .resolveCardRecipient(parsed.payload)
                                                .then((resolved) => {
                                                    if (!isMountedRef.current || cardResolveGenRef.current !== resolveGen) {
                                                        return;
                                                    }
                                                    setExportError("");
                                                    setRecipientId(resolved.recipientId);
                                                })
                                                .catch((err) => {
                                                    if (!isMountedRef.current || cardResolveGenRef.current !== resolveGen) {
                                                        return;
                                                    }
                                                    setExportError(translateError(err, t));
                                                })
                                                .finally(() => {
                                                    if (isMountedRef.current && cardResolveGenRef.current === resolveGen) {
                                                        setIsResolvingCard(false);
                                                    }
                                                });
                                            return;
                                        }
                                        cardResolveGenRef.current += 1;
                                        setIsResolvingCard(false);
                                        const normalized = normalizeUserIdInput(rawVal);
                                        setRecipientId(normalized ?? rawVal);
                                    }}
                                    placeholder={t('contacts.didPlaceholder')}
                                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                    autoFocus
                                />
                            </div>

                            <div className="pt-2">
                                <Button
                                    type="button"
                                    variant="primary"
                                    disabled={!recipientId.trim() || isResolvingCard || isSendingViaPost || isExporting}
                                    onClick={handleSendSigningRequestViaPost}
                                    data-testid="send-signing-request-post-button"
                                    className="w-full gap-2 text-xs font-bold py-3 rounded-xl shadow-sm"
                                >
                                    {isSendingViaPost ? <RefreshCw size={14} className="animate-spin" /> : <UploadCloud size={14} />}
                                    {isSendingViaPost ? t('voucher.signRequest.sendingViaPost') : t('voucher.signRequest.sendViaPostButton')}
                                </Button>
                            </div>
                        </div>
                    ) : (

                        <div className="space-y-4 animate-in slide-in-from-top-2 duration-300">
                            <div className="flex items-center gap-3 p-4 bg-theme-accent/5 rounded-2xl border border-theme-accent/20">
                                <input
                                    type="checkbox"
                                    id="wallet-protectWithPassword"
                                    checked={protectWithPassword}
                                    onChange={(e) => setProtectWithPassword(e.target.checked)}
                                    className="h-5 w-5 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent transition-all"
                                />
                                <label htmlFor="wallet-protectWithPassword" className="text-sm font-bold text-theme-secondary">
                                    {t('voucher.export.protectWithPassword')}
                                </label>
                            </div>

                            {protectWithPassword && (
                                <div className="space-y-4 animate-in slide-in-from-top-2 duration-300">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">{t('auth.password')}</label>
                                        <div className="relative">
                                            <input
                                                type={showExportPassword ? "text" : "password"}
                                                value={exportPassword}
                                                onChange={(e) => { setExportPassword(e.target.value); setExportError(""); }}
                                                placeholder={t('voucher.export.passwordPlaceholder')}
                                                className="w-full px-4 py-3 pr-20 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                            />
                                            <button
                                                type="button"
                                                onClick={() => setShowExportPassword(!showExportPassword)}
                                                className="absolute right-3 top-1/2 -translate-y-1/2 text-xs font-bold text-theme-light hover:text-theme-primary transition-colors"
                                            >
                                                {showExportPassword ? t('voucher.export.hide') : t('voucher.export.show')}
                                            </button>
                                        </div>
                                    </div>
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">{t('auth.confirmPassword')}</label>
                                        <input
                                            type={showExportPassword ? "text" : "password"}
                                            value={exportPasswordConfirm}
                                            onChange={(e) => { setExportPasswordConfirm(e.target.value); setExportError(""); }}
                                            placeholder={t('voucher.export.confirmPasswordPlaceholder')}
                                            className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                        />
                                    </div>
                                </div>
                            )}
                        </div>
                    )}
                    {postSuccessMsg && (
                        <div className="p-3 bg-emerald-50 text-emerald-700 rounded-xl text-xs font-bold border border-emerald-200 flex items-center gap-2 animate-in fade-in">
                            <CheckCircle2 size={16} className="shrink-0 text-emerald-600" />
                            <span>{postSuccessMsg}</span>
                        </div>
                    )}
                    {exportError && (
                        <div className="p-3 bg-rose-50 text-rose-600 rounded-xl text-xs font-bold border border-rose-100 animate-in shake duration-500">
                            {exportError}
                        </div>
                    )}
                </div>
            }
            confirmText={t('voucher.export.exportRequest')}
            onConfirm={handleExportSigningRequest}
            onCancel={resetAndClose}
            isProcessing={isExporting}
            confirmDisabled={isResolvingCard}
        />
    );
}
