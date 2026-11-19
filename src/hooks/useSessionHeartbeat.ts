// src/hooks/useSessionHeartbeat.ts
import { useEffect, useRef } from 'react';
import { authService, SessionStatusDto } from '../services/authService';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export function useSessionHeartbeat(
    hasCachedSession: boolean,
    onStatusSync: (status: SessionStatusDto) => void,
    lastHeartbeatRef: React.MutableRefObject<number>
) {
    const hasCachedSessionRef = useRef(hasCachedSession);

    // Sync Ref
    useEffect(() => {
        hasCachedSessionRef.current = hasCachedSession;
    }, [hasCachedSession]);

    // Global Activity Listener
    useEffect(() => {
        const handleActivity = () => {
            if (hasCachedSessionRef.current) {
                const now = Date.now();
                if (now - lastHeartbeatRef.current > 2000) {
                    lastHeartbeatRef.current = now;
                    authService.refreshSessionActivity().catch((e) => logger.warn(`Heartbeat failed: ${stringifyError(e)}`));
                }
            }
        };

        window.addEventListener('mousemove', handleActivity);
        window.addEventListener('keydown', handleActivity);
        window.addEventListener('click', handleActivity);

        return () => {
            window.removeEventListener('mousemove', handleActivity);
            window.removeEventListener('keydown', handleActivity);
            window.removeEventListener('click', handleActivity);
        };
    }, [lastHeartbeatRef]);

    // Polling for Session Status
    useEffect(() => {
        const checkStatus = async () => {
            if (hasCachedSessionRef.current) {
                try {
                    const status = await authService.getSessionStatus();
                    if (!status.hasCachedSession) {
                        logger.info("Session background check: Session expired.");
                        onStatusSync(status);
                    }
                } catch (e) {
                    logger.error(`Session check failed: ${stringifyError(e)}`);
                }
            }
        };

        const interval = setInterval(checkStatus, 10000);
        return () => clearInterval(interval);
    }, [onStatusSync]);
}
