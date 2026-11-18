import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { ModalShell } from './ModalShell';
import { HelpCircle, ShieldAlert } from 'lucide-react';

interface PrefixInfoModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export function PrefixInfoModal({ isOpen, onClose }: PrefixInfoModalProps) {
    const { t } = useTranslation();

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="xl"
            zIndexClass="z-[100]"
            className="bg-white rounded-[32px]"
            title={
                <div className="flex items-center gap-4">
                    <div className="p-3 bg-theme-primary/10 rounded-2xl text-theme-primary shrink-0">
                        <HelpCircle size={28} />
                    </div>
                    <h2 className="text-2xl font-black text-theme-primary tracking-tight">
                        {t('common.prefixInfoModalTitle')}
                    </h2>
                </div>
            }
            description={
                <p className="text-theme-light font-medium mt-1">
                    {t('common.prefixInfoModalSubtitle')}
                </p>
            }
            footer={
                <div className="flex justify-center">
                    <Button onClick={onClose} className="px-12 py-4 rounded-2xl shadow-premium-lg">
                        {t('common.prefixInfoUnderstandBtn')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-6 py-2">
                <div className="space-y-2">
                    <h3 className="font-bold text-base text-theme-primary">{t('common.prefixInfoQ1')}</h3>
                    <p className="text-sm text-theme-secondary leading-relaxed" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoA1') }} />
                </div>

                <div className="space-y-2">
                    <h3 className="font-bold text-base text-theme-primary">{t('common.prefixInfoQ2')}</h3>
                    <p className="text-sm text-theme-secondary leading-relaxed" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoA2') }} />
                </div>

                <div className="space-y-2">
                    <h3 className="font-bold text-base text-theme-primary">{t('common.prefixInfoQ3')}</h3>
                    <p className="text-sm text-theme-secondary leading-relaxed" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoA3') }} />
                </div>

                <div className="space-y-2">
                    <h3 className="font-bold text-base text-theme-primary">{t('common.prefixInfoQ4')}</h3>
                    <p className="text-sm text-theme-secondary leading-relaxed" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoA4') }} />
                </div>

                <div className="p-5 bg-rose-50 border border-rose-100 rounded-2xl space-y-3">
                    <div className="flex items-center gap-2 text-rose-600">
                        <ShieldAlert size={18} />
                        <h4 className="font-black text-xs uppercase tracking-wider">{t('common.prefixInfoSafetyTitle')}</h4>
                    </div>
                    <p className="text-xs text-rose-900 leading-relaxed font-medium" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoSafety1') }} />
                    <p className="text-xs text-rose-800 leading-relaxed" dangerouslySetInnerHTML={{ __html: t('common.prefixInfoSafety2') }} />
                </div>
            </div>
        </ModalShell>
    );
}
