// src/components/wallet/dashboard/ActiveVouchersDisclosure.tsx
import { useTranslation } from 'react-i18next';
import { Layers } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Advanced } from '../../ui/Advanced';
import { Disclosure } from '../../ui/Disclosure';
import { formatAmount } from '../../../utils/format';
import { formatRelativeExpiry } from '../../../utils/walletEvents';
import type { useNavigation } from '../../../context/NavigationContext';
import type { VoucherSummary } from '../../../types';

type NavigateFn = ReturnType<typeof useNavigation>['navigate'];

interface ActiveVouchersDisclosureProps {
    activeVouchers: VoucherSummary[];
    resolveIdentity: (did?: string | null, providedName?: string | null) => string;
    onNavigate: NavigateFn;
}

export function ActiveVouchersDisclosure({
    activeVouchers,
    resolveIdentity,
    onNavigate,
}: ActiveVouchersDisclosureProps) {
    const { t } = useTranslation();

    if (activeVouchers.length === 0) return null;

    return (
        <section>
            <Disclosure
                toggleTestId="dashboard-vouchers-toggle"
                contentTestId="dashboard-vouchers-list"
                icon={<Layers size={16} className="text-theme-primary" />}
                title={
                    <span className="text-xs font-black text-theme-secondary uppercase tracking-wider">
                        {t('dashboard.showMyVouchers', { count: activeVouchers.length })}
                    </span>
                }
                className="bg-white/80 dark:bg-slate-900/80 border border-theme-subtle rounded-2xl overflow-hidden shadow-xs"
                buttonClassName="w-full flex items-center justify-between gap-3 px-5 py-3.5 text-left hover:bg-bg-app/50 transition-colors cursor-pointer"
                contentClassName="px-5 pb-4 pt-2 border-t border-theme-subtle/60 space-y-2.5"
            >
                <div className="space-y-2">
                    {activeVouchers.map((voucher) => {
                        const fullCreatorName = [
                            voucher.creatorFirstName,
                            voucher.creatorLastName,
                        ]
                            .filter(Boolean)
                            .join(' ');
                        const issuerLabel = fullCreatorName || resolveIdentity(voucher.creatorId);
                        const expiryLabel = formatRelativeExpiry(voucher.validUntil, t);
                        return (
                            <button
                                key={voucher.localInstanceId}
                                type="button"
                                data-testid="dashboard-voucher-row"
                                onClick={() =>
                                    onNavigate({
                                        view: 'voucher_details',
                                        voucherId: voucher.localInstanceId,
                                        previousView: { view: 'logged_in' },
                                    })
                                }
                                className="w-full text-left px-4 py-3 rounded-xl border border-theme-subtle/80 bg-white dark:bg-slate-900 hover:border-theme-primary/40 hover:bg-bg-app/40 transition-all flex items-center justify-between gap-3 cursor-pointer"
                            >
                                <div className="min-w-0 space-y-0.5">
                                    <div className="flex items-center gap-2 flex-wrap">
                                        <span className="font-bold text-sm text-theme-secondary tabular-nums">
                                            {formatAmount(voucher.currentAmount)}{' '}
                                            {voucher.displayCurrency}
                                        </span>
                                        {voucher.isTestVoucher && (
                                            <span className="px-1.5 py-0.5 text-[10px] font-bold uppercase rounded bg-amber-100 text-amber-800 border border-amber-200">
                                                {t('voucher.testVoucher')}
                                            </span>
                                        )}
                                    </div>
                                    <p className="text-xs text-theme-light truncate">
                                        {t('dashboard.voucherIssuedBy', { issuer: issuerLabel })}
                                    </p>
                                </div>
                                {expiryLabel && (
                                    <span className="text-[11px] font-medium text-theme-light shrink-0">
                                        {expiryLabel}
                                    </span>
                                )}
                            </button>
                        );
                    })}
                </div>

                <Advanced>
                    <div className="pt-1 flex justify-end">
                        <Button
                            variant="outline"
                            size="sm"
                            data-testid="dashboard-open-wallet-btn"
                            onClick={() => onNavigate({ view: 'wallet' })}
                        >
                            {t('dashboard.openFullVoucherList')}
                        </Button>
                    </div>
                </Advanced>
            </Disclosure>
        </section>
    );
}
