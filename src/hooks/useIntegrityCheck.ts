// src/hooks/useIntegrityCheck.ts
import { useState, useCallback } from 'react';
import { integrityService } from '../services/integrityService';
import { logger } from '../utils/log';
import { IntegrityReport } from '../types';

export function useIntegrityCheck() {
    const [integrityReport, setIntegrityReport] = useState<IntegrityReport | null>(null);

    const checkIntegrity = useCallback(async () => {
        try {
            const report = await integrityService.checkIntegrity();
            setIntegrityReport(report);
            if (report.type !== 'valid') {
                logger.warn(`Integrity issue detected: ${report.type}`);
            }
        } catch (e) {
            logger.error(`Failed to check integrity: ${e}`);
        }
    }, []);

    return { integrityReport, checkIntegrity, setIntegrityReport };
}
