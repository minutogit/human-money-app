// src/sync/__tests__/marketSync.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
    startMarketSyncLoop,
    stopMarketSyncLoop,
    runMarketSyncPass,
    DEFAULT_MARKET_SYNC_INTERVAL_MS,
} from '../marketSync';
import { contactService } from '../../services/contactService';
import { channelMembersSyncManager } from '../channelMembersSyncManager';
import { rateLimitManager } from '../rateLimitManager';
import { stopAutoBackupLoop } from '../autoBackup';
import { APP_EVENTS } from '../../utils/appEvents';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

vi.mock('../../services/backupService', () => ({
    backupService: {
        runAutoBackupSync: vi.fn().mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        }),
        syncBackup: vi.fn().mockResolvedValue({
            uploadedTo: 0,
            authorizedTargets: 0,
            errors: [],
            paddedBytes: 0,
        }),
    },
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('marketSync & first-contact onboarding debounce', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        stopMarketSyncLoop();
        stopAutoBackupLoop();
        channelMembersSyncManager.reset();
        rateLimitManager.clearCooldown();
        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
            if (cmd === 'sync_channel_members') return 1;
            if (cmd === 'sync_market_channels') {
                return {
                    skippedWifiOnly: false,
                    uploadedTo: 1,
                    polledSyncs: 1,
                    decryptedSlots: 0,
                    importedCards: 0,
                    importedWotLists: 0,
                    errors: [],
                    revokedOwnSyncs: [],
                };
            }
            return undefined;
        });
    });

    afterEach(() => {
        stopMarketSyncLoop();
        stopAutoBackupLoop();
        channelMembersSyncManager.reset();
        rateLimitManager.clearCooldown();
        vi.useRealTimers();
    });

    it('startMarketSyncLoop runs background pass with respectCooldown every 20 minutes', async () => {
        startMarketSyncLoop(DEFAULT_MARKET_SYNC_INTERVAL_MS);

        await Promise.resolve();
        expect(invoke).toHaveBeenCalledWith('sync_market_channels', { respectCooldown: true });
        expect(invoke).toHaveBeenCalledTimes(1);

        // Before 20 minutes -> no second run
        await vi.advanceTimersByTimeAsync(19 * 60_000);
        expect(invoke).toHaveBeenCalledTimes(1);

        // At 20 minutes -> second background run
        await vi.advanceTimersByTimeAsync(60_000);
        expect(invoke).toHaveBeenCalledTimes(2);
    });

    it('coalesces 5 rapid trustLevel>=1 saveContact calls into 1 members flush + 1 forceUpload after 3s', async () => {
        for (let i = 1; i <= 5; i++) {
            await contactService.saveContact({
                did: `did:key:zContact${i}`,
                profile: { id: `did:key:zContact${i}`, firstName: `Friend${i}` },
                tags: [],
                addedAt: '2026-01-01T00:00:00Z',
                trustLevel: 2,
            });
        }

        // Only save_contact calls immediately; no market sync yet
        const marketCallsBefore = vi
            .mocked(invoke)
            .mock.calls.filter(([cmd]) => cmd === 'sync_market_channels');
        expect(marketCallsBefore).toHaveLength(0);

        // Members timer fires at 250ms, market onboarding timer fires at 3000ms
        await vi.advanceTimersByTimeAsync(3_000);

        const memberCalls = vi
            .mocked(invoke)
            .mock.calls.filter(([cmd]) => cmd === 'sync_channel_members');
        const marketCallsAfter = vi
            .mocked(invoke)
            .mock.calls.filter(([cmd]) => cmd === 'sync_market_channels');

        expect(memberCalls).toHaveLength(1);
        expect(marketCallsAfter).toHaveLength(1);
        expect(marketCallsAfter[0]).toEqual(['sync_market_channels', { forceUpload: true }]);
    });

    it('does not schedule market onboarding publish for trustLevel 0 (Level 0 exclusion)', async () => {
        await contactService.saveContact({
            did: 'did:key:zLevel0',
            profile: { id: 'did:key:zLevel0', firstName: 'Stranger' },
            tags: [],
            addedAt: '2026-01-01T00:00:00Z',
            trustLevel: 0,
        });

        await vi.advanceTimersByTimeAsync(5_000);

        const marketCalls = vi
            .mocked(invoke)
            .mock.calls.filter(([cmd]) => cmd === 'sync_market_channels');
        expect(marketCalls).toHaveLength(0);
    });

    it('skips background pass during 429 cooldown but allows forceUpload onboarding', async () => {
        rateLimitManager.setCooldown(60);
        await runMarketSyncPass();
        expect(invoke).not.toHaveBeenCalled();

        await runMarketSyncPass({ forceUpload: true });
        expect(invoke).toHaveBeenCalledWith('sync_market_channels', { forceUpload: true });
    });

    it('emits CONTACTS_UPDATED and MARKET_CARD_UPDATED when market sync imports cards', async () => {
        vi.mocked(invoke).mockResolvedValueOnce({
            skippedWifiOnly: false,
            uploadedTo: 0,
            polledSyncs: 1,
            decryptedSlots: 1,
            importedCards: 2,
            newAds: 1,
            importedWotLists: 0,
            errors: [],
            revokedOwnSyncs: [],
        });

        const contactsSpy = vi.fn();
        const cardSpy = vi.fn();
        window.addEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
        window.addEventListener(APP_EVENTS.MARKET_CARD_UPDATED, cardSpy);

        await runMarketSyncPass();

        expect(contactsSpy).toHaveBeenCalledTimes(1);
        expect(cardSpy).toHaveBeenCalledTimes(1);

        window.removeEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
        window.removeEventListener(APP_EVENTS.MARKET_CARD_UPDATED, cardSpy);
    });
});
