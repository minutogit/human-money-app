import { CollateralData } from "../../types";

interface CollateralFormProps {
    collateral: CollateralData;
    onCollateralChange: (collateral: CollateralData) => void;
}

export function CollateralForm({
    collateral: _collateral,
    onCollateralChange: _onCollateralChange
}: CollateralFormProps) {
    // TODO: Sicherheiten (Optional) sind noch nicht vollständig durchdacht und implementiert.
    // Daher wird dieser Abschnitt vorerst in der UI ausgeblendet.
    return null;
}

