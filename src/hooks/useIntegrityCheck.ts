// src/hooks/useIntegrityCheck.ts
import { useState, useCallback, useRef } from 'react';
import { integrityService } from '../services/integrityService';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';
import { IntegrityReport } from '../types';

export interface CheckIntegrityOptions {
    allowSilentRepair?: boolean;
}

export interface UseIntegrityCheckOptions {
    getSessionGen?: () => number;
}

export function useIntegrityCheck(hookOptions?: UseIntegrityCheckOptions) {
    const [integrityReport, setIntegrityReport] = useState<IntegrityReport | null>(null);
    const silentRepairAttempted = useRef(false);
    const getSessionGenRef = useRef(hookOptions?.getSessionGen);
    getSessionGenRef.current = hookOptions?.getSessionGen;

    const resetIntegrity = useCallback(() => {
        silentRepairAttempted.current = false;
        setIntegrityReport(null);
    }, []);

    const checkIntegrity = useCallback(async (options?: CheckIntegrityOptions) => {
        const expectedGen = getSessionGenRef.current?.();
        const isCurrent = () =>
            expectedGen === undefined || getSessionGenRef.current?.() === expectedGen;
        try {
            const report = await integrityService.checkIntegrity();
            if (!isCurrent()) return;
            if (
                options?.allowSilentRepair === true &&
                report.type === 'missingIntegrityRecord' &&
                !silentRepairAttempted.current
            ) {
                silentRepairAttempted.current = true;
                try {
                    await integrityService.repairWalletIntegrity(undefined);
                    if (!isCurrent()) return;
                    const refreshed = await integrityService.checkIntegrity();
                    if (!isCurrent()) return;
                    setIntegrityReport(refreshed);
                    if (refreshed.type !== 'valid') {
                        logger.warn(`Integrity issue persisted after silent repair: ${refreshed.type}`);
                    }
                    return;
                } catch (repairErr) {
                    if (!isCurrent()) return;
                    logger.warn(`Silent integrity repair skipped: ${stringifyError(repairErr)}`);
                }
            }
            if (!isCurrent()) return;
            setIntegrityReport(report);
            if (report.type !== 'valid') {
                logger.warn(`Integrity issue detected: ${report.type}`);
            }
        } catch (e) {
            logger.error(`Failed to check integrity: ${stringifyError(e)}`);
        }
    }, []);

    return { integrityReport, checkIntegrity, setIntegrityReport, resetIntegrity };
}
