// src/utils/appEvents.ts
import { useEffect, useRef } from 'react';
import type { FrontendAutoBackupStatusDto } from '../types';

export const APP_EVENTS = {
    POST_INBOX_UPDATED: 'humoco:post-inbox-updated',
    CONTACTS_UPDATED: 'humoco:contacts-updated',
    BACKUP_UPDATED: 'humoco:backup-updated',
    MARKET_CARD_UPDATED: 'humoco:market-card-updated',
    SYNC_COMPLETED: 'humoco:sync-completed',
    WOT_UPDATED: 'humoco:wot-updated',
    STANDARDS_UPDATED: 'humoco:standards-updated',
    SESSION_LOCKED: 'humoco:session-locked',
    CONFLICTS_UPDATED: 'humoco:conflicts-updated',
} as const;

export type AppEventName = (typeof APP_EVENTS)[keyof typeof APP_EVENTS];

export interface AppEventDetailMap {
    [APP_EVENTS.POST_INBOX_UPDATED]: undefined;
    [APP_EVENTS.CONTACTS_UPDATED]: undefined;
    [APP_EVENTS.BACKUP_UPDATED]: FrontendAutoBackupStatusDto | undefined;
    [APP_EVENTS.MARKET_CARD_UPDATED]: undefined;
    [APP_EVENTS.SYNC_COMPLETED]: { source?: 'market' | 'post' | 'members' } | undefined;
    [APP_EVENTS.WOT_UPDATED]: undefined;
    [APP_EVENTS.STANDARDS_UPDATED]: undefined;
    [APP_EVENTS.SESSION_LOCKED]: undefined;
    [APP_EVENTS.CONFLICTS_UPDATED]: undefined;
}

/**
 * Dispatches a typed CustomEvent on `window` (no-op in SSR/non-browser environments).
 */
export function emitAppEvent<K extends AppEventName>(
    eventName: K,
    detail?: AppEventDetailMap[K],
): void {
    if (typeof window === 'undefined') {
        return;
    }
    window.dispatchEvent(new CustomEvent(eventName, { detail }));
}

/**
 * Subscribes a React component to a typed app event without re-subscribing
 * when the handler callback identity changes.
 */
export function useAppEvent<K extends AppEventName>(
    eventName: K,
    handler: (detail: AppEventDetailMap[K], event: CustomEvent<AppEventDetailMap[K]>) => void,
    enabled: boolean = true,
): void {
    const handlerRef = useRef(handler);
    handlerRef.current = handler;

    useEffect(() => {
        if (!enabled || typeof window === 'undefined') {
            return;
        }
        const listener = (event: Event) => {
            const customEvent = event as CustomEvent<AppEventDetailMap[K]>;
            handlerRef.current(customEvent.detail, customEvent);
        };
        window.addEventListener(eventName, listener);
        return () => {
            window.removeEventListener(eventName, listener);
        };
    }, [eventName, enabled]);
}
