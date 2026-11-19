// src/components/settings/sync/syncHelpers.ts
import { useState, useCallback } from 'react';
import type { TFunction } from 'i18next';
import {
    FrontendDiscoveredSync,
    FrontendSyncReplacementTarget,
    StoredPublicSync,
    UiPreferences,
} from '../../../types';
import { normalizeSyncUrlInput } from '../../../utils/humocoUri';

export const COMMUNITY_SYNC_PRESET_URL = 'https://sync.menschlich-miteinander.org/sync.php';
export const HOST_SYNC_DRAFT_KEY = 'humoco_draft_host_webspace_url';
export const CONSUMER_SYNC_DRAFT_KEY = 'humoco_draft_consumer_sync_url';

export interface SyncFeedback {
    type: 'success' | 'error' | 'info';
    message: string;
}

export interface SyncProbeResult {
    status: 'success' | 'unauthorized' | 'error';
    message: string;
}

export interface ChannelSettingsState {
    postAcceptFromContacts: boolean;
    marketSyncEnabled: boolean;
    wotShareViaSync: boolean;
    shareMinLevel: number;
    marketPollWifiOnly: boolean;
}

/**
 * Derives channel toggle state from persisted UI preferences with defaults.
 */
export function buildChannelSettings(
    uiPreferences: UiPreferences | null | undefined,
): ChannelSettingsState {
    return {
        postAcceptFromContacts: uiPreferences?.postAcceptFromContacts ?? true,
        marketSyncEnabled: uiPreferences?.marketSyncEnabled ?? true,
        wotShareViaSync: uiPreferences?.wotShareViaSync ?? true,
        shareMinLevel: uiPreferences?.shareMinLevel ?? 1,
        marketPollWifiOnly: uiPreferences?.marketPollWifiOnly ?? false,
    };
}

export interface HostQuotasState {
    maxMembers: number;
    maxTotalStorageMb: number;
    maxUserStorageMb: number;
}

/**
 * Clamps hosted sync quotas to valid lower and upper bounds.
 */
export function normalizeHostQuotas(quotas: HostQuotasState): HostQuotasState {
    const maxUserStorageMb = Math.max(10, quotas.maxUserStorageMb);
    const maxTotalStorageMb = Math.max(maxUserStorageMb, quotas.maxTotalStorageMb);
    const maxMembers = Math.max(1, Math.min(100000, quotas.maxMembers));
    return { maxMembers, maxTotalStorageMb, maxUserStorageMb };
}

export interface RevokedDropNotice {
    url: string;
    status: string;
    sunsetAt?: number | null;
    reason?: string | null;
    replacement?: FrontendSyncReplacementTarget | null;
}

/**
 * Validates that an external sync or registration URL uses HTTPS, or HTTP on loopback.
 */
export function isSafeExternalSyncUrl(raw?: string | null): raw is string {
    if (!raw) return false;
    try {
        const u = new URL(raw.trim());
        if (u.protocol === 'https:') return true;
        if (u.protocol === 'http:') {
            return (
                u.hostname === '127.0.0.1' ||
                u.hostname === 'localhost' ||
                u.hostname === '::1' ||
                u.hostname === '[::1]'
            );
        }
        return false;
    } catch {
        return false;
    }
}

const normalizeEndpointUrl = (u?: string) => (u || '').trim().replace(/\/+$/, '').toLowerCase();

/**
 * Checks whether two sync descriptors share either their write or read endpoint URL.
 */
export const isSameSyncEndpoint = (
    a: { writeUrl?: string; readUrl?: string },
    b: { writeUrl?: string; readUrl?: string }
): boolean => {
    const aWrite = normalizeEndpointUrl(a.writeUrl);
    const bWrite = normalizeEndpointUrl(b.writeUrl);
    const aRead = normalizeEndpointUrl(a.readUrl);
    const bRead = normalizeEndpointUrl(b.readUrl);
    return (
        (Boolean(aWrite) && Boolean(bWrite) && aWrite === bWrite) ||
        (Boolean(aRead) && Boolean(bRead) && aRead === bRead)
    );
};

export function getSessionDraft(key: string): string {
    try {
        if (typeof window !== 'undefined' && window.sessionStorage) {
            return window.sessionStorage.getItem(key) || '';
        }
    } catch {
        // Ignore unavailable sessionStorage
    }
    return '';
}

export function setSessionDraft(key: string, value: string): void {
    try {
        if (typeof window !== 'undefined' && window.sessionStorage) {
            if (value) {
                window.sessionStorage.setItem(key, value);
            } else {
                window.sessionStorage.removeItem(key);
            }
        }
    } catch {
        // Ignore unavailable sessionStorage
    }
}

/**
 * React state backed by sessionStorage so drafts survive tab or view switches.
 */
export function useSessionDraft(key: string): [string, (val: string) => void] {
    const [value, setValueState] = useState(() => getSessionDraft(key));
    const setValue = useCallback(
        (next: string) => {
            setValueState(next);
            setSessionDraft(key, next);
        },
        [key]
    );
    return [value, setValue];
}

export function resolveNormalizedSyncUrl(raw: string, updateDraft: (val: string) => void): string | null {
    const trimmed = raw.trim();
    if (!trimmed) return null;
    const normalized = normalizeSyncUrlInput(trimmed);
    if (normalized && normalized !== raw) {
        updateDraft(normalized);
    }
    return normalized || trimmed;
}

/**
 * Collects deprecating or revoked notices from discovered contact drops and configured public drops.
 */
export function collectRevokedNotices(
    discoveredDrops: FrontendDiscoveredSync[],
    configuredPublicDrops: StoredPublicSync[]
): RevokedDropNotice[] {
    return [
        ...discoveredDrops
            .filter((d) => {
                const st = d.status || d.descriptor.status;
                return st === 'deprecating' || st === 'revoked';
            })
            .map((d) => ({
                url: d.descriptor.writeUrl || d.descriptor.readUrl,
                status: (d.status || d.descriptor.status) as string,
                sunsetAt: d.descriptor.sunsetAt,
                reason: d.descriptor.revocationReason,
                replacement: d.descriptor.replacement,
            })),
        ...configuredPublicDrops
            .filter((p) => p.status === 'deprecating' || p.status === 'revoked')
            .map((p) => ({
                url: p.writeUrl || p.readUrl,
                status: p.status as string,
                sunsetAt: p.sunsetAt,
                reason: p.revocationReason,
                replacement: p.replacement,
            })),
    ];
}

/**
 * Resolves badge classes, label, and tooltip for a discovered contact sync drop.
 */
export function getDiscoveredDropBadge(drop: FrontendDiscoveredSync, t: TFunction) {
    const status = drop.status || drop.descriptor.status || 'unverified';
    let badgeColor = 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300';
    let statusText = t('settings.syncStatusUnverified', 'Noch nicht geprüft');
    let statusTooltip: string | undefined;

    if (status === 'active') {
        if (drop.isSelected) {
            badgeColor = 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-300';
            statusText = t('settings.syncStatusConnected', 'Verbunden');
        } else {
            badgeColor = 'bg-emerald-50 text-emerald-700 border border-emerald-200 dark:bg-emerald-950/20 dark:text-emerald-300 dark:border-emerald-800';
            statusText = t('settings.syncStatusActiveInactive', 'Bereit (nicht aktiviert)');
        }
    } else if (status === 'pending_auth') {
        badgeColor = 'bg-amber-100 text-amber-800 border border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800';
        statusText = t('settings.syncStatusPendingAuthBadge', 'Erreichbar, nicht freigeschaltet');
        statusTooltip = t('settings.syncStatusPendingAuthDesc', 'Sync-Dienst erreichbar, aber dein Schlüssel ist beim Betreiber noch nicht freigeschaltet.');
    } else if (status === 'unreachable') {
        badgeColor = 'bg-rose-100 text-rose-800 border border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800';
        statusText = t('settings.syncStatusOffline', 'Nicht erreichbar (Offline)');
    } else if (status === 'deprecating') {
        badgeColor = 'bg-amber-100 text-amber-800 border border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800';
        const dateStr = drop.descriptor.sunsetAt
            ? new Date(drop.descriptor.sunsetAt * 1000).toLocaleDateString()
            : '';
        statusText = dateStr
            ? t('settings.syncStatusDeprecatingDate', { date: dateStr, defaultValue: `Schließt am ${dateStr}` })
            : t('settings.syncStatusDeprecating', 'Wird bald geschlossen');
        statusTooltip = drop.descriptor.revocationReason || undefined;
    } else if (status === 'revoked') {
        badgeColor = 'bg-rose-100 text-rose-800 border border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800';
        statusText = t('settings.syncStatusRevoked', 'Geschlossen');
        statusTooltip = drop.descriptor.revocationReason || undefined;
    }

    return { status, badgeColor, statusText, statusTooltip };
}
