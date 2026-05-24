import { useTranslation } from "react-i18next";
import { ShieldAlert } from "lucide-react";
import { Card } from "../ui/Card";
import { Input } from "../ui/Input";
import { CollateralData } from "../../types";

interface CollateralFormProps {
    collateral: CollateralData;
    onCollateralChange: (collateral: CollateralData) => void;
}

export function CollateralForm({
    collateral,
    onCollateralChange
}: CollateralFormProps) {
    const { t } = useTranslation();
    const handleChange = (field: string, value: string) => {
        onCollateralChange({ ...collateral, [field]: value });
    };

    return (
        <Card header={
            <div className="flex items-center gap-2">
                <ShieldAlert size={18} className="text-theme-primary" />
                <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('voucher.collateral.header')}</span>
            </div>
        }>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.collateral.amountLabel')}</label>
                    <Input type="number" value={collateral.amount} onChange={(e) => handleChange('amount', e.target.value)} />
                </div>
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.collateral.unitLabel')}</label>
                    <Input value={collateral.unit} onChange={(e) => handleChange('unit', e.target.value)} placeholder={t('voucher.collateral.unitPlaceholder')} />
                </div>
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.collateral.abbreviationLabel')}</label>
                    <Input value={collateral.abbreviation || ""} onChange={(e) => handleChange('abbreviation', e.target.value)} placeholder={t('voucher.collateral.abbreviationPlaceholder')} />
                </div>
            </div>
        </Card>
    );
}
