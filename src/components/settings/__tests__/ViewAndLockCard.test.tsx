import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ViewAndLockCard } from '../ViewAndLockCard';
import { UiModeProvider } from '../../../context/UiModeContext';
import type { AppConfigDto } from '../../../types';

function makeConfig(sessionTimeoutSeconds: number): AppConfigDto {
    return {
        uiPreferences: {
            uiMode: 'simple',
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
            sessionTimeoutSeconds,
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
            l2ProbeCache: {},
        },
    };
}

describe('ViewAndLockCard', () => {
    it('does not save on typing and commits timeout on Save button click or Enter', async () => {
        const onPatchSecurityPolicy = vi.fn().mockResolvedValue(undefined);
        render(
            <UiModeProvider>
                <ViewAndLockCard
                    config={makeConfig(600)}
                    onPatchSecurityPolicy={onPatchSecurityPolicy}
                />
            </UiModeProvider>
        );

        const input = screen.getByRole('spinbutton') as HTMLInputElement;
        const saveBtn = screen.getByTestId('save-timeout-button');

        expect(input.value).toBe('10');
        expect(saveBtn).toBeDisabled();

        // Typing changes draft without saving
        fireEvent.change(input, { target: { value: '15' } });
        expect(onPatchSecurityPolicy).not.toHaveBeenCalled();
        expect(saveBtn).not.toBeDisabled();

        fireEvent.click(saveBtn);
        await waitFor(() => {
            expect(onPatchSecurityPolicy).toHaveBeenCalledWith({ sessionTimeoutSeconds: 900 });
        });

        // Empty or negative values keep Save disabled
        fireEvent.change(input, { target: { value: '' } });
        expect(saveBtn).toBeDisabled();
        fireEvent.change(input, { target: { value: '-5' } });
        expect(saveBtn).toBeDisabled();

        // Value 0 ("Always ask") is valid and can be submitted via Enter
        fireEvent.change(input, { target: { value: '0' } });
        expect(saveBtn).not.toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });

        await waitFor(() => {
            expect(onPatchSecurityPolicy).toHaveBeenCalledWith({ sessionTimeoutSeconds: 0 });
        });
    });
});
