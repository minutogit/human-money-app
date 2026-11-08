// src/utils/dropEvents.ts

import { FrontendDropOverviewStatus } from '../types';

export const DROP_STATUS_UPDATED_EVENT = 'humoco:drop-status-updated';

export function notifyDropStatusUpdated(detail?: FrontendDropOverviewStatus): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(DROP_STATUS_UPDATED_EVENT, { detail }));
    }
}
