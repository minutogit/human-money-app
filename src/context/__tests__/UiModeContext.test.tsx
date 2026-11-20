import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { UiModeProvider, useUiMode } from '../UiModeContext';
import { settingsService } from '../../services/settingsService';
import type { AppConfigDto, UiPreferences } from '../../types';

let mockSessionState = {
    isAuthenticated: false,
    hasCachedSession: false,
};

vi.mock('../SessionContext', () => ({
    useOptionalSession: () => mockSessionState,
}));

vi.mock('../../services/settingsService', () => ({
    settingsService: {
        getConfig: vi.fn(),
        patchUiPreferences: vi.fn(),
    },
}));

function makeConfig(uiMode: 'simple' | 'expert'): AppConfigDto {
    return {
        uiPreferences: {
            uiMode,
            lastUsedDirectory: null,
            balanceViewMode: 'spendable',
            marketShowUnknown: true,
            maxStoredCards: 2000,
            marketSendCardOnPayment: false,
            marketSyncEnabled: true,
            shareMinLevel: 1,
            marketPollWifiOnly: false,
            wotShareViaSync: true,
            postAcceptFromContacts: true,
            mediaAutoLoadMaxHops: 2,
        },
        securityPolicy: {
            sessionTimeoutSeconds: 600,
            privacyDefault: 'ask',
            devModeUnlocked: false,
            autoBackupEnabled: true,
            syncBackupEnabled: true,
            backupSyncFolder: null,
            backupProtectionMode: null,
            marketSelectedSyncs: [],
            publicSyncs: [],
        },
        lifecycle: {
            pendingRestoreL2Reconciliation: false,
            backupDirty: false,
            lastBackupAt: null,
            lastSyncBackupAt: null,
        },
    };
}

function ModeHarness() {
    const { mode, isExpert, setMode, toggle } = useUiMode();
    return (
        <div>
            <span data-testid="mode-value">{mode}</span>
            <span data-testid="is-expert-value">{String(isExpert)}</span>
            <button type="button" data-testid="set-expert" onClick={() => setMode('expert')}>
                Set Expert
            </button>
            <button type="button" data-testid="toggle-mode" onClick={() => toggle()}>
                Toggle
            </button>
        </div>
    );
}

describe('UiModeContext', () => {
    let getItemSpy: ReturnType<typeof vi.spyOn<Storage, 'getItem'>>;
    let setItemSpy: ReturnType<typeof vi.spyOn<Storage, 'setItem'>>;
    let removeItemSpy: ReturnType<typeof vi.spyOn<Storage, 'removeItem'>>;

    beforeEach(() => {
        vi.clearAllMocks();
        mockSessionState = { isAuthenticated: false, hasCachedSession: false };
        getItemSpy = vi.spyOn(Storage.prototype, 'getItem');
        setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
        removeItemSpy = vi.spyOn(Storage.prototype, 'removeItem');
    });

    afterEach(() => {
        getItemSpy.mockRestore();
        setItemSpy.mockRestore();
        removeItemSpy.mockRestore();
    });

    it('never reads or writes localStorage, loads uiMode on login, and resets on logout', async () => {
        vi.mocked(settingsService.getConfig).mockResolvedValue(makeConfig('expert'));

        const { rerender } = render(
            <UiModeProvider>
                <ModeHarness />
            </UiModeProvider>
        );

        expect(screen.getByTestId('mode-value')).toHaveTextContent('simple');
        expect(screen.getByTestId('is-expert-value')).toHaveTextContent('false');
        expect(settingsService.getConfig).not.toHaveBeenCalled();

        // Login triggers getConfig() load
        mockSessionState = { isAuthenticated: true, hasCachedSession: true };
        rerender(
            <UiModeProvider>
                <ModeHarness />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('mode-value')).toHaveTextContent('expert');
            expect(screen.getByTestId('is-expert-value')).toHaveTextContent('true');
        });
        expect(settingsService.getConfig).toHaveBeenCalledTimes(1);

        // Logout resets back to initialMode without leaking previous user's preference
        mockSessionState = { isAuthenticated: false, hasCachedSession: false };
        rerender(
            <UiModeProvider>
                <ModeHarness />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('mode-value')).toHaveTextContent('simple');
            expect(screen.getByTestId('is-expert-value')).toHaveTextContent('false');
        });

        expect(getItemSpy).not.toHaveBeenCalled();
        expect(setItemSpy).not.toHaveBeenCalled();
        expect(removeItemSpy).not.toHaveBeenCalled();
    });

    it('persists mode toggle via patchUiPreferences without re-reading getConfig', async () => {
        const simpleCfg = makeConfig('simple');
        vi.mocked(settingsService.getConfig).mockResolvedValue(simpleCfg);
        vi.mocked(settingsService.patchUiPreferences).mockResolvedValue({
            ...simpleCfg.uiPreferences,
            uiMode: 'expert',
        } satisfies UiPreferences);

        mockSessionState = { isAuthenticated: true, hasCachedSession: true };
        render(
            <UiModeProvider>
                <ModeHarness />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(settingsService.getConfig).toHaveBeenCalledTimes(1);
        });

        fireEvent.click(screen.getByTestId('set-expert'));

        await waitFor(() => {
            expect(screen.getByTestId('mode-value')).toHaveTextContent('expert');
            expect(settingsService.patchUiPreferences).toHaveBeenCalledWith({ uiMode: 'expert' });
        });
        expect(settingsService.getConfig).toHaveBeenCalledTimes(1);
        expect(getItemSpy).not.toHaveBeenCalled();
        expect(setItemSpy).not.toHaveBeenCalled();
    });
});
