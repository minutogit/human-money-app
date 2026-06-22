import { FileSignature, History, CheckCircle2, Trash2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { HelpIcon } from '../ui/HelpIcon';
import { useTranslation } from 'react-i18next';
import { VoucherDetails, VoucherStatus } from '../../types';

interface TimelineSectionProps {
  signatures: VoucherDetails['voucher']['signatures'];
  transactions: VoucherDetails['voucher']['transactions'];
  displayCurrency: string;
  onRemoveSignature: (id: string) => void;
  voucherStatus: VoucherStatus;
}

export function TimelineSection({
  signatures,
  transactions,
  displayCurrency,
  onRemoveSignature,
  voucherStatus
}: TimelineSectionProps) {
  const { t } = useTranslation();
  const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : t('common.na');

  return (
    <div className="space-y-6">
      <Card className="border-none shadow-premium" header={
        <div className="flex items-center gap-1.5">
          <FileSignature size={18} className="text-theme-primary" />
          <span className="font-black text-xs uppercase tracking-widest">{t('voucher.signaturesHeader')}</span>
          <HelpIcon topic="voucherSignatures" size={13} />
        </div>
      }>
        <div className="space-y-4">
          {signatures.map((sig) => {
            const isCreator = sig.role === "creator" || sig.role === "issuer";
            return (
              <div key={sig.signatureId} className="relative pl-8 pb-4 last:pb-0">
                <div className="absolute left-[11px] top-2 bottom-0 w-0.5 bg-theme-subtle/30"></div>
                <div className={`absolute left-0 top-1.5 w-[24px] h-[24px] rounded-full flex items-center justify-center border-4 border-bg-app z-10 ${isCreator ? 'bg-theme-primary text-white' : 'bg-theme-subtle text-theme-light'}`}>
                  <CheckCircle2 size={10} />
                </div>
                
                <div className="bg-white/40 border border-theme-subtle/30 rounded-2xl p-4 flex items-center justify-between group hover:border-theme-primary/30 transition-all shadow-sm">
                  <div>
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-[10px] font-black uppercase tracking-widest text-theme-light">
                        {t(`voucher.role.${sig.role}`, { defaultValue: sig.role })}
                      </span>
                      <span className="text-xs font-bold text-theme-secondary">
                        {sig.details?.firstName} {sig.details?.lastName}
                      </span>
                    </div>
                    <p className="text-[10px] font-mono text-theme-light opacity-50 truncate max-w-[200px]">{sig.signerId}</p>
                  </div>
                  <div className="flex items-center gap-3">
                    <span className="text-[10px] font-bold text-theme-light opacity-60">{formatDateTime(sig.signatureTime)}</span>
                    {voucherStatus === "incomplete" && !isCreator && (
                      <button 
                        className="p-1.5 rounded-lg opacity-0 group-hover:opacity-100 hover:bg-rose-50 text-rose-500 transition-all"
                        onClick={() => onRemoveSignature(sig.signatureId)}
                        data-testid="remove-signature"
                      >
                        <Trash2 size={14} />
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </Card>

      <Card className="border-none shadow-premium" header={
        <div className="flex items-center gap-2">
          <History size={18} className="text-theme-primary" />
          <span className="font-black text-xs uppercase tracking-widest">{t('voucher.transactionHistoryHeader')}</span>
        </div>
      }>
        <div className="space-y-3 max-h-[300px] overflow-y-auto pr-2 custom-scrollbar">
          {transactions.slice().reverse().map((tItem) => (
            <div key={tItem.tId} className="p-3 bg-white/40 border border-theme-subtle/30 rounded-xl relative">
              <div className="flex justify-between items-start mb-2">
                <span className={`text-[8px] font-black uppercase px-2 py-0.5 rounded-full ${tItem.tType === 'init' ? 'bg-emerald-500 text-white' : 'bg-theme-secondary text-white'}`}>
                  {t(`voucher.txType.${tItem.tType}`, { defaultValue: tItem.tType })}
                </span>
                <span className="text-xs font-black text-theme-primary">
                  {tItem.amount} {displayCurrency}
                </span>
              </div>
              <div className="space-y-1">
                <p className="text-[9px] text-theme-light flex items-center justify-between">
                  <span>{tItem.senderId ? `${t('transfer.from')}: ${tItem.senderId.slice(0, 12)}...` : `${t('transfer.from')}: SYSTEM`}</span>
                  <span>{formatDateTime(tItem.tTime).split(',')[0]}</span>
                </p>
              </div>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
