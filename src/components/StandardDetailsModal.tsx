import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { VoucherStandardInfo, VoucherStandardDefinition } from '../types';
import { standardsService } from '../services/standardsService';
import { copyToClipboard } from '../utils/clipboard';
import { ModalShell } from './ui/ModalShell';
import { Button } from './ui/Button';
import {
    ShieldCheck,
    FileCode,
    ChevronUp,
    ChevronDown,
    Copy,
    Check,
    CheckCircle2
} from 'lucide-react';

export interface StandardDetailsModalProps {
    isOpen: boolean;
    standard: VoucherStandardInfo;
    definition?: VoucherStandardDefinition | null;
    onClose: () => void;
}

export function StandardDetailsModal({
    isOpen,
    standard,
    definition,
    onClose,
}: StandardDetailsModalProps) {
    const { t, i18n } = useTranslation();
    const [parsedDef, setParsedDef] = useState<VoucherStandardDefinition | null>(definition || null);
    const [showRawToml, setShowRawToml] = useState(false);
    const [copied, setCopied] = useState(false);

    useEffect(() => {
        if (!isOpen) {
            setShowRawToml(false);
            setCopied(false);
            return;
        }

        if (definition) {
            setParsedDef(definition);
            return;
        }

        let isMounted = true;
        standardsService
            .parseStandard(standard.content)
            .then((res) => {
                if (isMounted) {
                    setParsedDef(res);
                }
            })
            .catch((err) => {
                console.warn('Failed to parse voucher standard TOML:', err);
                if (isMounted) {
                    setParsedDef(null);
                }
            });

        return () => {
            isMounted = false;
        };
    }, [isOpen, definition, standard]);

    const handleCopy = async () => {
        const success = await copyToClipboard(standard.content);
        if (success) {
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    const def = parsedDef;

    // Localized text resolution
    const lang = (i18n.language || 'en').slice(0, 2);
    const descriptions = def?.mutable?.i18n?.descriptions || {};
    const description =
        descriptions[lang] ||
        descriptions['de'] ||
        descriptions['en'] ||
        Object.values(descriptions)[0];

    const footnotes = def?.mutable?.i18n?.footnotes || {};
    const footnote =
        footnotes[lang] ||
        footnotes['de'] ||
        footnotes['en'] ||
        Object.values(footnotes)[0];

    const collateralDescriptions = def?.mutable?.i18n?.collateralDescriptions || {};
    const collateralDesc =
        collateralDescriptions[lang] ||
        collateralDescriptions['de'] ||
        collateralDescriptions['en'] ||
        Object.values(collateralDescriptions)[0];

    // Identity and blueprint
    const name = def?.immutable?.identity?.name || standard.displayName;
    const abbreviation = def?.immutable?.identity?.abbreviation;
    const unit = def?.immutable?.blueprint?.unit;
    const uuid = def?.immutable?.identity?.uuid || standard.id;

    // Signatures and rules
    const sigRange = def?.immutable?.issuance?.additionalSignaturesRange || [];
    const minSigs = sigRange[0] ?? 0;
    const maxSigs = sigRange[1] ?? 0;
    const hasSignatureRequirements = maxSigs > 0 || minSigs > 0;
    const roles = def?.immutable?.issuance?.allowedSignatureRoles || [];
    const rulesCount = Object.keys(def?.immutable?.customRules || {}).length;

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="lg"
            closeOnBackdrop={true}
            title={t('standards.detailsModalTitle')}
            footer={
                <div className="flex justify-end">
                    <Button variant="secondary" onClick={onClose}>
                        {t('common.close')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-5">
                {/* Top Summary Banner */}
                <div className="p-5 bg-emerald-500/5 rounded-2xl border border-emerald-500/20 flex items-center gap-4">
                    <div className="p-3 bg-emerald-500/10 text-emerald-600 rounded-xl shrink-0">
                        <ShieldCheck size={28} />
                    </div>
                    <div className="space-y-1 overflow-hidden min-w-0 flex-1">
                        <h4 className="text-base font-black text-theme-primary tracking-tight truncate">
                            {name}
                            {abbreviation && (
                                <span className="ml-1 text-theme-secondary font-medium">
                                    ({abbreviation})
                                </span>
                            )}
                        </h4>
                        {unit && (
                            <p className="text-xs text-theme-secondary font-medium">
                                {t('standards.unitLabel')}:{' '}
                                <span className="font-bold text-theme-primary">{unit}</span>
                            </p>
                        )}
                        <p className="text-[10px] font-mono text-theme-light truncate">
                            UUID: {uuid}
                        </p>
                    </div>
                </div>

                {/* Localized Description & Footnotes */}
                {(description || footnote || collateralDesc) && (
                    <div className="bg-slate-50 border border-slate-200/60 rounded-2xl p-4 text-xs text-theme-secondary leading-relaxed space-y-2">
                        {description && <p>{description}</p>}
                        {footnote && (
                            <p className="text-[11px] text-theme-light italic border-t border-slate-200/60 pt-2">
                                {footnote}
                            </p>
                        )}
                        {collateralDesc && (
                            <p className="text-[11px] text-theme-light border-t border-slate-200/60 pt-2">
                                {collateralDesc}
                            </p>
                        )}
                    </div>
                )}

                {/* Compact 2x2 Key Rules Grid */}
                {def && (
                    <div className="grid grid-cols-2 gap-3 text-xs">
                        <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                            <span className="text-[10px] font-black uppercase text-theme-light block mb-1">
                                {t('standards.issuerLabel')}
                            </span>
                            <span className="font-bold text-theme-primary truncate block">
                                {def.mutable?.metadata?.issuerName || t('standards.decentralizedIssuer')}
                            </span>
                        </div>
                        <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                            <span className="text-[10px] font-black uppercase text-theme-light block mb-1">
                                {t('standards.validityRangeLabel')}
                            </span>
                            <span className="font-bold text-theme-primary truncate block">
                                {def.immutable?.issuance?.validityDurationRange &&
                                def.immutable.issuance.validityDurationRange.length > 0
                                    ? def.immutable.issuance.validityDurationRange.join(' – ')
                                    : t('standards.unlimitedValidity')}
                            </span>
                        </div>
                        <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                            <span className="text-[10px] font-black uppercase text-theme-light block mb-1">
                                {t('standards.divisibleLabel')}
                            </span>
                            <span className="font-bold text-theme-primary block">
                                {def.immutable?.features?.allowPartialTransfers
                                    ? t('common.yes')
                                    : t('common.no')}
                            </span>
                        </div>
                        <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60">
                            <span className="text-[10px] font-black uppercase text-theme-light block mb-1">
                                {t('standards.privacyModeLabel')}
                            </span>
                            <span className="font-bold text-theme-primary capitalize block">
                                {def.immutable?.features?.privacyMode}
                            </span>
                        </div>
                    </div>
                )}

                {/* Signatures / Roles & Custom Rules */}
                {def && (hasSignatureRequirements || roles.length > 0) && (
                    <div className="p-3 bg-slate-50 rounded-xl border border-slate-200/60 space-y-2">
                        {hasSignatureRequirements && (
                            <div className="flex items-center justify-between text-xs">
                                <span className="text-[10px] font-black uppercase text-theme-light">
                                    {t('standards.signaturesRequiredLabel')}
                                </span>
                                <span className="font-bold text-theme-primary">
                                    {minSigs === maxSigs ? String(minSigs) : `${minSigs}–${maxSigs}`}
                                </span>
                            </div>
                        )}
                        {roles.length > 0 && (
                            <div>
                                <span className="text-[10px] font-black uppercase text-theme-light tracking-widest block mb-1.5">
                                    {t('standards.allowedRolesLabel')}
                                </span>
                                <div className="flex flex-wrap gap-1.5">
                                    {roles.map((role, idx) => (
                                        <span
                                            key={idx}
                                            className="text-[11px] font-bold bg-theme-primary/10 text-theme-primary px-2.5 py-0.5 rounded-lg border border-theme-primary/20"
                                        >
                                            {role}
                                        </span>
                                    ))}
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {def && rulesCount > 0 && (
                    <div className="p-3 bg-blue-50 rounded-xl border border-blue-200 text-xs text-blue-900 font-medium flex items-center gap-2">
                        <CheckCircle2 size={16} className="text-blue-600 shrink-0" />
                        <span>{t('standards.customRulesCount', { count: rulesCount })}</span>
                    </div>
                )}

                {/* Collapsible Raw TOML Section */}
                <div className="space-y-3 pt-2 border-t border-slate-100">
                    <button
                        type="button"
                        onClick={() => setShowRawToml((prev) => !prev)}
                        className="flex items-center justify-between w-full px-3.5 py-2.5 rounded-xl bg-slate-100/80 hover:bg-slate-200/60 text-xs font-bold text-theme-secondary transition-colors cursor-pointer"
                    >
                        <div className="flex items-center gap-2">
                            <FileCode size={14} />
                            <span>
                                {showRawToml
                                    ? t('standards.hideRawToml')
                                    : t('standards.showRawToml')}
                            </span>
                        </div>
                        {showRawToml ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                    </button>

                    {showRawToml && (
                        <div className="space-y-2 animate-in fade-in duration-200">
                            <div className="flex items-center justify-between px-1">
                                <span className="text-[10px] font-mono text-theme-light">
                                    {standard.id}.toml
                                </span>
                                <button
                                    type="button"
                                    onClick={handleCopy}
                                    className="flex items-center gap-1.5 text-xs font-bold text-theme-primary hover:underline cursor-pointer"
                                >
                                    {copied ? (
                                        <>
                                            <Check size={13} className="text-emerald-500" />
                                            <span className="text-emerald-600">
                                                {t('common.copied')}
                                            </span>
                                        </>
                                    ) : (
                                        <>
                                            <Copy size={13} />
                                            <span>{t('standards.copyToml')}</span>
                                        </>
                                    )}
                                </button>
                            </div>
                            <pre className="p-4 bg-slate-900 text-slate-100 rounded-2xl text-[11px] font-mono overflow-auto max-h-64 leading-relaxed select-all whitespace-pre-wrap break-words">
                                {standard.content}
                            </pre>
                        </div>
                    )}
                </div>
            </div>
        </ModalShell>
    );
}
