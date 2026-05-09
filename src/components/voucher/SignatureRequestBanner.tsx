import React from 'react';
import { FileSignature } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';

interface SignatureRequestBannerProps {
    onAction: (e: React.MouseEvent) => void;
    hint?: string | null;
}

export const SignatureRequestBanner: React.FC<SignatureRequestBannerProps> = ({
    onAction,
    hint
}) => {
    return (
        <div className="space-y-3">
            <Card variant="accent" className="p-4 flex flex-col sm:flex-row items-center justify-between gap-4 border-none shadow-premium">
                <div className="flex items-center gap-4">
                    <div className="p-3 bg-theme-accent/10 rounded-2xl text-theme-accent">
                        <FileSignature size={24} />
                    </div>
                    <div>
                        <p className="text-[10px] font-black uppercase tracking-widest text-theme-accent">Action Required</p>
                        <p className="text-base font-bold text-theme-secondary">Needs more signatures</p>
                    </div>
                </div>
                <Button 
                    size="sm" 
                    variant="primary"
                    onClick={onAction}
                    className="px-6"
                >
                    Request Signatures
                </Button>
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
