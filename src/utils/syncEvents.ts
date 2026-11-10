// src/utils/syncEvents.ts

import { FrontendSyncOverviewStatus } from '../types';

export const SYNC_STATUS_UPDATED_EVENT = 'humoco:sync-status-updated';

export function notifySyncStatusUpdated(detail?: FrontendSyncOverviewStatus): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(SYNC_STATUS_UPDATED_EVENT, { detail }));
    }
}
