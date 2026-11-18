// src/components/wallet/dashboard/DashboardActionsGrid.tsx
import { useTranslation } from 'react-i18next';
import { Plus, ArrowUpRight, ArrowDownLeft } from 'lucide-react';
import { Button } from '../../ui/Button';
import type { useNavigation } from '../../../context/NavigationContext';
import type { AggregatedBalance } from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

interface DashboardActionsGridProps {
    balances: AggregatedBalance[];
    feedbackMsg: string;
    onProtectedNavigate: (target: Parameters<NavigateFn>[0]) => void;
    onNavigate: NavigateFn;
}

export function DashboardActionsGrid({
    balances,
    feedbackMsg,
    onProtectedNavigate,
    onNavigate,
}: DashboardActionsGridProps) {
    const { t } = useTranslation();

    return (
        <>
            <div
                className="fixed bottom-0 inset-x-0 z-30 p-3 bg-white/95 dark:bg-gray-900/95 backdrop-blur-md border-t border-theme-subtle flex items-center justify-center gap-3 md:static md:p-0 md:bg-transparent md:border-0 md:mb-6"
                data-testid="dashboard-primary-actions"
            >
                <Button
                    onClick={() => onProtectedNavigate({ view: 'send_vouchers' })}
                    className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                    size="md"
                    data-testid="dashboard-action-send"
                >
                    <ArrowUpRight size={18} />
                    <span>{t('transfer.send')}</span>
                </Button>
                <Button
                    onClick={() => onNavigate({ view: 'receive_bundle' })}
                    className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                    variant="secondary"
                    size="md"
                    data-testid="dashboard-action-receive"
                >
                    <ArrowDownLeft size={18} />
                    <span>{t('transfer.receive')}</span>
                </Button>
                <Button
                    onClick={() =>
                        onProtectedNavigate({
                            view: 'create_voucher',
                            previousView: { view: 'logged_in' },
                        })
                    }
                    className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                    variant="secondary"
                    size="md"
                    data-testid="dashboard-action-create"
                >
                    <Plus size={18} />
                    <span>{t('common.create')}</span>
                </Button>
            </div>

            {balances.length === 0 && (
                <div className="px-2 pt-2 text-center">
                    <h1 className="text-3xl font-black text-theme-primary tracking-tighter">
                        {t('dashboard.welcomeTitle')}
                    </h1>
                    <p className="text-sm font-medium text-theme-light mt-1">
                        {t('dashboard.welcomeSubtitle')}
                    </p>
                </div>
            )}

            {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}
        </>
    );
}
