// src/components/voucher/VoucherBasicsForm.tsx

import { Coins, CheckCircle2, AlertCircle } from "lucide-react";
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
    return (
        <Card header={
            <div className="flex items-center gap-2">
                <Coins size={18} className="text-theme-primary" />
                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Voucher Details</span>
            </div>
        }>
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                <div className="lg:col-span-2 space-y-6">
                    <div className="space-y-2">
                        <label htmlFor="voucher-type" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Voucher Type</label>
                        <select 
                            id="voucher-type"
                            ref={standardRef}
                            value={selectedStandardId} 
                            onChange={(e) => onStandardChange(e.target.value)} 
                            disabled={isLoading || standards.length === 0} 
                            className={`w-full bg-white border rounded-2xl px-4 py-3.5 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all ${errors.standard ? 'border-rose-500' : 'border-theme-subtle'}`}
                        >
                            <option value="">Select Voucher Type...</option>
                            {standards.map(s => (
                                <option key={s.id} value={s.id}>{s.displayName}</option>
                            ))}
                        </select>
                    </div>
                    <div className="grid grid-cols-2 gap-4">
                        <div className="space-y-2">
                            <label htmlFor="voucher-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">Amount (e.g., 60)</label>
                            <Input 
                                id="voucher-amount"
                                ref={amountRef}
                                type="number" 
                                value={amount} 
                                onChange={(e) => onAmountChange(e.target.value)} 
                                placeholder="0.00"
                                className={`font-black text-lg py-4 ${errors.amount ? 'border-rose-500' : ''}`}
                            />
                        </div>
                        <div className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Validity Duration</label>
                            <div className="flex gap-2">
                                <Input 
                                    type="number" 
                                    value={validityValue} 
                                    onChange={(e) => onValidityValueChange(parseInt(e.target.value, 10) || 0)}
                                    className="w-1/2 font-bold"
                                />
                                <select 
                                    value={validityUnit} 
                                    onChange={(e) => onValidityUnitChange(e.target.value as "Y" | "M" | "D")}
                                    className="w-1/2 bg-white border border-theme-subtle rounded-xl px-3 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                >
                                    <option value="Y">Years</option>
                                    <option value="M">Months</option>
                                    <option value="D">Days</option>
                                </select>
                            </div>
                        </div>
                    </div>
                </div>
                
                <div className="flex flex-col justify-center">
                    <label className={`relative flex flex-col p-6 rounded-[32px] border-2 transition-all cursor-pointer group shadow-sm ${nonRedeemable ? 'bg-emerald-500/5 border-emerald-500 shadow-emerald-100' : 'bg-rose-500/5 border-rose-500 shadow-rose-100'}`}>
                        <input 
                            type="checkbox" 
                            checked={nonRedeemable} 
                            onChange={(e) => onNonRedeemableChange(e.target.checked)}
                            className="absolute opacity-0"
                        />
                        <div className={`mb-3 p-3 rounded-2xl w-fit ${nonRedeemable ? 'bg-emerald-500 text-white' : 'bg-rose-500 text-white'}`}>
                            {nonRedeemable ? <CheckCircle2 size={24} /> : <AlertCircle size={24} />}
                        </div>
                        <span className={`text-lg font-black tracking-tight ${nonRedeemable ? 'text-emerald-900' : 'text-rose-900'}`}>
                            {nonRedeemable ? 'Test Voucher' : 'Real Voucher'}
                        </span>
                        <span className={`text-[10px] font-bold mt-1 ${nonRedeemable ? 'text-emerald-700/60' : 'text-rose-700/60'}`}>
                            {nonRedeemable ? 'Test Voucher' : 'Standard obligation'}
                        </span>
                    </label>
                </div>
            </div>
        </Card>
    );
}
