import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { HelpCircle, ShieldAlert } from 'lucide-react';

interface PrefixInfoModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export function PrefixInfoModal({ isOpen, onClose }: PrefixInfoModalProps) {
    const { t } = useTranslation();
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4 animate-in fade-in duration-300">
            <div className="w-full max-w-xl bg-white border border-theme-subtle rounded-[32px] overflow-hidden shadow-premium-2xl animate-in zoom-in-95 slide-in-from-bottom-10 duration-500">
                {/* Header */}
                <div className="bg-theme-primary/5 p-8 border-b border-theme-subtle">
                    <div className="flex items-center gap-4 mb-2">
                        <div className="p-3 bg-theme-primary/10 rounded-2xl text-theme-primary">
                            <HelpCircle size={28} />
                        </div>
                        <h2 className="text-2xl font-black text-theme-primary tracking-tight">{t('common.prefixInfoModalTitle')}</h2>
                    </div>
                    <p className="text-theme-light font-medium">{t('common.prefixInfoModalSubtitle')}</p>
                </div>

                {/* Content */}
                <div className="p-8 space-y-6 max-h-[60vh] overflow-y-auto custom-scrollbar">
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

                {/* Footer */}
                <div className="p-6 bg-theme-subtle/10 flex justify-center border-t border-theme-subtle">
                    <Button onClick={onClose} className="px-12 py-4 rounded-2xl shadow-premium-lg">
                        {t('common.prefixInfoUnderstandBtn')}
                    </Button>
                </div>
            </div>
        </div>
    );
}
