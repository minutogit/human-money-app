// src/hooks/useIntegrityCheck.ts
import { useState, useCallback, useRef } from 'react';
import { integrityService } from '../services/integrityService';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';
import { IntegrityReport } from '../types';

export function useIntegrityCheck() {
    const [integrityReport, setIntegrityReport] = useState<IntegrityReport | null>(null);
    const silentRepairAttempted = useRef(false);

    const checkIntegrity = useCallback(async () => {
        try {
            const report = await integrityService.checkIntegrity();
            if (report.type === 'missingIntegrityRecord' && !silentRepairAttempted.current) {
                silentRepairAttempted.current = true;
                try {
                    await integrityService.repairWalletIntegrity(undefined);
                    const refreshed = await integrityService.checkIntegrity();
                    setIntegrityReport(refreshed);
                    if (refreshed.type !== 'valid') {
                        logger.warn(`Integrity issue persisted after silent repair: ${refreshed.type}`);
                    }
                    return;
                } catch (repairErr) {
                    logger.warn(`Silent integrity repair skipped: ${stringifyError(repairErr)}`);
                }
            }
            setIntegrityReport(report);
            if (report.type !== 'valid') {
                logger.warn(`Integrity issue detected: ${report.type}`);
            }
        } catch (e) {
            logger.error(`Failed to check integrity: ${stringifyError(e)}`);
        }
    }, []);

    return { integrityReport, checkIntegrity, setIntegrityReport };
}
