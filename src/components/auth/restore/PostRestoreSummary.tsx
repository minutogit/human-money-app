import React from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, CheckCircle2, RefreshCw } from 'lucide-react';
import { RestoreOutcomeDto } from '../../../types';
import { Button } from '../../ui/Button';

export interface PostRestoreSummaryProps {
    outcome: RestoreOutcomeDto;
    isReconcilingL2: boolean;
    onReconcileL2: () => void;
    onOpenWallet: () => void;
}

/**
 * Unified restore completion summary showing restored voucher/standard counts
 * and optional Layer-2 online reconciliation trigger.
 */
export const PostRestoreSummary: React.FC<PostRestoreSummaryProps> = ({
    outcome,
    isReconcilingL2,
    onReconcileL2,
    onOpenWallet,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-6 text-center animate-in zoom-in-95 duration-300">
            <div className="w-16 h-16 rounded-full bg-emerald-100 text-emerald-600 flex items-center justify-center mx-auto shadow-inner">
                <CheckCircle2 size={36} />
            </div>

            <div className="space-y-1">
                <h2 className="text-2xl font-black text-theme-secondary">
                    {t('profile.backupRestoreSuccessTitle')}
                </h2>
                <p className="text-xs text-theme-light max-w-md mx-auto">
                    {t('profile.backupRestoreSuccessDesc')}
                </p>
            </div>

            <div className="grid grid-cols-3 gap-3 p-4 bg-slate-50 border border-slate-200 rounded-2xl max-w-md mx-auto">
                <div>
                    <p className="text-lg font-black text-theme-primary">
                        {outcome.activeVoucherQueriesCount}
                    </p>
                    <p className="text-[10px] uppercase font-bold text-theme-light">
                        {t('backup.active')}
                    </p>
                </div>
                <div>
                    <p className="text-lg font-black text-slate-700">
                        {outcome.reconciledSpentCount}
                    </p>
                    <p className="text-[10px] uppercase font-bold text-theme-light">
                        {t('backup.spent')}
                    </p>
                </div>
                <div>
                    <p className="text-lg font-black text-slate-700">
                        {outcome.restoredStandardsCount}
                    </p>
                    <p className="text-[10px] uppercase font-bold text-theme-light">
                        {t('backup.standards')}
                    </p>
                </div>
            </div>

            <div className="p-3.5 rounded-xl bg-slate-100 text-xs text-theme-secondary max-w-md mx-auto leading-relaxed">
                {outcome.l2ReconciliationStatus === 'no_servers'
                    ? t('profile.backupRestoreOfflineNotice')
                    : outcome.l2ReconciliationStatus === 'reconciled'
                      ? t('profile.backupRestoreReconciledNotice', {
                            active: outcome.activeVoucherQueriesCount,
                            spent: outcome.reconciledSpentCount,
                        })
                      : t('profile.backupRestorePendingNotice')}
            </div>

            {outcome.l2ReconciliationStatus !== 'reconciled' &&
                outcome.activeVoucherQueriesCount > 0 && (
                    <div className="p-4 rounded-2xl bg-amber-50 border border-amber-200 text-amber-900 text-xs text-left space-y-2 max-w-md mx-auto">
                        <div className="flex items-center gap-2 font-bold text-amber-800">
                            <AlertTriangle size={16} className="shrink-0" />
                            <span>{t('profile.backupRestoreL2WarningTitle')}</span>
                        </div>
                        <p className="leading-relaxed">
                            {t('profile.backupRestoreL2WarningDesc')}
                        </p>
                        <Button
                            data-testid="restore-reconcile-l2-btn"
                            variant="secondary"
                            size="sm"
                            disabled={isReconcilingL2}
                            onClick={onReconcileL2}
                            className="w-full mt-1"
                        >
                            {isReconcilingL2 ? (
                                <>
                                    <RefreshCw size={14} className="animate-spin mr-2" />
                                    {t('profile.backupRestoreReconcilingL2')}
                                </>
                            ) : (
                                <>
                                    <RefreshCw size={14} className="mr-2" />
                                    {t('profile.backupRestoreReconcileL2Btn')}
                                </>
                            )}
                        </Button>
                    </div>
                )}

            <Button
                variant="primary"
                onClick={onOpenWallet}
                className="w-full max-w-xs mx-auto py-3 text-sm shadow-md"
            >
                {t('profile.backupRestoreOpenWallet')}
            </Button>
        </div>
    );
};
