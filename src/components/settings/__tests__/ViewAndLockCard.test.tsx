import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SessionTimeoutControl, ViewAndLockCard } from '../ViewAndLockCard';
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

        // Empty or negative values keep Save disabled and block Enter submit
        fireEvent.change(input, { target: { value: '' } });
        expect(saveBtn).toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });
        fireEvent.change(input, { target: { value: '-5' } });
        expect(saveBtn).toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(onPatchSecurityPolicy).toHaveBeenCalledTimes(1);

        // Value 0 ("Always ask") is valid and can be submitted via Enter
        fireEvent.change(input, { target: { value: '0' } });
        expect(saveBtn).not.toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });

        await waitFor(() => {
            expect(onPatchSecurityPolicy).toHaveBeenCalledWith({ sessionTimeoutSeconds: 0 });
        });
    });

    it('floors decimal minutes to whole minutes and keeps Save disabled when floored value equals current timeout', async () => {
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

        fireEvent.change(input, { target: { value: '10.9' } });
        expect(saveBtn).toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(onPatchSecurityPolicy).not.toHaveBeenCalled();

        fireEvent.change(input, { target: { value: '2.9' } });
        expect(saveBtn).not.toBeDisabled();
        fireEvent.click(saveBtn);

        await waitFor(() => {
            expect(onPatchSecurityPolicy).toHaveBeenCalledWith({ sessionTimeoutSeconds: 120 });
        });
    });

    it('syncs input value on external config update and defaults to 10 minutes when config is null', () => {
        const onPatchSecurityPolicy = vi.fn().mockResolvedValue(undefined);
        const { rerender } = render(
            <UiModeProvider>
                <ViewAndLockCard
                    config={null}
                    onPatchSecurityPolicy={onPatchSecurityPolicy}
                />
            </UiModeProvider>
        );

        const input = screen.getByRole('spinbutton') as HTMLInputElement;
        const saveBtn = screen.getByTestId('save-timeout-button');
        expect(input.value).toBe('10');
        expect(saveBtn).toBeDisabled();

        rerender(
            <UiModeProvider>
                <ViewAndLockCard
                    config={makeConfig(1800)}
                    onPatchSecurityPolicy={onPatchSecurityPolicy}
                />
            </UiModeProvider>
        );

        expect(input.value).toBe('30');
        expect(saveBtn).toBeDisabled();
    });

    it('prevents duplicate submissions while save is in flight and re-enables after completion', async () => {
        let resolveSave!: () => void;
        const onPatchSecurityPolicy = vi.fn().mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    resolveSave = resolve;
                })
        );
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

        fireEvent.change(input, { target: { value: '15' } });
        fireEvent.click(saveBtn);

        expect(saveBtn).toBeDisabled();
        expect(input).toBeDisabled();
        fireEvent.click(saveBtn);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(onPatchSecurityPolicy).toHaveBeenCalledTimes(1);

        resolveSave();
        await waitFor(() => {
            expect(input).not.toBeDisabled();
        });
    });

    it('respects disabled prop on SessionTimeoutControl even when pressing Enter', () => {
        const onSave = vi.fn();
        render(
            <SessionTimeoutControl
                sessionTimeoutSeconds={600}
                onSaveTimeout={onSave}
                disabled={true}
            />
        );

        const input = screen.getByRole('spinbutton') as HTMLInputElement;
        const saveBtn = screen.getByTestId('save-timeout-button');
        expect(input).toBeDisabled();
        expect(saveBtn).toBeDisabled();

        fireEvent.change(input, { target: { value: '20' } });
        expect(saveBtn).toBeDisabled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(onSave).not.toHaveBeenCalled();
    });
});
