// src/components/settings/ViewAndLockCard.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { SlidersHorizontal, Clock } from 'lucide-react';
import { AppSettings } from '../../types';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { UiModeToggle } from '../ui/UiModeToggle';

export interface ViewAndLockCardProps {
    settings: AppSettings | null;
    onPersistSettings: (updater: (prev: AppSettings | null) => AppSettings | null) => void;
}

export const ViewAndLockCard: React.FC<ViewAndLockCardProps> = ({
    settings,
    onPersistSettings,
}) => {
    const { t } = useTranslation();
    const initialMinutes = settings ? Math.floor((settings.sessionTimeoutSeconds || 0) / 60) : 10;
    const [minutesText, setMinutesText] = useState<string>(String(initialMinutes));

    useEffect(() => {
        if (settings) {
            setMinutesText(String(Math.floor((settings.sessionTimeoutSeconds || 0) / 60)));
        }
    }, [settings]);

    const handleMinutesChange = (raw: string) => {
        setMinutesText(raw);
        const parsed = parseInt(raw, 10);
        const safeMinutes = isNaN(parsed) || parsed < 0 ? 0 : parsed;
        onPersistSettings((prev) =>
            prev ? { ...prev, sessionTimeoutSeconds: safeMinutes * 60 } : prev
        );
    };

    return (
        <Card
            header={
                <div className="flex items-center gap-2">
                    <SlidersHorizontal size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">
                        {t('settings.viewModeTitle')}
                    </span>
                </div>
            }
        >
            <div className="space-y-5">
                <div className="space-y-2.5">
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('settings.viewModeSubtitle')}
                    </p>
                    <UiModeToggle variant="full" />
                </div>

                <div className="pt-4 border-t border-theme-subtle/50 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                    <div className="space-y-1">
                        <label
                            htmlFor="viewAndLockTimeoutInput"
                            className="text-xs font-bold text-theme-secondary flex items-center gap-1.5"
                        >
                            <Clock size={14} className="text-theme-primary" />
                            <span>{t('settings.timeoutIntervalLabel')}</span>
                        </label>
                        <p className="text-[11px] text-theme-light/80 font-medium leading-relaxed">
                            {t('settings.timeoutIntervalDescription')}
                        </p>
                    </div>
                    <div className="w-full sm:w-36 shrink-0">
                        <Input
                            id="viewAndLockTimeoutInput"
                            type="number"
                            value={minutesText}
                            onChange={(e) => handleMinutesChange(e.target.value)}
                            min="0"
                            className="font-bold"
                        />
                    </div>
                </div>
            </div>
        </Card>
    );
};

export default ViewAndLockCard;
