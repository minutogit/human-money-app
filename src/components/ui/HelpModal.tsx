import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { HelpCircle } from 'lucide-react';

interface HelpModalProps {
    isOpen: boolean;
    onClose: () => void;
    title: string;
    short: string;
    long: string;
}

export function HelpModal({ isOpen, onClose, title, short, long }: HelpModalProps) {
    const { t } = useTranslation();
    if (!isOpen) return null;

    return (
        <div 
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4 animate-in fade-in duration-300"
            onClick={onClose}
        >
            <div 
                className="w-full max-w-lg bg-white border border-theme-subtle rounded-[32px] overflow-hidden shadow-premium-2xl animate-in zoom-in-95 slide-in-from-bottom-10 duration-500"
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header */}
                <div className="bg-theme-primary/5 p-8 border-b border-theme-subtle">
                    <div className="flex items-center gap-4">
                        <div className="p-3 bg-theme-primary/10 rounded-2xl text-theme-primary shrink-0">
                            <HelpCircle size={28} />
                        </div>
                        <h2 className="text-xl font-black text-theme-primary tracking-tight leading-tight">{title}</h2>
                    </div>
                </div>

                {/* Content */}
                <div className="p-8 space-y-6 max-h-[50vh] overflow-y-auto custom-scrollbar">
                    <div className="space-y-4">
                        {/* Highlighted short summary */}
                        <p className="text-base font-semibold text-theme-primary leading-relaxed bg-theme-primary/5 p-4 rounded-2xl border border-theme-primary/10">
                            {short}
                        </p>
                        {/* Detailed long description */}
                        <p className="text-sm text-theme-secondary leading-relaxed whitespace-pre-line">
                            {long}
                        </p>
                    </div>
                </div>

                {/* Footer */}
                <div className="p-6 bg-theme-subtle/10 flex justify-center border-t border-theme-subtle">
                    <Button onClick={onClose} className="px-12 py-3.5 rounded-2xl shadow-premium-lg">
                        {t('common.close')}
                    </Button>
                </div>
            </div>
        </div>
    );
}
