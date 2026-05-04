import { Send } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { formatAmount } from '../../utils/format';
import { VoucherSummary } from '../../types';

interface TransferSummaryBarProps {
  checkoutSummary: {
    count: number;
    summableTotals: Record<string, number>;
    countableTotals: Record<string, number>;
  };
  selectionSize: number;
  isProcessing: boolean;
  onPrepareTransfer: (event: React.FormEvent) => void;
  availableVouchers: VoucherSummary[];
  uuidToPrecisionMap: Map<string, number>;
}

export function TransferSummaryBar({
  checkoutSummary,
  selectionSize,
  isProcessing,
  onPrepareTransfer,
  availableVouchers,
  uuidToPrecisionMap
}: TransferSummaryBarProps) {
  return (
    <div className="sticky bottom-8 z-30 transform-gpu will-change-transform">
      <Card variant="none" className="shadow-premium-xl border-theme-primary/20 bg-theme-primary text-white overflow-hidden p-0 rounded-[40px]">
        <div className="p-8 space-y-6">
          <div className="flex justify-between items-center">
            <div className="space-y-1">
              <p className="text-[10px] font-black uppercase tracking-[0.2em] text-white/60">Disbursement Summary</p>
              <h3 className="text-2xl font-black tracking-tighter">
                {checkoutSummary.count > 0 ? (
                  <div className="flex flex-wrap gap-x-4 gap-y-1">
                    {Object.entries(checkoutSummary.summableTotals).map(([unit, total]) => {
                      const voucher = availableVouchers.find(v => v.displayCurrency === unit);
                      const precision = voucher ? (uuidToPrecisionMap.get(voucher.voucherStandardUuid) ?? 2) : 2;
                      return (
                        <span key={unit} className="flex items-baseline gap-1.5">
                          {formatAmount(total.toString(), precision)}
                          <span className="text-sm font-bold uppercase tracking-widest opacity-70">{unit}</span>
                        </span>
                      );
                    })}
                    {Object.entries(checkoutSummary.countableTotals).map(([unit, total]) => (
                      <span key={unit} className="flex items-baseline gap-1.5">
                        {total}
                        <span className="text-sm font-bold uppercase tracking-widest opacity-70">{unit}{total > 1 ? 's' : ''}</span>
                      </span>
                    ))}
                  </div>
                ) : "0.00"}
              </h3>
            </div>
            <div className="w-14 h-14 bg-white/20 rounded-2xl flex items-center justify-center shadow-inner">
              <Send size={28} />
            </div>
          </div>
          
          <div className="flex items-center justify-between pt-4 border-t border-white/20">
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-white animate-pulse"></div>
              <span key={selectionSize} className="text-[9px] font-black uppercase tracking-widest animate-in fade-in duration-300">
                {selectionSize} Vouchers Selected
              </span>
            </div>
            <Button 
              onClick={onPrepareTransfer} 
              disabled={isProcessing} 
              className="!bg-white !bg-none !text-theme-primary rounded-2xl px-6 py-3 font-black text-xs uppercase tracking-widest shadow-lg hover:scale-105 active:scale-95 transition-all"
            >
              {isProcessing ? "Sending..." : "Send Voucher"}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
