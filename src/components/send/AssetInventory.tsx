import { useTranslation } from 'react-i18next';
import { Coins, Filter, CreditCard } from 'lucide-react';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { VoucherCard } from '../ui/VoucherCard';
import { Advanced } from '../ui/Advanced';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
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
  onCreateVoucher?: () => void;
  onReceiveVoucher?: () => void;
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
  standardIdToUuidMap,
  onCreateVoucher,
  onReceiveVoucher,
}: AssetInventoryProps) {
  const { t } = useTranslation();
  const resolveStandardId = (standardUuid: string): string =>
    [...standardIdToUuidMap.entries()].find(([, uuid]) => uuid === standardUuid)?.[0] || standardUuid;

  const selectedClass = selectedStandardId
    ? activeAssetClasses.find(g => {
        const stdUuid = standardIdToUuidMap.get(selectedStandardId) || selectedStandardId;
        return g.standardUuid === stdUuid && (selectedIsTest === null || g.isTestVoucher === selectedIsTest);
      })
    : undefined;
  const fallbackClass = activeAssetClasses.find(g => !g.isTestVoucher) ?? activeAssetClasses[0];
  const activeOrFallbackClass = selectedClass ?? fallbackClass;
  const displayCurrencyUnit = filteredVouchers[0]?.displayCurrency ?? activeOrFallbackClass?.displayCurrency ?? '';
  const hasAnyActiveVouchers = activeAssetClasses.length > 0 || filteredVouchers.length > 0;
  const currentSelectValue = activeOrFallbackClass
    ? `${resolveStandardId(activeOrFallbackClass.standardUuid)}:${activeOrFallbackClass.isTestVoucher}`
    : '';

  return (
    <Card header={
      <div className="flex items-center gap-2">
        <Coins size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.amountAndCurrencyTitle')}</span>
      </div>
    }>
      <div className="space-y-6">
        {hasAnyActiveVouchers && (
          <div className="space-y-3 animate-in slide-in-from-top-2">
            <label htmlFor="target-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.targetAmount')}</label>
            <div className="flex items-stretch gap-2">
              <div className="relative flex-1">
                <Input
                  id="target-amount"
                  value={targetAmountStr}
                  onChange={(e) => onTargetAmountChange(e.target.value)}
                  type="number"
                  placeholder="0.00"
                  className="py-5 px-6 rounded-3xl font-black text-2xl tracking-tighter"
                />
                {activeAssetClasses.length <= 1 && (
                  <div className="absolute right-6 top-1/2 -translate-y-1/2 font-black text-theme-primary uppercase tracking-widest">
                    {displayCurrencyUnit}
                  </div>
                )}
              </div>
              {activeAssetClasses.length > 1 && (
                <select
                  data-testid="currency-select"
                  aria-label={t('transfer.currencyLabel')}
                  value={currentSelectValue}
                  onChange={(e) => {
                    const idx = e.target.selectedIndex;
                    const chosen = activeAssetClasses[idx];
                    if (chosen) {
                      onStandardSelect(resolveStandardId(chosen.standardUuid), chosen.isTestVoucher);
                    }
                  }}
                  className="bg-white border border-theme-subtle rounded-3xl px-4 text-sm font-bold text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shrink-0"
                >
                  {activeAssetClasses.map(group => {
                    const stdId = resolveStandardId(group.standardUuid);
                    const key = `${stdId}:${group.isTestVoucher}`;
                    return (
                      <option key={key} value={key}>
                        {group.displayStandardName}
                      </option>
                    );
                  })}
                </select>
              )}
            </div>
            <Advanced>
              <p className="text-[10px] font-bold text-theme-light italic text-center">{t('transfer.autoSelection')}</p>
            </Advanced>
          </div>
        )}

        <Advanced>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => onStandardSelect(null, null)}
              className={`px-4 py-2 text-[10px] font-black uppercase tracking-widest rounded-full border-2 transition-all ${selectedStandardId === null ? 'bg-theme-primary border-theme-primary text-white shadow-md' : 'border-slate-100 text-slate-400 hover:border-slate-200'}`}
            >
              {t('transfer.all')}
            </button>
            {activeAssetClasses.map(group => {
              const standardId = resolveStandardId(group.standardUuid);
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
        </Advanced>

        {hasAnyActiveVouchers ? (
          <AdvancedDisclosure
            title={t('transfer.selectVouchersManually')}
            toggleTestId="manual-voucher-selection-toggle"
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.availableVouchers')}</h3>
                <span title={t('transfer.sortOptimalHint')} className="text-theme-light flex items-center gap-1.5 text-[10px] font-black uppercase tracking-widest cursor-help">
                  <Filter size={10}/> {t('transfer.sortOptimal')}
                </span>
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
                  <div className="text-center py-8 px-4 bg-slate-50 rounded-[24px] border border-dashed border-slate-200">
                    <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('transfer.inventoryDepleted')}</p>
                  </div>
                )}
              </div>
            </div>
          </AdvancedDisclosure>
        ) : (
          <div className="text-center py-10 px-4 bg-slate-50 rounded-[32px] border-2 border-dashed border-slate-200 space-y-3">
            <CreditCard className="mx-auto text-slate-300" size={32} />
            <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('transfer.inventoryDepleted')}</p>
            {(onCreateVoucher || onReceiveVoucher) && (
              <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
                {onCreateVoucher && (
                  <Button type="button" variant="primary" size="xs" onClick={onCreateVoucher}>
                    {t('voucher.create')}
                  </Button>
                )}
                {onReceiveVoucher && (
                  <Button type="button" variant="outline" size="xs" onClick={onReceiveVoucher}>
                    {t('transfer.receive')}
                  </Button>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
