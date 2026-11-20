// src/components/transfer/receive/ToleranceConfirmationModal.tsx
import { useTranslation } from 'react-i18next';
import { ShieldAlert } from 'lucide-react';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import { Input } from '../../ui/Input';

interface ToleranceConfirmationModalProps {
    isOpen: boolean;
    zone: 'Soft' | 'Critical';
    message: string;
    hasConflict?: boolean;
    confirmText: string;
    onConfirmTextChange: (val: string) => void;
    onConfirm: () => void;
    onCancel: () => void;
    isProcessing: boolean;
}

export function ToleranceConfirmationModal({
    isOpen,
    zone,
    message,
    hasConflict = false,
    confirmText,
    onConfirmTextChange,
    onConfirm,
    onCancel,
    isProcessing,
}: ToleranceConfirmationModalProps) {
    const { t } = useTranslation();
    const isSoft = zone === 'Soft';

    return (
        <ConfirmationModal
            isOpen={isOpen}
            title={
                isSoft
                    ? t('transfer.importFile.syncFromBackup')
                    : t('transfer.importFile.criticalConflict')
            }
            confirmVariant="danger"
            description={
                <div className="space-y-6 pt-2">
                    <div className="p-4 bg-rose-50 border border-rose-200 rounded-2xl flex items-start gap-3">
                        <ShieldAlert size={20} className="text-rose-600 shrink-0 mt-0.5" />
                        <div className="space-y-1.5">
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                {message}
                            </p>
                            {hasConflict && (
                                <p className="text-xs font-bold text-rose-800">
                                    {t('dashboard.pendingToleranceConflictHint')}
                                </p>
                            )}
                        </div>
                    </div>

                    {isSoft ? (
                        <label className="flex items-center gap-4 p-4 bg-white border border-theme-subtle rounded-2xl cursor-pointer group hover:border-theme-accent/40 transition-all">
                            <input
                                type="checkbox"
                                className="w-6 h-6 rounded-lg border-theme-subtle text-theme-accent focus:ring-theme-accent"
                                checked={confirmText === 'checked'}
                                onChange={(e) =>
                                    onConfirmTextChange(e.target.checked ? 'checked' : '')
                                }
                            />
                            <span className="text-sm font-bold text-theme-secondary select-none">
                                {t('transfer.importFile.acceptResponsibility')}
                            </span>
                        </label>
                    ) : (
                        <div className="space-y-3">
                            <p className="text-[10px] font-black text-rose-500 uppercase tracking-[0.2em]">
                                {t('transfer.importFile.mandatoryAffirmation')}
                            </p>
                            <Input
                                className="border-2 border-rose-500/30 focus:border-rose-500 uppercase font-mono tracking-widest text-center py-4"
                                placeholder={t('transfer.importFile.typeToConfirm')}
                                value={confirmText}
                                onChange={(e) => onConfirmTextChange(e.target.value)}
                            />
                        </div>
                    )}
                </div>
            }
            confirmText={t('transfer.importFile.importAnyway')}
            onConfirm={onConfirm}
            onCancel={onCancel}
            isProcessing={isProcessing}
            confirmDisabled={
                isSoft ? confirmText !== 'checked' : confirmText.toUpperCase() !== 'IMPORT'
            }
        />
    );
}
