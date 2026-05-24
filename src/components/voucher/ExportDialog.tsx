import { useState } from 'react';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { useTranslation } from 'react-i18next';

import { SigningRequestConfig } from '../../services/voucherService';

interface ExportDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onExport: (config: SigningRequestConfig) => void;
  isProcessing: boolean;
  error: string;
}

export function ExportDialog({
  isOpen,
  onClose,
  onExport,
  isProcessing,
  error
}: ExportDialogProps) {
  const { t } = useTranslation();
  const [recipientId, setRecipientId] = useState("");
  const [encryptToDid, setEncryptToDid] = useState(true);
  const [protectWithPassword, setProtectWithPassword] = useState(false);
  const [exportPassword, setExportPassword] = useState("");
  const [exportPasswordConfirm, setExportPasswordConfirm] = useState("");
  const [localError, setLocalError] = useState("");

  const handleConfirm = () => {
    setLocalError("");
    let config;
    if (encryptToDid) {
      if (!recipientId.trim()) {
        setLocalError(t('voucher.errorEnterRecipientDid'));
        return;
      }
      config = { type: "TargetDid" as const, value: [recipientId.trim(), "TrialDecryption"] as [string, string] };
    } else if (protectWithPassword) {
      if (!exportPassword) {
        setLocalError(t('voucher.errorEnterPassword'));
        return;
      }
      if (exportPassword !== exportPasswordConfirm) {
        setLocalError(t('auth.passwordsDontMatch'));
        return;
      }
      config = { type: "Password" as const, value: exportPassword };
    } else {
      config = { type: "Cleartext" as const };
    }
    onExport(config);
  };

  return (
    <ConfirmationModal
      isOpen={isOpen}
      title={t('voucher.sendForSignatureHeader')}
      description={
        <div className="space-y-6 pt-2 text-left">
          <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20 space-y-4">
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                id="encryptToDid"
                checked={encryptToDid}
                onChange={(e) => setEncryptToDid(e.target.checked)}
                className="h-5 w-5 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
              />
              <label htmlFor="encryptToDid" className="text-sm font-bold text-theme-secondary cursor-pointer select-none">
                {t('voucher.identityBasedEncryption')}
              </label>
            </div>
            {encryptToDid && (
              <div className="animate-in slide-in-from-top-2">
                <input
                  type="text"
                  value={recipientId}
                  onChange={(e) => setRecipientId(e.target.value)}
                  placeholder={t('voucher.signerDidPlaceholder')}
                  className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shadow-inner-soft text-sm"
                  autoFocus
                />
              </div>
            )}
          </div>
          
          {!encryptToDid && (
            <div className="p-4 bg-theme-accent/5 rounded-2xl border border-theme-accent/20 space-y-4">
              <div className="flex items-center gap-3">
                <input
                  type="checkbox"
                  id="protectWithPassword"
                  checked={protectWithPassword}
                  onChange={(e) => setProtectWithPassword(e.target.checked)}
                  className="h-5 w-5 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent transition-all"
                />
                <label htmlFor="protectWithPassword" className="text-sm font-bold text-theme-secondary cursor-pointer select-none">
                  {t('voucher.passwordProtection')}
                </label>
              </div>
              {protectWithPassword && (
                <div className="space-y-3 animate-in slide-in-from-top-2">
                  <input
                    type="password"
                    value={exportPassword}
                    onChange={(e) => setExportPassword(e.target.value)}
                    placeholder={t('voucher.accessPasswordPlaceholder')}
                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                  />
                  <input
                    type="password"
                    value={exportPasswordConfirm}
                    onChange={(e) => setExportPasswordConfirm(e.target.value)}
                    placeholder={t('voucher.confirmAccessPasswordPlaceholder')}
                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                  />
                </div>
              )}
            </div>
          )}
          {(error || localError) && <p className="text-rose-500 text-xs font-bold px-2">{error || localError}</p>}
        </div>
      }
      confirmText={t('voucher.createSignatureFileBtn')}
      onConfirm={handleConfirm}
      onCancel={onClose}
      isProcessing={isProcessing}
    />
  );
}
