import React from 'react';
import { useTranslation } from 'react-i18next';
import { Users } from 'lucide-react';
import type { ActiveRecoverySessionDto } from '../../../types';
import { AlertBanner } from '../../ui/AlertBanner';
import { Button } from '../../ui/Button';

interface StartRecoveryStepProps {
    conflictingSession: ActiveRecoverySessionDto | null;
    isSubmitting: boolean;
    isLoadingSession: boolean;
    onStartSession: () => void;
}

export const StartRecoveryStep: React.FC<StartRecoveryStepProps> = ({
    conflictingSession,
    isSubmitting,
    isLoadingSession,
    onStartSession,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4">
            {conflictingSession && (
                <AlertBanner variant="warning" data-testid="social-conflicting-session-warning">
                    {t('backup.conflictingSessionWarning')}
                </AlertBanner>
            )}
            <p className="text-xs text-theme-light leading-relaxed">
                {t('backup.socialRecoveryDesc')}
            </p>
            <div className="p-4 bg-bg-surface border border-theme-subtle rounded-2xl space-y-2 text-xs text-theme-secondary">
                <p className="font-bold text-theme-secondary">{t('backup.socialOverviewTitle')}</p>
                <ul className="space-y-1.5 text-theme-light leading-relaxed">
                    <li>{t('backup.socialOverviewStep1')}</li>
                    <li>{t('backup.socialOverviewStep2')}</li>
                    <li>{t('backup.socialOverviewStep3')}</li>
                </ul>
            </div>
            <Button
                type="button"
                variant="primary"
                data-testid="start-social-recovery-btn"
                onClick={onStartSession}
                disabled={isSubmitting || isLoadingSession}
                className="flex items-center gap-2"
            >
                <Users size={16} />
                {isSubmitting ? t('common.loading') : t('backup.startRecoverySession')}
            </Button>
        </div>
    );
};
