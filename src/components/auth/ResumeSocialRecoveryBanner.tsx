import React from 'react';
import { useTranslation } from 'react-i18next';
import { AlertBanner } from '../ui/AlertBanner';
import { Button } from '../ui/Button';
import { ActiveRecoverySessionDto } from '../../types';

export interface ResumeSocialRecoveryBannerProps {
    /** Active recovery session to resume; renders nothing when null. */
    session: ActiveRecoverySessionDto | null;
    /** Callback invoked when the user clicks the resume button. */
    onResume: () => void;
    /** Optional test ID for the outer AlertBanner. */
    bannerTestId?: string;
    /** Optional test ID for the resume action Button. */
    buttonTestId?: string;
}

/**
 * Shared informational banner prompting the user to resume an in-progress
 * social recovery session across Login, CreateNewProfile, and RestoreBackupView.
 */
export const ResumeSocialRecoveryBanner: React.FC<ResumeSocialRecoveryBannerProps> = ({
    session,
    onResume,
    bannerTestId,
    buttonTestId,
}) => {
    const { t } = useTranslation();

    if (!session) {
        return null;
    }

    return (
        <AlertBanner
            variant="info"
            data-testid={bannerTestId}
            action={
                <Button
                    type="button"
                    variant="primary"
                    size="xs"
                    data-testid={buttonTestId}
                    onClick={onResume}
                >
                    {t('backup.resumeSessionBtn')}
                </Button>
            }
        >
            {t('backup.resumeSessionDesc', {
                code: session.sasCode,
                collected: session.collectedCount,
                required: session.thresholdM ?? '?',
            })}
        </AlertBanner>
    );
};
