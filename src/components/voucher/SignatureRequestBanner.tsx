import React from 'react';
import { FileSignature, Upload, CheckCircle2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { useTranslation } from 'react-i18next';

interface SignatureRequestBannerProps {
    onAction: (e: React.MouseEvent) => void;
    onImportSignature?: (e: React.MouseEvent) => void;
    hint?: string | null;
    progressText?: string | null;
}

export const SignatureRequestBanner: React.FC<SignatureRequestBannerProps> = ({
    onAction,
    onImportSignature,
    hint,
    progressText
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-3">
            <Card variant="accent" className="p-5 flex flex-col lg:flex-row items-center justify-between gap-5 border-none shadow-premium">
                <div className="flex items-center gap-4">
                    <div className="p-3.5 bg-theme-accent/10 rounded-2xl text-theme-accent shrink-0">
                        <FileSignature size={26} />
                    </div>
                    <div className="space-y-1">
                        <p className="text-[10px] font-black uppercase tracking-widest text-theme-accent">{t('auth.actionRequired')}</p>
                        <div className="flex items-center gap-1.5 flex-wrap">
                            <p className="text-base font-bold text-theme-secondary">{t('voucher.needsMoreSignatures')}</p>
                            <HelpIcon topic="signatureRequest" size={14} />
                        </div>
                        {progressText && (
                            <div className="inline-flex items-center gap-1.5 px-3 py-1 bg-amber-100 text-amber-900 border border-amber-200/60 rounded-full text-xs font-bold mt-1">
                                <CheckCircle2 size={13} className="text-amber-700 shrink-0" />
                                <span>{progressText}</span>
                            </div>
                        )}
                    </div>
                </div>
                <div className="flex items-center gap-2.5 w-full lg:w-auto justify-end flex-wrap">
                    {onImportSignature && (
                        <Button 
                            size="sm" 
                            variant="outline"
                            onClick={onImportSignature}
                            className="bg-white/90 hover:bg-white text-xs font-bold gap-2 px-4 shadow-sm"
                        >
                            <Upload size={14} />
                            {t('voucher.importSignatureBtn')}
                        </Button>
                    )}
                    <Button 
                        size="sm" 
                        variant="primary"
                        onClick={onAction}
                        className="text-xs font-bold gap-2 px-5"
                    >
                        <FileSignature size={14} />
                        {t('voucher.requestSignaturesBtn')}
                    </Button>
                </div>
            </Card>
            
            {hint && (
                <div className="p-3 bg-blue-50/50 rounded-xl border border-blue-100 flex items-start gap-3">
                    <span className="text-blue-500 text-sm mt-0.5">💡</span>
                    <p className="text-xs text-blue-800 font-medium leading-normal">
                        {hint}
                    </p>
                </div>
            )}
        </div>
    );
};

