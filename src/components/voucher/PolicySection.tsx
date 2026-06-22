import { Info } from 'lucide-react';
import { Card } from '../ui/Card';
import { useTranslation } from 'react-i18next';

interface PolicySectionProps {
  description?: string;
  footnote?: string;
}

export function PolicySection({ description, footnote }: PolicySectionProps) {
  const { t } = useTranslation();

  if (!description && !footnote) return null;

  return (
    <Card className="border-none shadow-premium" header={
      <div className="flex items-center gap-2">
        <Info size={18} className="text-theme-primary" />
        <span className="font-black text-xs uppercase tracking-widest">{t('voucher.policyConditionsHeader')}</span>
      </div>
    }>
      <div className="space-y-6 p-2">
        {description && (
          <div className="space-y-2">
            <p className="text-[10px] font-black text-theme-light uppercase tracking-widest opacity-60">{t('common.description')}</p>
            <p className="text-sm text-theme-secondary leading-relaxed italic">
              "{description}"
            </p>
          </div>
        )}
        {footnote && (
          <div className="space-y-2 pt-4 border-t border-theme-subtle/30">
            <p className="text-[10px] font-black text-theme-light uppercase tracking-widest opacity-60">{t('voucher.footnoteLabel')}</p>
            <p className="text-xs text-theme-light leading-relaxed">
              {footnote}
            </p>
          </div>
        )}
      </div>
    </Card>
  );
}
