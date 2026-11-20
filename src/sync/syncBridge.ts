// src/sync/syncBridge.ts
import { invoke } from '@tauri-apps/api/core';
import { backupService } from '../services/backupService';
import { postService } from '../services/postService';
import { FrontendAutoBackupStatusDto, FrontendBackupSyncResult } from '../types';
import { APP_EVENTS, emitAppEvent } from '../utils/appEvents';
import { detectIsWifiConnection } from '../utils/networkStatus';
import { notifySyncStatusUpdated } from '../utils/syncEvents';
import { rateLimitManager } from './rateLimitManager';

export interface SyncWakeScopes {
    seal?: boolean;
    localBackup?: boolean;
    syncBackup?: boolean;
    syncBackupForce?: boolean;
    post?: boolean;
    market?: boolean;
    marketForceUpload?: boolean;
    syncMembers?: boolean;
}

export interface WalletMutationOptions {
    syncMembers?: boolean;
    scheduleMarket?: boolean;
    contactsUpdated?: boolean;
    wotUpdated?: boolean;
}

export function dispatchBackupUpdated(status?: FrontendAutoBackupStatusDto): void {
    emitAppEvent(APP_EVENTS.BACKUP_UPDATED, status);
}

export async function updateNetworkStatus(
    isOnline: boolean,
    isWifi: boolean,
    isVisible: boolean,
): Promise<void> {
    try {
        await invoke<void>('update_network_status', { isOnline, isWifi, isVisible });
    } catch {
        // Ignore in non-Tauri / test environments where runtime invoke may be unmocked
    }
}

export async function wakeSyncCoordinator(scopes: SyncWakeScopes): Promise<void> {
    try {
        await invoke<void>('wake_sync_coordinator', {
            scopes: {
                seal: Boolean(scopes.seal),
                localBackup: Boolean(scopes.localBackup),
                syncBackup: Boolean(scopes.syncBackup),
                syncBackupForce: Boolean(scopes.syncBackupForce),
                post: Boolean(scopes.post),
                market: Boolean(scopes.market),
                marketForceUpload: Boolean(scopes.marketForceUpload),
                syncMembers: Boolean(scopes.syncMembers),
            },
        });
    } catch {
        // Ignore when session is locked or invoke is unmocked in unit tests
    }
}

export function triggerAutoBackup(forceSyncBackup: boolean = true): void {
    void wakeSyncCoordinator({
        localBackup: true,
        syncBackup: true,
        syncBackupForce: forceSyncBackup,
    });
}

/**
 * Triggers standard post-mutation side effects: UI event emission and debounced backend backup +
 * optional member/market coordinator wakes (with zero frontend interval/timer loops).
 */
export function afterWalletMutation(
    _password?: string | null,
    options?: WalletMutationOptions,
): void {
    void wakeSyncCoordinator({
        localBackup: true,
        syncBackup: true,
        syncBackupForce: true,
        ...(options?.scheduleMarket ? { market: true, marketForceUpload: true } : {}),
        ...(options?.syncMembers ? { syncMembers: true } : {}),
    });

    if (options?.contactsUpdated) {
        emitAppEvent(APP_EVENTS.CONTACTS_UPDATED);
    }
    if (options?.wotUpdated) {
        emitAppEvent(APP_EVENTS.WOT_UPDATED);
    }
}

/**
 * Direct one-shot UI invocation for syncing off-site backups immediately (e.g. from Settings).
 */
export async function runSyncBackupPass(
    force: boolean = false,
): Promise<FrontendBackupSyncResult | null> {
    if (!force && rateLimitManager.isCoolingDown()) {
        return null;
    }
    try {
        const result = await backupService.syncBackup(undefined, detectIsWifiConnection());
        dispatchBackupUpdated();
        notifySyncStatusUpdated();
        if (result) {
            const retrySecs = rateLimitManager.extractRetryAfter(result);
            if (retrySecs) {
                rateLimitManager.setCooldown(retrySecs);
            }
        }
        return result;
    } catch (e) {
        const retrySecs = rateLimitManager.extractRetryAfter(e);
        if (retrySecs) {
            rateLimitManager.setCooldown(retrySecs);
        }
        return null;
    }
}

/**
 * Direct one-shot UI invocation for flushing PoST immediately (e.g. right after transfer send).
 */
export async function runPostSyncPass(): Promise<void> {
    if (rateLimitManager.isCoolingDown()) {
        return;
    }
    try {
        const res = await postService.syncPostChannels(detectIsWifiConnection());
        if (res) {
            const retrySecs = rateLimitManager.extractRetryAfter(res);
            if (retrySecs) {
                rateLimitManager.setCooldown(retrySecs);
            }
        }
    } catch (err) {
        const retrySecs = rateLimitManager.extractRetryAfter(err);
        if (retrySecs) {
            rateLimitManager.setCooldown(retrySecs);
        }
    }
}
