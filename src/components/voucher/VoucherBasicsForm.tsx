// src/components/voucher/VoucherBasicsForm.tsx

import { useTranslation } from "react-i18next";
import { Coins, AlertCircle, FlaskConical } from "lucide-react";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { VoucherStandardInfo } from "../../types";

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
    amountRef
}: VoucherBasicsFormProps) {
    const { t } = useTranslation();
    return (
        <Card header={
            <div className="flex items-center gap-2">
                <Coins size={18} className="text-theme-primary" />
                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.detailsHeader')}</span>
            </div>
        }>
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                <div className="lg:col-span-2 space-y-6">
                    <div className="space-y-2">
                        <label htmlFor="voucher-type" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.create.voucherType')}</label>
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
                    </div>
                    <div className="grid grid-cols-2 gap-4">
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
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.basics.validityDuration')}</label>
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
                        </div>
                    </div>
                </div>
                
                <div className="flex flex-col justify-center space-y-3">
                    <span className="text-[10px] font-black text-theme-light uppercase tracking-widest pl-1">{t('voucher.basics.modeHeader')}</span>
                    
                    <div className="bg-theme-secondary/[0.04] border border-theme-subtle/50 p-1.5 rounded-[24px] flex gap-1 shadow-inner-soft">
                        <button
                            type="button"
                            disabled={isLoading}
                            onClick={() => onNonRedeemableChange(true)}
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
                </div>
            </div>
        </Card>
    );
}
