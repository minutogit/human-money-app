import { useTranslation } from 'react-i18next';
import { Coins, Filter, CreditCard } from 'lucide-react';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { VoucherCard } from '../ui/VoucherCard';
import { VoucherSummary, AssetClassSummary } from '../../types';

interface AssetInventoryProps {
  filteredVouchers: VoucherSummary[];
  activeAssetClasses: AssetClassSummary[];
  selectedStandardId: string | null;
  selectedIsTest: boolean | null;
  onStandardSelect: (id: string | null, isTest: boolean | null) => void;
  targetAmountStr: string;
  onTargetAmountChange: (val: string) => void;
  selection: Map<string, string>;
  onVoucherToggle: (v: VoucherSummary) => void;
  onAmountChange: (id: string, amt: string, max: string) => void;
  uuidToPrecisionMap: Map<string, number>;
  standardIdToUuidMap: Map<string, string>;
}

export function AssetInventory({
  filteredVouchers,
  activeAssetClasses,
  selectedStandardId,
  selectedIsTest,
  onStandardSelect,
  targetAmountStr,
  onTargetAmountChange,
  selection,
  onVoucherToggle,
  onAmountChange,
  uuidToPrecisionMap,
  standardIdToUuidMap
}: AssetInventoryProps) {
  const { t } = useTranslation();
  return (
    <Card header={
      <div className="flex items-center gap-2">
        <Coins size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.voucherSelectionTitle')}</span>
      </div>
    }>
      <div className="space-y-6">
        <div className="flex flex-wrap gap-2">
          <button 
            type="button" 
            onClick={() => onStandardSelect(null, null)} 
            className={`px-4 py-2 text-[10px] font-black uppercase tracking-widest rounded-full border-2 transition-all ${selectedStandardId === null ? 'bg-theme-primary border-theme-primary text-white shadow-md' : 'border-slate-100 text-slate-400 hover:border-slate-200'}`}
          >
            {t('transfer.all')}
          </button>
          {activeAssetClasses.map(group => {
            const standardId = [...standardIdToUuidMap.entries()].find(([, uuid]) => uuid === group.standardUuid)?.[0] || group.standardUuid;
            const key = `${standardId}:${group.isTestVoucher}`;
            const isSelected = selectedStandardId === standardId && selectedIsTest === group.isTestVoucher;
            return (
              <button
                key={key}
                type="button"
                onClick={() => onStandardSelect(standardId, group.isTestVoucher)}
                className={`px-4 py-2 text-[10px] font-black uppercase tracking-widest rounded-full border-2 transition-all ${isSelected ? (group.isTestVoucher ? 'bg-rose-500 border-rose-500 text-white shadow-md' : 'bg-theme-primary border-theme-primary text-white shadow-md') : (group.isTestVoucher ? 'border-rose-200 text-rose-400 hover:border-rose-300' : 'border-slate-100 text-slate-400 hover:border-slate-200')}`}
              >
                {group.displayStandardName}
              </button>
            );
          })}
        </div>

        {selectedStandardId && (
          <div className="space-y-3 animate-in slide-in-from-top-2">
            <label htmlFor="target-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.targetAmount')}</label>
            <div className="relative">
              <Input 
                id="target-amount" 
                value={targetAmountStr} 
                onChange={(e) => onTargetAmountChange(e.target.value)} 
                type="number" 
                placeholder="0.00" 
                className="py-5 px-6 rounded-3xl font-black text-2xl tracking-tighter" 
              />
              <div className="absolute right-6 top-1/2 -translate-y-1/2 font-black text-theme-primary uppercase tracking-widest">
                {filteredVouchers[0]?.displayCurrency}
              </div>
            </div>
            <p className="text-[10px] font-bold text-theme-light italic text-center">{t('transfer.autoSelection')}</p>
          </div>
        )}

        <div className="space-y-3 pt-4 border-t border-theme-subtle/40">
          <div className="flex items-center justify-between">
            <h3 className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.availableVouchers')}</h3>
            <button type="button" className="text-theme-primary hover:underline flex items-center gap-1.5 text-[10px] font-black uppercase tracking-widest">
              <Filter size={10}/> {t('transfer.sortOptimal')}
            </button>
          </div>
          
          <div className="space-y-3 max-h-[400px] overflow-y-auto pr-2 scrollbar-thin scrollbar-thumb-theme-subtle">
            {filteredVouchers.length > 0 ? filteredVouchers.map(v => {
              const selectedAmount = selection.get(v.localInstanceId);
              const isSelected = selectedAmount !== undefined;
              const precision = uuidToPrecisionMap.get(v.voucherStandardUuid) ?? 2;
              
              return (
                <VoucherCard
                  key={v.localInstanceId}
                  voucher={v}
                  mode="adjustable"
                  isSelected={isSelected}
                  selectedAmount={selectedAmount}
                  onToggleSelect={onVoucherToggle}
                  onAmountChange={onAmountChange}
                  precision={precision}
                  showStatus={false}
                />
              );
            }) : (
              <div className="text-center py-10 bg-slate-50 rounded-[32px] border-2 border-dashed border-slate-200">
                <CreditCard className="mx-auto text-slate-300 mb-2" size={32} />
                <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('transfer.inventoryDepleted')}</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </Card>
  );
}
