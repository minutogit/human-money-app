// src/components/voucher/VoucherBasicsForm.tsx

import { useTranslation } from "react-i18next";
import { Coins, AlertCircle, FlaskConical, FileSignature, Clock, ShieldAlert, WifiOff } from "lucide-react";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { HelpIcon } from "../ui/HelpIcon";
import { Advanced } from "../ui/Advanced";
import { Disclosure } from "../ui/Disclosure";
import { VoucherStandardInfo, VoucherStandardDefinition, StoredAnchorTemplateMeta } from "../../types";
import { getStandardShortSummary } from "../../hooks/useVoucherCreation";
import { formatQuarter } from "../../utils/retentionQuarter";

interface VoucherBasicsFormProps {
    standards: VoucherStandardInfo[];
    selectedStandardId: string;
    onStandardChange: (id: string) => void;
    amount: string;
    onAmountChange: (val: string) => void;
    validityValue: number;
    onValidityValueChange: (val: number) => void;
    validityUnit: "Y" | "M" | "D";
    onValidityUnitChange: (unit: "Y" | "M" | "D") => void;
    nonRedeemable: boolean;
    onNonRedeemableChange: (val: boolean) => void;
    isLoading: boolean;
    errors: Record<string, boolean>;
    standardRef: React.RefObject<HTMLSelectElement | null>;
    amountRef: React.RefObject<HTMLInputElement | null>;
    parsedStandard?: VoucherStandardDefinition | null;
    onManageStandards?: () => void;
    anchorTemplates?: StoredAnchorTemplateMeta[];
    selectedAnchorId?: string | null;
    onSelectedAnchorChange?: (id: string | null) => void;
    retentionInsufficient?: boolean;
}

export function VoucherBasicsForm({
    standards,
    selectedStandardId,
    onStandardChange,
    amount,
    onAmountChange,
    validityValue,
    onValidityValueChange,
    validityUnit,
    onValidityUnitChange,
    nonRedeemable,
    onNonRedeemableChange,
    isLoading,
    errors,
    standardRef,
    amountRef,
    parsedStandard,
    onManageStandards,
    anchorTemplates = [],
    selectedAnchorId = null,
    onSelectedAnchorChange,
    retentionInsufficient = false,
}: VoucherBasicsFormProps) {
    const { t, i18n } = useTranslation();

    const additionalSignaturesRange = parsedStandard?.immutable?.issuance?.additionalSignaturesRange;
    const minGuarantors = additionalSignaturesRange && additionalSignaturesRange[0] !== undefined ? additionalSignaturesRange[0] : 0;
    const customRules = parsedStandard?.immutable?.customRules || {};
    const hasGenderMale = Object.values(customRules).some(r => r.expression?.includes("gender == '1'"));
    const hasGenderFemale = Object.values(customRules).some(r => r.expression?.includes("gender == '2'"));
    const shortSummary = getStandardShortSummary(
        parsedStandard,
        i18n.language || "de",
        amount,
        (name, unit) => t("voucher.basics.standardFallbackSummary", { name, unit })
    );
    const validityUnitLabel =
        validityUnit === "Y"
            ? t("voucher.basics.years")
            : validityUnit === "M"
              ? t("voucher.basics.months")
              : t("voucher.basics.days");

    return (
        <Card header={
            <div className="flex items-center gap-2">
                <Coins size={18} className="text-theme-primary" />
                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.detailsHeader')}</span>
            </div>
        }>
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                <div className="lg:col-span-2 space-y-6">
                    <div className="space-y-3">
                        <div className="space-y-2">
                            <div className="flex items-center justify-between gap-2">
                                <label htmlFor="voucher-type" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.create.voucherType')}</label>
                                {onManageStandards && (
                                    <Advanced>
                                        <button
                                            type="button"
                                            onClick={onManageStandards}
                                            data-testid="manage-standards-button"
                                            className="text-[11px] font-bold text-theme-primary hover:underline cursor-pointer"
                                        >
                                            {t('voucher.basics.manageStandards', 'Standards verwalten')}
                                        </button>
                                    </Advanced>
                                )}
                            </div>
                            <select 
                                id="voucher-type"
                                ref={standardRef}
                                value={selectedStandardId} 
                                onChange={(e) => onStandardChange(e.target.value)} 
                                disabled={isLoading || standards.length === 0} 
                                className={`w-full bg-white border rounded-2xl px-4 py-3.5 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all ${errors.standard ? 'border-rose-500' : 'border-theme-subtle'}`}
                            >
                                <option value="">{t('voucher.basics.selectType')}</option>
                                {standards.map(s => (
                                    <option key={s.id} value={s.id}>{s.displayName}</option>
                                ))}
                            </select>
                            {shortSummary && (
                                <p
                                    className="text-xs text-theme-secondary leading-relaxed px-1"
                                    data-testid="standard-short-description"
                                >
                                    {shortSummary}
                                </p>
                            )}
                        </div>

                        {minGuarantors > 0 && (
                            <div className="p-3.5 bg-amber-500/[0.08] border border-amber-500/20 rounded-2xl flex items-start gap-3 text-amber-900 animate-in fade-in duration-300">
                                <FileSignature size={18} className="text-amber-600 shrink-0 mt-0.5" />
                                <div className="text-xs leading-relaxed">
                                    <p className="font-bold text-amber-950">
                                        {t('voucher.basics.guarantorRequiredTitle')}
                                    </p>
                                    <p className="text-amber-800/90 text-[11px] mt-0.5">
                                        {hasGenderMale && hasGenderFemale && minGuarantors === 2
                                            ? t('voucher.basics.guarantorRequirementMinuto')
                                            : t('voucher.basics.guarantorRequirementGeneric', { count: minGuarantors })}
                                    </p>
                                </div>
                            </div>
                        )}
                    </div>

                    <div className="space-y-4">
                        <div className="space-y-2">
                            <label htmlFor="voucher-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.basics.amountLabel')}</label>
                            <Input 
                                id="voucher-amount"
                                ref={amountRef}
                                type="number" 
                                value={amount} 
                                onChange={(e) => onAmountChange(e.target.value)} 
                                placeholder="0.00"
                                className={`font-bold text-sm ${errors.amount ? 'border-rose-500' : ''}`}
                            />
                        </div>
                        <Advanced>
                            <Disclosure
                                title={
                                    <span className="text-xs font-bold text-theme-secondary">
                                        {t('voucher.basics.validityDuration')}
                                    </span>
                                }
                                subtitle={`${validityValue} ${validityUnitLabel}`}
                                icon={<Clock size={14} className="text-theme-primary" />}
                                toggleTestId="voucher-validity-toggle"
                                contentTestId="voucher-validity-content"
                                className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50 dark:bg-slate-900/50"
                                buttonClassName="w-full flex items-center justify-between gap-3 p-3.5 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left cursor-pointer"
                                contentClassName="p-3.5 border-t border-theme-subtle bg-white dark:bg-slate-900"
                            >
                                <div className="flex gap-2">
                                    <Input 
                                        type="number" 
                                        value={validityValue} 
                                        onChange={(e) => onValidityValueChange(parseInt(e.target.value, 10) || 0)}
                                        className="w-1/2 font-bold text-sm"
                                    />
                                    <select 
                                        value={validityUnit} 
                                        onChange={(e) => onValidityUnitChange(e.target.value as "Y" | "M" | "D")}
                                        className="w-1/2 bg-white border border-theme-subtle rounded-xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                    >
                                        <option value="Y">{t('voucher.basics.years')}</option>
                                        <option value="M">{t('voucher.basics.months')}</option>
                                        <option value="D">{t('voucher.basics.days')}</option>
                                    </select>
                                </div>
                            </Disclosure>
                        </Advanced>
                    </div>
                </div>
                
                <div className="flex flex-col justify-center space-y-3">
                    <div className="flex items-center gap-1.5 pl-1">
                        <span className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.basics.modeHeader')}</span>
                        <HelpIcon topic="voucherMode" size={13} />
                    </div>
                    
                    <div className="bg-theme-secondary/[0.04] border border-theme-subtle/50 p-1.5 rounded-[24px] flex gap-1 shadow-inner-soft">
                        <button
                            type="button"
                            disabled={isLoading}
                            onClick={() => onNonRedeemableChange(true)}
                            data-testid="voucher-mode-test"
                            className={`flex-1 flex items-center justify-center gap-2 py-3 px-4 rounded-[18px] text-xs font-black tracking-wider uppercase transition-all duration-300 tap-bounce cursor-pointer ${
                                nonRedeemable
                                    ? 'bg-white text-emerald-600 shadow-premium border border-emerald-100/50 scale-[1.02]'
                                    : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'
                            }`}
                        >
                            <FlaskConical size={14} />
                            <span>{t('voucher.mode.test')}</span>
                        </button>
                        
                        <button
                            type="button"
                            disabled={isLoading}
                            onClick={() => onNonRedeemableChange(false)}
                            data-testid="voucher-mode-real"
                            className={`flex-1 flex items-center justify-center gap-2 py-3 px-4 rounded-[18px] text-xs font-black tracking-wider uppercase transition-all duration-300 tap-bounce cursor-pointer ${
                                !nonRedeemable
                                    ? 'bg-white text-rose-600 shadow-premium border border-rose-100/50 scale-[1.02]'
                                    : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'
                            }`}
                        >
                            <AlertCircle size={14} />
                            <span>{t('voucher.mode.real')}</span>
                        </button>
                    </div>
                    
                    <div className={`p-4 rounded-3xl border text-xs font-medium leading-relaxed transition-all duration-300 shadow-sm ${
                        nonRedeemable 
                            ? 'bg-emerald-500/[0.03] border-emerald-500/20 text-emerald-800' 
                            : 'bg-rose-500/[0.03] border-rose-500/20 text-rose-800'
                    }`}>
                        {nonRedeemable ? (
                            <div className="space-y-1 animate-in fade-in duration-350">
                                <p className="font-bold flex items-center gap-1.5 text-emerald-900">
                                    <FlaskConical size={12} className="text-emerald-600" /> {t('voucher.mode.testTitle')}
                                </p>
                                <p className="text-emerald-700/80 leading-normal text-[11px]">
                                    {t('voucher.mode.testDescription')}
                                </p>
                            </div>
                        ) : (
                            <div className="space-y-1 animate-in fade-in duration-350">
                                <p className="font-bold flex items-center gap-1.5 text-rose-900">
                                    <AlertCircle size={12} className="text-rose-600" /> {t('voucher.mode.realTitle')}
                                </p>
                                <p className="text-rose-700/80 leading-normal text-[11px]">
                                    {t('voucher.mode.realDescription')}
                                </p>
                            </div>
                        )}
                    </div>

                    {onSelectedAnchorChange && (
                        <div className="space-y-2 p-4 rounded-3xl border border-theme-subtle bg-white/70">
                            <div className="flex items-center justify-between gap-2">
                                <label
                                    htmlFor="voucher-l2-anchor"
                                    className="text-[10px] font-black text-theme-light uppercase tracking-widest"
                                >
                                    {t('voucher.basics.l2AnchorLabel', 'Online Lock Protection')}
                                </label>
                                <HelpIcon topic="l2Protection" size={14} />
                            </div>
                            <select
                                id="voucher-l2-anchor"
                                value={selectedAnchorId || ''}
                                onChange={(e) => onSelectedAnchorChange(e.target.value || null)}
                                className="w-full bg-white border border-theme-subtle rounded-xl px-3 py-2 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none"
                            >
                                <option value="">
                                    {t('voucher.basics.l2AnchorOfflineOnly', 'Offline-only (N = 0, trust-based)')}
                                </option>
                                {anchorTemplates.map((tpl) => (
                                    <option key={tpl.anchorId} value={tpl.anchorId}>
                                        {tpl.label} ({tpl.threshold}/{tpl.servers.length} · ≥ {formatQuarter(tpl.retainUntilQuarter)})
                                    </option>
                                ))}
                            </select>

                            {!selectedAnchorId && (
                                <p
                                    data-testid="voucher-offline-only-notice"
                                    className="text-[11px] text-amber-800 bg-amber-50 border border-amber-200 rounded-xl p-2 flex items-start gap-2"
                                >
                                    <WifiOff size={14} className="shrink-0 mt-0.5 text-amber-600" />
                                    <span>
                                        {t(
                                            'voucher.basics.l2AnchorOfflineOnlyWarning',
                                            'No lock servers selected (N = 0). Protected offline via SST V4 signatures, without live double-spend locks.'
                                        )}
                                    </span>
                                </p>
                            )}

                            {selectedAnchorId && retentionInsufficient && (
                                <p
                                    data-testid="voucher-retention-error"
                                    className="text-[11px] text-rose-800 bg-rose-50 border border-rose-200 rounded-xl p-2 flex items-start gap-2 font-semibold"
                                >
                                    <ShieldAlert size={14} className="shrink-0 mt-0.5 text-rose-600" />
                                    <span>
                                        {t(
                                            'voucher.basics.l2AnchorRetentionTooShort',
                                            'Selected anchor set retention is shorter than the voucher validity and retention window. Choose a longer template or shorten validity.'
                                        )}
                                    </span>
                                </p>
                            )}
                        </div>
                    )}
                </div>
            </div>
        </Card>
    );
}
