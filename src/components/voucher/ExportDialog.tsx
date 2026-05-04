import { useState } from 'react';
import { ConfirmationModal } from '../ui/ConfirmationModal';

interface ExportDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onExport: (config: any) => void;
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
        setLocalError("Please enter a recipient DID.");
        return;
      }
      config = { type: "TargetDid", value: [recipientId.trim(), "TrialDecryption"] };
    } else if (protectWithPassword) {
      if (!exportPassword) {
        setLocalError("Please enter a password.");
        return;
      }
      if (exportPassword !== exportPasswordConfirm) {
        setLocalError("Passwords do not match.");
        return;
      }
      config = { type: "Password", value: exportPassword };
    } else {
      config = { type: "Cleartext" };
    }
    onExport(config);
  };

  return (
    <ConfirmationModal
      isOpen={isOpen}
      title="Send for Signature"
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
                Identity-Based Encryption
              </label>
            </div>
            {encryptToDid && (
              <div className="animate-in slide-in-from-top-2">
                <input
                  type="text"
                  value={recipientId}
                  onChange={(e) => setRecipientId(e.target.value)}
                  placeholder="Enter Signer DID (did:key:...)"
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
                  Password Protection
                </label>
              </div>
              {protectWithPassword && (
                <div className="space-y-3 animate-in slide-in-from-top-2">
                  <input
                    type="password"
                    value={exportPassword}
                    onChange={(e) => setExportPassword(e.target.value)}
                    placeholder="Access Password"
                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                  />
                  <input
                    type="password"
                    value={exportPasswordConfirm}
                    onChange={(e) => setExportPasswordConfirm(e.target.value)}
                    placeholder="Confirm Access Password"
                    className="w-full px-4 py-3 border border-theme-subtle rounded-xl bg-white text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-accent/20 shadow-inner-soft text-sm"
                  />
                </div>
              )}
            </div>
          )}
          {(error || localError) && <p className="text-rose-500 text-xs font-bold px-2">{error || localError}</p>}
        </div>
      }
      confirmText="Create Signature File"
      onConfirm={handleConfirm}
      onCancel={onClose}
      isProcessing={isProcessing}
    />
  );
}
