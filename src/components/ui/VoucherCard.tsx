import React, { useCallback } from 'react';
import { 
    CheckCircle2, 
    Clock, 
    ShieldAlert, 
    History,
    MoreVertical,
    ArrowUpRight,
} from "lucide-react";
import { VoucherSummary, VoucherStatus } from "../../types";
import { Card } from "./Card";
import { formatAmount, formatDate } from "../../utils/format";
import Avatar from "boring-avatars";

interface VoucherCardProps {
    voucher: VoucherSummary;
    mode?: 'view' | 'select' | 'adjustable';
    isSelected?: boolean;
    selectedAmount?: string;
    onToggleSelect?: (voucher: VoucherSummary) => void;
    onAmountChange?: (voucherId: string, amount: string, maxAmount: string) => void;
    isExpanded?: boolean;
    onToggleExpand?: (voucherId: string) => void;
    showStatus?: boolean;
    precision?: number;
    children?: React.ReactNode;
}

function getVoucherStatus(status: VoucherStatus | string): { name: string; color: string; bgColor: string; icon: any } {
    if (!status) {
        return { name: 'unknown', color: 'text-gray-600', bgColor: 'bg-gray-100', icon: Clock };
    }
    const statusName = (typeof status === 'string' ? status : (Object.keys(status)[0] || 'unknown')).toLowerCase();
    
    switch (statusName) {
        case 'active':
            return { name: 'active', color: 'text-emerald-600', bgColor: 'bg-emerald-50', icon: CheckCircle2 };
        case 'quarantined':
            return { name: 'quarantined', color: 'text-rose-600', bgColor: 'bg-rose-50', icon: ShieldAlert };
        case 'archived':
            return { name: 'archived', color: 'text-indigo-600', bgColor: 'bg-indigo-50', icon: History };
        case 'incomplete':
            return { name: 'incomplete', color: 'text-amber-600', bgColor: 'bg-amber-50', icon: Clock };
        default:
            return { name: statusName, color: 'text-gray-600', bgColor: 'bg-gray-100', icon: Clock };
    }
}

export const VoucherCard = React.memo(({
    voucher,
    mode = 'view',
    isSelected = false,
    selectedAmount,
    onToggleSelect,
    onAmountChange,
    isExpanded = false,
    onToggleExpand,
    showStatus = true,
    precision = 2,
    children
}: VoucherCardProps) => {
    const { name: statusName, color, bgColor, icon: StatusIcon } = getVoucherStatus(voucher.status);
    
    const handleCardClick = useCallback(() => {
        if (mode === 'select' || mode === 'adjustable') {
            onToggleSelect?.(voucher);
        } else if (onToggleExpand) {
            onToggleExpand(voucher.localInstanceId);
        }
    }, [mode, voucher, onToggleSelect, onToggleExpand]);

    const displayAmount = (mode === 'adjustable' && isSelected && selectedAmount !== undefined) 
        ? selectedAmount 
        : voucher.currentAmount;

    return (
        <div className="relative group">
            {voucher.isTestVoucher && (
                <div className="absolute inset-0 flex items-center justify-center pointer-events-none z-20 overflow-hidden rounded-3xl">
                    <span className="text-[60px] font-black text-rose-500/10 -rotate-12 select-none uppercase tracking-[0.5em]">TEST</span>
                </div>
            )}
            
            <Card 
                variant="default"
                className={`p-0 overflow-hidden transition-all duration-500 ${
                    isExpanded ? 'ring-2 ring-theme-primary ring-offset-4 ring-offset-bg-app' : ''
                } ${
                    isSelected ? 'bg-theme-primary/5 border-theme-primary ring-4 ring-theme-primary/10' : ''
                }`}
                hover={!isExpanded && mode === 'view'}
                onClick={handleCardClick}
            >
                <div className="flex">
                    {/* Left Side: Status Stripe */}
                    {showStatus && <div className={`w-2 ${bgColor.replace('bg-', 'bg-opacity-100 bg-')}`}></div>}
                    
                    <div className="flex-grow flex flex-col">
                        {/* Top Section: Standard & Meta */}
                        <div className="px-6 py-4 flex items-center justify-between border-b border-theme-subtle/30 bg-white/50">
                            <div className="flex items-center gap-3">
                                <span className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                                    {voucher.displayStandardName}
                                </span>
                                {voucher.isTestVoucher && (
                                    <span className="text-[9px] font-black bg-rose-500 text-white px-2 py-0.5 rounded-full uppercase tracking-tighter">
                                        Test Mode
                                    </span>
                                )}
                            </div>
                            <div className="flex items-center gap-2">
                                {showStatus && (
                                    <div className={`flex items-center gap-1.5 px-2 py-1 rounded-full text-[10px] font-black uppercase ${bgColor} ${color}`}>
                                        <StatusIcon size={12} />
                                        {statusName}
                                    </div>
                                )}
                                {mode === 'view' && (
                                    <button className="p-1 hover:bg-theme-subtle/20 rounded-md text-theme-light transition-colors" onClick={(e) => e.stopPropagation()}>
                                        <MoreVertical size={16} />
                                    </button>
                                )}
                                {isSelected && (
                                    <div className="text-theme-primary">
                                        <CheckCircle2 size={16} />
                                    </div>
                                )}
                            </div>
                        </div>

                        {/* Middle Section: Amount & Title */}
                        <div className="px-6 py-6 flex flex-col md:flex-row md:items-center justify-between gap-4">
                            <div>
                                <div className="flex items-baseline gap-2">
                                    <h3 className="text-3xl font-black text-theme-primary tracking-tighter">
                                        {formatAmount(displayAmount, precision)}
                                    </h3>
                                    <span className="text-lg font-bold text-theme-light uppercase">
                                        {voucher.displayCurrency}
                                    </span>
                                </div>
                                <div className="flex items-center gap-2 mt-1">
                                    <Avatar size={16} name={voucher.localInstanceId} variant="pixel" />
                                    <p className="text-xs font-bold text-theme-secondary flex items-center gap-2">
                                        Issued by <span className="text-theme-primary">{voucher.creatorFirstName} {voucher.creatorLastName}</span>
                                    </p>
                                </div>
                            </div>
                            
                            {!isExpanded && (
                                <div className="flex items-center gap-6">
                                    <div className="text-right">
                                        <p className="text-[10px] font-black text-theme-light uppercase tracking-widest mb-1">Expires</p>
                                        <p className="text-xs font-bold text-theme-secondary">{formatDate(voucher.validUntil)}</p>
                                    </div>
                                    {mode === 'view' && (
                                        <ArrowUpRight size={20} className="text-theme-light opacity-30 group-hover:opacity-100 group-hover:translate-x-1 group-hover:-translate-y-1 transition-all" />
                                    )}
                                </div>
                            )}
                        </div>

                        {/* Adjustable Amount Section (SendView) */}
                        {mode === 'adjustable' && isSelected && voucher.divisible && (
                            <div className="px-6 pb-6 animate-in fade-in slide-in-from-top-2" onClick={(e) => e.stopPropagation()}>
                                <div className="pt-4 border-t border-theme-primary/10 flex items-center justify-between">
                                    <span className="text-[9px] font-black text-theme-primary uppercase tracking-widest">Partial Send</span>
                                    <div className="flex items-center gap-2">
                                        <input 
                                            type="number" 
                                            value={selectedAmount} 
                                            onChange={(e) => onAmountChange?.(voucher.localInstanceId, e.target.value, voucher.currentAmount)}
                                            className="w-24 py-1.5 px-3 text-right font-black text-xs rounded-xl border border-theme-primary/30 focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                                            max={voucher.currentAmount}
                                            min="0"
                                            step="any"
                                        />
                                    </div>
                                </div>
                            </div>
                        )}

                        {/* Expanded Content (WalletView) */}
                        <div className={`transition-all duration-500 ease-in-out ${isExpanded ? 'max-h-[800px] opacity-100 pb-6 px-6' : 'max-h-0 opacity-0 overflow-hidden'}`}>
                            {children}
                        </div>
                    </div>
                </div>
                
                {/* Ticket Notch Decorations */}
                <div className="absolute top-1/2 -left-3 w-6 h-6 bg-bg-app rounded-full -translate-y-1/2 border-r border-theme-subtle/30 z-10 hidden md:block"></div>
                <div className="absolute top-1/2 -right-3 w-6 h-6 bg-bg-app rounded-full -translate-y-1/2 border-l border-theme-subtle/30 z-10 hidden md:block"></div>
            </Card>
        </div>
    );
});

VoucherCard.displayName = 'VoucherCard';
