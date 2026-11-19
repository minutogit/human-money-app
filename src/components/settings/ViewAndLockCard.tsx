// src/components/settings/ViewAndLockCard.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { SlidersHorizontal, Clock } from 'lucide-react';
import { AppConfigDto, SecurityPolicyPatch } from '../../types';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { UiModeToggle } from '../ui/UiModeToggle';

export interface SessionTimeoutControlProps {
    sessionTimeoutSeconds: number;
    onSaveTimeout: (seconds: number) => Promise<boolean | void> | void;
    inputId?: string;
    disabled?: boolean;
}

export const SessionTimeoutControl: React.FC<SessionTimeoutControlProps> = ({
    sessionTimeoutSeconds,
    onSaveTimeout,
    inputId = 'sessionTimeoutMinutesInput',
    disabled = false,
}) => {
    const { t } = useTranslation();
    const committedMinutes = Math.floor((sessionTimeoutSeconds || 0) / 60);
    const [minutesText, setMinutesText] = useState<string>(String(committedMinutes));
    const [isSaving, setIsSaving] = useState(false);

    useEffect(() => {
        setMinutesText(String(Math.floor((sessionTimeoutSeconds || 0) / 60)));
    }, [sessionTimeoutSeconds]);

    const trimmed = minutesText.trim();
    const parsed = trimmed === '' ? NaN : Number(trimmed);
    const isValid = Number.isFinite(parsed) && parsed >= 0;
    const targetSeconds = isValid ? Math.floor(parsed) * 60 : sessionTimeoutSeconds;
    const isDirty = isValid && targetSeconds !== sessionTimeoutSeconds;

    const handleSave = async () => {
        if (!isDirty || isSaving || disabled) return;
        setIsSaving(true);
        try {
            await onSaveTimeout(targetSeconds);
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div className="space-y-1">
                <label
                    htmlFor={inputId}
                    className="text-xs font-bold text-theme-secondary flex items-center gap-1.5"
                >
                    <Clock size={14} className="text-theme-primary" />
                    <span>{t('settings.timeoutIntervalLabel')}</span>
                </label>
                <p className="text-[11px] text-theme-light/80 font-medium leading-relaxed">
                    {t('settings.timeoutIntervalDescription')}
                </p>
            </div>
            <div className="flex items-center gap-2 w-full sm:w-auto shrink-0">
                <div className="w-full sm:w-28 shrink-0">
                    <Input
                        id={inputId}
                        type="number"
                        value={minutesText}
                        onChange={(e) => setMinutesText(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                                e.preventDefault();
                                void handleSave();
                            }
                        }}
                        min="0"
                        disabled={disabled || isSaving}
                        className="font-bold"
                    />
                </div>
                <Button
                    type="button"
                    size="sm"
                    data-testid="save-timeout-button"
                    onClick={() => void handleSave()}
                    disabled={!isDirty || isSaving || disabled}
                >
                    {t('common.save')}
                </Button>
            </div>
        </div>
    );
};

export interface ViewAndLockCardProps {
    config: AppConfigDto | null;
    onPatchSecurityPolicy: (patch: SecurityPolicyPatch) => Promise<boolean | void> | void;
}

export const ViewAndLockCard: React.FC<ViewAndLockCardProps> = ({
    config,
    onPatchSecurityPolicy,
}) => {
    const { t } = useTranslation();

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

                <div className="pt-4 border-t border-theme-subtle/50">
                    <SessionTimeoutControl
                        inputId="viewAndLockTimeoutInput"
                        sessionTimeoutSeconds={config?.securityPolicy.sessionTimeoutSeconds ?? 600}
                        onSaveTimeout={(seconds) =>
                            onPatchSecurityPolicy({ sessionTimeoutSeconds: seconds })
                        }
                    />
                </div>
            </div>
        </Card>
    );
};

export default ViewAndLockCard;
