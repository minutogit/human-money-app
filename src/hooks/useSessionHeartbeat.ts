// src/hooks/useSessionHeartbeat.ts
import { useEffect, useRef } from 'react';
import { authService } from '../services/authService';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export function useSessionHeartbeat(
    isSessionActive: boolean,
    onSessionExpired: () => void,
    lastHeartbeatRef: React.MutableRefObject<number>
) {
    const isSessionActiveRef = useRef(isSessionActive);

    // Sync Ref
    useEffect(() => {
        isSessionActiveRef.current = isSessionActive;
    }, [isSessionActive]);

    // Global Activity Listener
    useEffect(() => {
        const handleActivity = () => {
            if (isSessionActiveRef.current) {
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
            if (isSessionActiveRef.current) {
                try {
                    const active = await authService.isSessionActive();
                    if (!active) {
                        logger.info("Session background check: Session expired.");
                        onSessionExpired();
                    }
                } catch (e) {
                    logger.error(`Session check failed: ${stringifyError(e)}`);
                }
            }
        };

        const interval = setInterval(checkStatus, 10000);
        return () => clearInterval(interval);
    }, [onSessionExpired]);
}
