import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { standardsService } from '../services/standardsService';
import { VoucherStandardDefinition } from '../types';
import { Input } from './ui/Input';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { CheckCircle2, Lock, ShieldCheck } from 'lucide-react';
import { translateError } from '../utils/errorHelper';

interface ImportStandardModalProps {
    isOpen: boolean;
    containerBytes: number[];
    initialDefinition?: VoucherStandardDefinition | null;
    onClose: () => void;
    onSuccess: (standardId: string) => void;
}

export function ImportStandardModal({
    isOpen,
    containerBytes,
    initialDefinition,
    onClose,
    onSuccess
}: ImportStandardModalProps) {
    const { t } = useTranslation();
    const [definition, setDefinition] = useState<VoucherStandardDefinition | null>(initialDefinition || null);
    const [password, setPassword] = useState('');
    const [needsPassword, setNeedsPassword] = useState(!initialDefinition);
    const [isInspecting, setIsInspecting] = useState(false);
    const [isImporting, setIsImporting] = useState(false);
    const [error, setError] = useState('');

    const handleInspectWithPassword = async () => {
        setError('');
        setIsInspecting(true);
        try {
            const def = await standardsService.inspectStandardFile(containerBytes, password);
            setDefinition(def);
            setNeedsPassword(false);
        } catch (e) {
            setError(translateError(e, t));
        } finally {
            setIsInspecting(false);
        }
    };

    const handleConfirmImport = async () => {
        setError('');
        setIsImporting(true);
        try {
            const importedId = await standardsService.importStandard(containerBytes, password || null);
            onSuccess(importedId);
            onClose();
        } catch (e) {
            const errorMsg = translateError(e, t);
            if (errorMsg.includes("Password required") || errorMsg.includes("MacError")) {
                setNeedsPassword(true);
                setError(t('transfer.importFile.passwordHint'));
            } else {
                setError(errorMsg);
            }
        } finally {
            setIsImporting(false);
        }
    };

    const rulesCount = definition?.immutable?.customRules 
        ? Object.keys(definition.immutable.customRules).length 
        : 0;

    return (
        <ConfirmationModal
            isOpen={isOpen}
            title={t('standards.importModalTitle')}
            onCancel={onClose}
            onConfirm={needsPassword ? handleInspectWithPassword : handleConfirmImport}
            confirmText={needsPassword ? t('common.preview') : t('standards.confirmImport')}
            isProcessing={isInspecting || isImporting}
            description={
                <div className="space-y-6 pt-2 text-left">
                    {needsPassword ? (
                        <div className="space-y-4">
                            <div className="p-4 bg-amber-500/10 rounded-2xl border border-amber-500/20 flex items-center gap-3">
                                <Lock size={24} className="text-amber-600 shrink-0" />
                                <p className="text-xs font-bold text-amber-900">{t('transfer.importFile.passwordHint')}</p>
                            </div>
                            <div>
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('standards.password')}</label>
                                <Input
                                    type="password"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    placeholder={t('transfer.importFile.passwordPlaceholder')}
                                    className="mt-1"
                                    autoFocus
                                />
                            </div>
                        </div>
                    ) : definition ? (
                        <div className="space-y-6">
                            {/* Main Metadata Banner */}
                            <div className="p-5 bg-emerald-500/5 rounded-2xl border border-emerald-500/20 flex items-center gap-4">
                                <div className="p-3 bg-emerald-500/10 text-emerald-600 rounded-xl">
                                    <ShieldCheck size={28} />
                                </div>
                                <div className="space-y-1 overflow-hidden">
                                    <h4 className="text-base font-black text-theme-primary tracking-tight">
                                        {definition.immutable.identity.name} ({definition.immutable.identity.abbreviation})
                                    </h4>
                                    <p className="text-xs text-theme-secondary font-medium">
                                        Unit: <span className="font-bold">{definition.immutable.blueprint.unit}</span>
                                    </p>
                                    <p className="text-[10px] font-mono text-theme-light truncate">
                                        UUID: {definition.immutable.identity.uuid}
                                    </p>
                                </div>
                            </div>

                            {/* Details Grid */}
                            <div className="grid grid-cols-2 gap-3 text-xs">
                                <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                                    <span className="text-[10px] font-black uppercase text-theme-light block mb-1">Issuer / Org</span>
                                    <span className="font-bold text-theme-primary">{definition.mutable.metadata.issuerName || 'De-centralized'}</span>
                                </div>
                                <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                                    <span className="text-[10px] font-black uppercase text-theme-light block mb-1">Privacy Default</span>
                                    <span className="font-bold text-theme-primary capitalize">{definition.immutable.features.privacyMode}</span>
                                </div>
                                <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                                    <span className="text-[10px] font-black uppercase text-theme-light block mb-1">Max Validity</span>
                                    <span className="font-bold text-theme-primary">
                                        {definition.immutable.issuance.validityDurationRange.length > 0 
                                            ? definition.immutable.issuance.validityDurationRange.join(', ') 
                                            : 'Unlimited'}
                                    </span>
                                </div>
                                <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                                    <span className="text-[10px] font-black uppercase text-theme-light block mb-1">Divisible</span>
                                    <span className="font-bold text-theme-primary">{definition.immutable.features.allowPartialTransfers ? 'Yes' : 'No'}</span>
                                </div>
                            </div>

                            {/* Roles */}
                            {definition.immutable.issuance.allowedSignatureRoles.length > 0 && (
                                <div>
                                    <span className="text-[10px] font-black uppercase text-theme-light tracking-widest block mb-2">Allowed Co-Signer Roles</span>
                                    <div className="flex flex-wrap gap-2">
                                        {definition.immutable.issuance.allowedSignatureRoles.map((role: string, idx: number) => (
                                            <span key={idx} className="text-xs font-bold bg-theme-primary/10 text-theme-primary px-3 py-1 rounded-lg border border-theme-primary/20">
                                                {role}
                                            </span>
                                        ))}
                                    </div>
                                </div>
                            )}

                            {/* Rules Count */}
                            {rulesCount > 0 && (
                                <div className="p-3 bg-blue-50 rounded-xl border border-blue-200 text-xs text-blue-900 font-medium flex items-center gap-2">
                                    <CheckCircle2 size={16} className="text-blue-600 shrink-0" />
                                    <span>Contains <span className="font-bold">{rulesCount}</span> compliance rule(s)</span>
                                </div>
                            )}
                        </div>
                    ) : null}

                    {error && (
                        <p className="text-xs font-bold text-rose-500 animate-shake">{error}</p>
                    )}
                </div>
            }
        />
    );
}
