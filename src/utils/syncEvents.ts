// src/utils/syncEvents.ts

import { FrontendMyMarketProfile, FrontendSyncOverviewStatus } from '../types';

export const SYNC_STATUS_UPDATED_EVENT = 'humoco:sync-status-updated';
export const MY_PROFILE_UPDATED_EVENT = 'humoco:my-profile-updated';
export const OPEN_PROFILE_EDITOR_EVENT = 'humoco:open-profile-editor';
export const APP_NOTICES_UPDATED_EVENT = 'humoco:app-notices-updated';
export const TRANSIENT_SYNC_TOAST_EVENT = 'humoco:transient-sync-toast';

export interface TransientSyncToastDetail {
    message: string;
    variant?: 'info' | 'success' | 'error';
}

export function notifySyncStatusUpdated(detail?: FrontendSyncOverviewStatus): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(SYNC_STATUS_UPDATED_EVENT, { detail }));
    }
}

export function notifyMyProfileUpdated(detail?: FrontendMyMarketProfile): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(MY_PROFILE_UPDATED_EVENT, { detail }));
    }
}

export function openOwnProfileEditor(): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(OPEN_PROFILE_EDITOR_EVENT));
    }
}

export function notifyAppNoticesUpdated(): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(APP_NOTICES_UPDATED_EVENT));
    }
}

export function showTransientSyncToast(detail: TransientSyncToastDetail): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(TRANSIENT_SYNC_TOAST_EVENT, { detail }));
    }
}
