// src/components/marketplace/RenewAdModal.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Clock, RefreshCw } from 'lucide-react';
import { FrontendMyAd } from '../../types';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { SegmentedTabs } from '../ui/SegmentedTabs';

export interface RenewAdModalProps {
    isOpen: boolean;
    onClose: () => void;
    ad: FrontendMyAd | null;
    onRenew: (id: string, durationDays: number) => Promise<void>;
}

export const RenewAdModal: React.FC<RenewAdModalProps> = ({
    isOpen,
    onClose,
    ad,
    onRenew,
}) => {
    const { t } = useTranslation();
    const [durationDays, setDurationDays] = useState<number>(90);
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    if (!ad) return null;

    const targetExpiryDate = new Date(Date.now() + durationDays * 86_400 * 1000).toLocaleDateString();

    const handleConfirm = async () => {
        setIsSubmitting(true);
        setError(null);
        try {
            await onRenew(ad.id, durationDays);
            onClose();
        } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            if (msg.includes('market.adLimitReached')) {
                setError(t('marketplace.adLimitReached', 'Das Limit von 5 aktiven Inseraten ist erreicht.'));
            } else {
                setError(msg);
            }
        } finally {
            setIsSubmitting(false);
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="sm"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <RefreshCw size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">
                            {ad.status === 'expired'
                                ? t('marketplace.republishAdTitle', 'Inserat erneut veröffentlichen')
                                : t('marketplace.renewAdTitle', 'Inserat verlängern')}
                        </h2>
                        <p className="text-xs text-theme-light truncate max-w-xs">{ad.title}</p>
                    </div>
                </div>
            }
            footer={
                <div className="flex items-center justify-end gap-2 w-full">
                    <Button variant="secondary" onClick={onClose} disabled={isSubmitting} size="sm">
                        {t('common.cancel', 'Abbrechen')}
                    </Button>
                    <Button variant="primary" onClick={handleConfirm} disabled={isSubmitting} size="sm">
                        {isSubmitting ? t('common.saving', 'Speichern...') : t('marketplace.renewConfirmBtn', 'Jetzt verlängern')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-4 pt-2">
                {error && (
                    <div className="text-xs p-3 rounded-xl bg-rose-50 text-rose-800 border border-rose-200">
                        {error}
                    </div>
                )}

                <div className="space-y-2">
                    <label className="text-[11px] font-black text-theme-light uppercase tracking-wider block">
                        {t('marketplace.renewDurationLabel', 'Verlängerungsdauer')}
                    </label>
                    <SegmentedTabs<string>
                        fullWidth
                        activeTab={String(durationDays)}
                        onChange={(tab) => setDurationDays(Number(tab))}
                        tabs={[
                            { id: '30', label: t('marketplace.duration30Days', '30 Tage') },
                            { id: '90', label: t('marketplace.duration90Days', '90 Tage') },
                            { id: '180', label: t('marketplace.duration180Days', '180 Tage') },
                        ]}
                    />
                </div>

                {/* Live preview banner */}
                <div className="p-3.5 rounded-xl bg-theme-primary/5 border border-theme-primary/15 flex items-center gap-2.5 text-xs text-theme-secondary">
                    <Clock size={16} className="text-theme-primary shrink-0" />
                    <span>
                        {t('marketplace.renewLivePreview', 'Läuft dann bis {{date}}', {
                            date: targetExpiryDate,
                        })}
                    </span>
                </div>
            </div>
        </ModalShell>
    );
};
