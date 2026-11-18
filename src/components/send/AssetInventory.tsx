import { useTranslation } from 'react-i18next';
import { Coins, Filter, CreditCard, ShieldCheck, AlertCircle } from 'lucide-react';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { VoucherCard } from '../ui/VoucherCard';
import { Disclosure } from '../ui/Disclosure';
import { TransferPlan } from '../../services/transferService';
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
  currentPlan?: TransferPlan | null;
  recipientSelected?: boolean;
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
  currentPlan,
  recipientSelected = false,
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
  const currentSelectValue = selectedStandardId === null
    ? 'ALL'
    : activeOrFallbackClass
      ? `${resolveStandardId(activeOrFallbackClass.standardUuid)}:${activeOrFallbackClass.isTestVoucher}`
      : '';

  const minTrustPercent = currentPlan?.minRecipientTrustPercent;

  return (
    <Card header={
      <div className="flex items-center gap-2">
        <Coins size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.amountAndCurrencyTitle')}</span>
      </div>
    }>
      <div className="space-y-4">
        {hasAnyActiveVouchers && (
          <div className="space-y-2.5">
            <label htmlFor="target-amount" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.targetAmount')}</label>
            <div className="flex items-stretch gap-2">
              <div className="relative flex-1">
                <Input
                  id="target-amount"
                  value={targetAmountStr}
                  onChange={(e) => onTargetAmountChange(e.target.value)}
                  type="number"
                  placeholder="0.00"
                  className="py-4 px-5 rounded-2xl font-black text-2xl tracking-tighter"
                />
                {activeAssetClasses.length === 0 && displayCurrencyUnit && (
                  <div className="absolute right-5 top-1/2 -translate-y-1/2 font-black text-theme-primary uppercase tracking-widest text-sm">
                    {displayCurrencyUnit}
                  </div>
                )}
              </div>
              {activeAssetClasses.length > 0 && (
                <select
                  data-testid="currency-select"
                  aria-label={t('transfer.currencyLabel')}
                  value={currentSelectValue}
                  onChange={(e) => {
                    const val = e.target.value;
                    if (val === 'ALL') {
                      onStandardSelect(null, null);
                      return;
                    }
                    const chosen = activeAssetClasses.find(group => {
                      const stdId = resolveStandardId(group.standardUuid);
                      return `${stdId}:${group.isTestVoucher}` === val;
                    });
                    if (chosen) {
                      onStandardSelect(resolveStandardId(chosen.standardUuid), chosen.isTestVoucher);
                    }
                  }}
                  className="bg-white border border-theme-subtle rounded-2xl px-3.5 text-sm font-bold text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary/20 shrink-0"
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
                  <option value="ALL">{t('transfer.all')}</option>
                </select>
              )}
            </div>

            {selection.size > 0 && (
              <div className="px-3.5 py-2.5 rounded-2xl bg-slate-50 border border-theme-subtle/70 text-xs space-y-1">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-bold text-theme-secondary">
                    {t('transfer.vouchersSelected', { count: selection.size })}
                  </span>
                  {recipientSelected && typeof minTrustPercent === 'number' && (
                    minTrustPercent > 0 ? (
                      <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-emerald-700">
                        <ShieldCheck size={13} className="shrink-0" />
                        {t('transfer.recipientTrustFit', { percent: minTrustPercent })}
                      </span>
                    ) : (
                      <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-amber-700">
                        <AlertCircle size={13} className="shrink-0" />
                        {t('transfer.recipientTrustUnknownHint')}
                      </span>
                    )
                  )}
                </div>
                {currentPlan?.needsSplit && (
                  <p className="text-[11px] text-amber-800 leading-relaxed">
                    {t('transfer.splitReassurance')}
                  </p>
                )}
              </div>
            )}
          </div>
        )}

        {hasAnyActiveVouchers ? (
          <Disclosure
            defaultOpen={false}
            title={t('transfer.selectVouchersManually')}
            toggleTestId="manual-voucher-selection-toggle"
            className="border border-theme-subtle rounded-2xl overflow-hidden bg-white/50"
            buttonClassName="w-full flex items-center justify-between gap-3 px-4 py-3 bg-slate-50/70 hover:bg-slate-100/70 transition-colors text-left cursor-pointer text-xs font-bold text-theme-secondary"
            contentClassName="p-4 border-t border-theme-subtle bg-white"
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('transfer.availableVouchers')}</h3>
                <span title={t('transfer.sortOptimalHint')} className="text-theme-light flex items-center gap-1.5 text-[10px] font-black uppercase tracking-widest cursor-help">
                  <Filter size={10}/> {t('transfer.sortOptimal')}
                </span>
              </div>

              <div className="space-y-3 max-h-[360px] overflow-y-auto pr-1 scrollbar-thin scrollbar-thumb-theme-subtle">
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
                  <div className="text-center py-8 px-4 bg-slate-50 rounded-2xl border border-dashed border-slate-200">
                    <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('transfer.inventoryDepleted')}</p>
                  </div>
                )}
              </div>
            </div>
          </Disclosure>
        ) : (
          <div className="text-center py-10 px-4 bg-slate-50 rounded-3xl border-2 border-dashed border-slate-200 space-y-3">
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
