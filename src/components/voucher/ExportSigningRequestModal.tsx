// src/components/voucher/ExportSigningRequestModal.tsx
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { voucherService, SigningRequestConfig } from "../../services/voucherService";
import { settingsService } from "../../services/settingsService";
import { save } from "@tauri-apps/plugin-dialog";
import { logger } from "../../utils/log";
import { translateError } from "../../utils/errorHelper";
import { ConfirmationModal } from "../ui/ConfirmationModal";
import { AppSettings } from "../../types";
import { updateLastUsedDirectory } from "../../utils/settingsUtils";

interface ExportSigningRequestModalProps {
    isOpen: boolean;
    voucherId: string;
    onClose: () => void;
    settings: AppSettings | null;
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>;
    onSettingsRefresh: (settings: AppSettings) => void;
}

export function ExportSigningRequestModal({
    isOpen,
    voucherId,
    onClose,
    settings,
    protectAction,
    onSettingsRefresh
}: ExportSigningRequestModalProps) {
    const { t } = useTranslation();
    const [recipientId, setRecipientId] = useState("");
    const [encryptToDid, setEncryptToDid] = useState(true);
    const [protectWithPassword, setProtectWithPassword] = useState(false);
    const [exportPassword, setExportPassword] = useState("");
    const [exportPasswordConfirm, setExportPasswordConfirm] = useState("");
    const [showExportPassword, setShowExportPassword] = useState(false);
    const [isExporting, setIsExporting] = useState(false);
    const [exportError, setExportError] = useState("");

    const handleExportSigningRequest = async () => {
        if (!voucherId) return;
        setIsExporting(true);
        setExportError("");
        try {
            let config;
            if (encryptToDid) {
                if (!recipientId.trim()) {
                    setExportError(t('voucher.errorEnterRecipientDid'));
                    setIsExporting(false);
                    return;
                }
                config = { type: "TargetDid", value: [recipientId.trim(), "TrialDecryption"] };
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
                config = { type: "Password", value: exportPassword };
            } else {
                config = { type: "Cleartext" };
            }

            const bundleBytes = await voucherService.createSigningRequest(voucherId, config as SigningRequestConfig);

            const filePath = await save({
                defaultPath: settings?.lastUsedDirectory 
                    ? `${settings.lastUsedDirectory}/signature-request-${voucherId.slice(0, 8)}.ask`
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
                
                if (settings) {
                    await updateLastUsedDirectory(filePath, settings, protectAction);
                    const newSettings = await settingsService.getSettings();
                    onSettingsRefresh(newSettings);
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

    const resetAndClose = () => {
        setRecipientId("");
        setExportPassword("");
        setExportPasswordConfirm("");
        setExportError("");
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
                        <div className="space-y-2 animate-in slide-in-from-top-2 duration-300">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">{t('voucher.export.signerDid')}</label>
                            <input
                                type="text"
                                value={recipientId}
                                onChange={(e) => setRecipientId(e.target.value)}
                                placeholder={t('contacts.didPlaceholder')}
                                className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary shadow-inner-soft transition-all"
                                autoFocus
                            />
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
        />
    );
}
