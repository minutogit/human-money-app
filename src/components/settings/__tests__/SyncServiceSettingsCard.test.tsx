/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SyncServiceSettingsCard, normalizeSyncUrlInput } from '../SyncServiceSettingsCard';
import { marketService } from '../../../services/marketService';
import { settingsService } from '../../../services/settingsService';
import { save } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { copyToClipboard } from '../../../utils/clipboard';
import { AppConfigDto, FrontendSyncAdminStatus } from '../../../types';

vi.mock('react-i18next', () => {
    const fs = require('fs');
    const path = require('path');
    const enPath = path.resolve(process.cwd(), 'src/locales/en.json');
    let enTranslations = {};
    try {
        enTranslations = JSON.parse(fs.readFileSync(enPath, 'utf-8'));
    } catch (err) {
        console.error(err);
    }

    const stableT = (key: string, options?: any) => {
        let val = (enTranslations as Record<string, string>)[key] || (typeof options === 'string' ? options : key);
        if (options && typeof options === 'object') {
            if (options.defaultValue) {
                val = (enTranslations as Record<string, string>)[key] || options.defaultValue;
            }
            for (const [k, v] of Object.entries(options)) {
                val = val.replace(new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, 'g'), String(v));
            }
        }
        return val;
    };

    return {
        useTranslation: () => ({
            t: stableT,
            i18n: { language: 'en', changeLanguage: vi.fn() },
            ready: true,
        }),
    };
});

vi.mock('@tauri-apps/plugin-dialog', () => ({
    save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
    openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

vi.mock('../../../services/marketService', () => ({
    marketService: {
        discoverAvailableSyncs: vi.fn(),
        getSyncAdminStatus: vi.fn(),
        probePublicSync: vi.fn(),
        savePublicSync: vi.fn(),
        removePublicSync: vi.fn(),
        prepareSyncSetup: vi.fn(),
        getRustSyncConfig: vi.fn(),
        saveSyncSetupZip: vi.fn(),
        testSyncConnection: vi.fn(),
        finalizeSyncSetup: vi.fn(),
        updateSyncAdminConfig: vi.fn(),
        deleteSyncAdmin: vi.fn(),
        getSyncOverviewStatus: vi.fn(),
        probeContactSyncs: vi.fn().mockResolvedValue([]),
    },
}));

vi.mock('../../../services/settingsService', () => ({
    settingsService: {
        getConfig: vi.fn(),
        patchUiPreferences: vi.fn(),
    },
}));

const mockProtectAction = vi.fn(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: mockProtectAction,
    }),
}));

describe('SyncServiceSettingsCard Component', () => {
    const createDefaultConfig = (): AppConfigDto => ({
        uiPreferences: {
            uiMode: 'expert',
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
            sessionTimeoutSeconds: 300,
            privacyDefault: 'public',
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
    });

    beforeEach(() => {
        vi.clearAllMocks();
        const cfg = createDefaultConfig();
        vi.mocked(marketService.discoverAvailableSyncs).mockResolvedValue([]);
        vi.mocked(marketService.getSyncAdminStatus).mockResolvedValue(null);
        vi.mocked(settingsService.getConfig).mockResolvedValue(cfg);
        vi.mocked(settingsService.patchUiPreferences).mockImplementation(async (patch) => ({
            ...cfg.uiPreferences,
            ...patch,
        }));
    });

    it('renders card header and default connected tab with channel toggles', async () => {
        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        expect(screen.getByText(/HuMoCo Sync-Dienst|HuMoCo Sync Service/i)).toBeInTheDocument();
        expect(screen.getByText(/Verbundene Dienste|Connected Services/i)).toBeInTheDocument();
        expect(screen.getByText(/Eigenen Dienst bereitstellen|Host Own Service/i)).toBeInTheDocument();

        // Check channel labels (Allow Backups duplicate removed in APP-UX-09 P6)
        expect(screen.getByText(/Postfach aktivieren|mailbox/i)).toBeInTheDocument();
        expect(screen.getByText(/Marktplatz.*synchronisieren|marketplace/i)).toBeInTheDocument();
        expect(screen.getAllByText(/Vertrauens(liste|einstufungen).*teilen|web of trust|trust ratings/i).length).toBeGreaterThan(0);
        expect(screen.queryByText(/Backups erlauben|Cloud-Sicherung nutzen|Allow Backups|Use Cloud Backup/i)).not.toBeInTheDocument();
    });

    it('toggles channel switch and calls settingsService.patchUiPreferences', async () => {
        const onUpdated = vi.fn();
        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={onUpdated}
            />
        );

        const mailboxToggle = screen.getByLabelText(/Postfach aktivieren|mailbox/i);
        expect(mailboxToggle).toBeChecked();

        fireEvent.click(mailboxToggle);

        await waitFor(() => {
            expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
                { postAcceptFromContacts: false },
                'mock-wallet-pw'
            );
            expect(onUpdated).toHaveBeenCalled();
        });
    });

    it('probes a public sync drop URL and adds it to publicSyncs', async () => {
        vi.mocked(marketService.probePublicSync).mockResolvedValue({
            writeUrl: 'https://test-drop.org/sync.php',
            readUrl: 'https://test-drop.org/sync.php',
            dropPub: 'test-pubkey',
            isAuthorized: true,
            authorizedChannels: ['mailbox', 'market', 'backup'],
        } as any);
        vi.mocked(marketService.savePublicSync).mockResolvedValue(undefined as any);

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const input = screen.getByPlaceholderText(/https:\/\//i);
        fireEvent.change(input, { target: { value: 'https://test-drop.org/sync.php' } });

        const probeBtn = screen.getByRole('button', { name: /Verbinden & Prüfen|Connect & Probe/i });
        fireEvent.click(probeBtn);

        await waitFor(() => {
            expect(marketService.probePublicSync).toHaveBeenCalledWith('https://test-drop.org/sync.php', 'mock-wallet-pw');
            expect(marketService.savePublicSync).toHaveBeenCalledWith(
                expect.objectContaining({
                    writeUrl: 'https://test-drop.org/sync.php',
                    enabled: true,
                }),
                'mock-wallet-pw'
            );
        });
    });

    it('switches to host tab and triggers setup ZIP generation', async () => {
        vi.mocked(save).mockResolvedValue('/tmp/humoco-sync-setup.zip');
        vi.mocked(marketService.prepareSyncSetup).mockResolvedValue(undefined as any);
        vi.mocked(marketService.saveSyncSetupZip).mockResolvedValue(undefined as any);

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        await waitFor(() => {
            expect(screen.getByRole('button', { name: /humoco-sync-setup\.zip herunterladen|Download humoco-sync-setup\.zip/i })).toBeInTheDocument();
        });

        expect(screen.getByLabelText(/Gesamtspeicher \(MB\)|Total Storage \(MB\)/i)).toHaveValue(2048);
        expect(screen.getByLabelText(/Max\. Mitglieder|Max Members/i)).toHaveValue(300);
        expect(screen.getByLabelText(/Limit pro Nutzer \(MB\)|Limit per User \(MB\)/i)).toHaveValue(25);

        const downloadBtn = screen.getByRole('button', { name: /humoco-sync-setup\.zip herunterladen|Download humoco-sync-setup\.zip/i });
        fireEvent.click(downloadBtn);

        await waitFor(() => {
            expect(save).toHaveBeenCalled();
            expect(marketService.prepareSyncSetup).toHaveBeenCalledWith(
                expect.objectContaining({
                    maxMembers: 300,
                    maxTotalStorageMb: 2048,
                    maxUserStorageMb: 25,
                }),
                'mock-wallet-pw'
            );
            expect(marketService.saveSyncSetupZip).toHaveBeenCalledWith('/tmp/humoco-sync-setup.zip', 'mock-wallet-pw');
        });
    });

    it('renders active admin dashboard when admin status is returned', async () => {
        const mockAdmin: FrontendSyncAdminStatus = {
            descriptor: {
                readUrl: 'https://my-space.org/sync.php',
                writeUrl: 'https://my-space.org/sync.php',
                dropPub: 'ed25519-public-key',
            },
            adminPubkey: 'admin-pubkey',
            dataDir: 'data',
            maxMembers: 300,
            maxTotalStorageMb: 50,
            maxUserStorageMb: 25,
            usedBytes: 10 * 1024 * 1024,
            softWarning: false,
            cardPolicy: {
                directMin: 2,
                hop2Enabled: true,
                bridgeMin: 2,
                hop2Min: 2,
            },
            allow: [],
            deny: [],
            blockedPubkeys: [],
            directMemberCount: 10,
            hop2MemberCount: 5,
            totalMemberCount: 15,
            activeSlotIds: [],
        };

        vi.mocked(marketService.getSyncAdminStatus).mockResolvedValue(mockAdmin);

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        await waitFor(() => {
            expect(screen.getByText(/Eigener Sync-Dienst ist aktiv|Own Sync Service Active/i)).toBeInTheDocument();
            expect(screen.getByText(/15 \/ 300/)).toBeInTheDocument();
            expect(screen.getByText(/10 \/ 50 MB/)).toBeInTheDocument();
            expect(screen.getByText(/Zugriffsregeln.*aktualisieren|Update Access Rules on Server/i)).toBeInTheDocument();
            expect(screen.getByText(/Dienst löschen|Delete/i)).toBeInTheDocument();
        });
    });

    it('allows editing policy options and updates them on the active server', async () => {
        const mockAdmin: FrontendSyncAdminStatus = {
            descriptor: {
                readUrl: 'https://my-space.org/sync.php',
                writeUrl: 'https://my-space.org/sync.php',
                dropPub: 'ed25519-public-key',
            },
            adminPubkey: 'admin-pubkey',
            dataDir: 'data',
            maxMembers: 300,
            maxTotalStorageMb: 50,
            maxUserStorageMb: 25,
            usedBytes: 10 * 1024 * 1024,
            softWarning: false,
            cardPolicy: {
                directMin: 2,
                hop2Enabled: true,
                bridgeMin: 2,
                hop2Min: 2,
            },
            allow: [],
            deny: [],
            blockedPubkeys: [],
            directMemberCount: 10,
            hop2MemberCount: 5,
            totalMemberCount: 15,
            activeSlotIds: [],
        };

        vi.mocked(marketService.getSyncAdminStatus).mockResolvedValue(mockAdmin);
        vi.mocked(marketService.updateSyncAdminConfig).mockResolvedValue({
            ...mockAdmin,
            cardPolicy: {
                directMin: 3,
                hop2Enabled: true,
                bridgeMin: 3,
                hop2Min: 1,
            },
        });

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        await waitFor(() => {
            expect(screen.getByText(/Zugriffsregeln|Access Rules/i)).toBeInTheDocument();
        });

        // Select level 3 and update storage/member quotas
        const directMinSelect = screen.getByRole('combobox', { name: /Welche deiner Kontakte|Which contacts/i });
        fireEvent.change(directMinSelect, { target: { value: '3' } });
        expect(directMinSelect).toHaveValue('3');

        const totalStorageInput = screen.getByLabelText(/Gesamtspeicher \(MB\)|Total Storage \(MB\)/i);
        fireEvent.change(totalStorageInput, { target: { value: '4096' } });
        const maxMembersInput = screen.getByLabelText(/Max\. Mitglieder|Max Members/i);
        fireEvent.change(maxMembersInput, { target: { value: '500' } });
        const maxUserInput = screen.getByLabelText(/Limit pro Nutzer \(MB\)|Limit per User \(MB\)/i);
        fireEvent.change(maxUserInput, { target: { value: '30' } });

        // Click update rules
        const updateBtn = screen.getByRole('button', { name: /Zugriffsregeln.*aktualisieren|Update Access Rules on Server/i });
        fireEvent.click(updateBtn);

        await waitFor(() => {
            expect(marketService.updateSyncAdminConfig).toHaveBeenCalledWith(
                expect.objectContaining({
                    cardPolicy: expect.objectContaining({
                        directMin: 3,
                    }),
                    maxMembers: 500,
                    maxTotalStorageMb: 4096,
                    maxUserStorageMb: 30,
                }),
                true,
                'mock-wallet-pw'
            );
        });
    });

    it('intelligently auto-completes https and sync.php in host setup and does not remove url on failure', async () => {
        vi.mocked(marketService.testSyncConnection).mockResolvedValueOnce({
            success: false,
            message: 'Server unreachable',
            writeOk: false,
            readOk: false,
        });

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        // Find the host url input
        const input = screen.getByPlaceholderText(/your-domain\.com\/sync|dein-webspace\.de\/sync/i);
        expect(input).toBeInTheDocument();

        // Enter domain without https and without sync.php
        fireEvent.change(input, { target: { value: 'my-custom-host.org/sync' } });

        // Check that detected sync url preview is displayed
        await waitFor(() => {
            expect(screen.getByText('https://my-custom-host.org/sync/sync.php')).toBeInTheDocument();
        });

        // Click test connection & activate
        const testBtn = screen.getByRole('button', { name: /Test Connection & Activate|Verbindung testen & aktivieren/i });
        fireEvent.click(testBtn);

        await waitFor(() => {
            expect(marketService.testSyncConnection).toHaveBeenCalledWith('https://my-custom-host.org/sync/sync.php', 'mock-wallet-pw');
        });

        // Verify the input still contains the entered/normalized URL (NOT cleared/removed!)
        expect(input).toHaveValue('https://my-custom-host.org/sync/sync.php');
        expect(screen.getByText(/Server unreachable/i)).toBeInTheDocument();
    });

    it('normalizes various input formats with normalizeSyncUrlInput helper', () => {
        expect(normalizeSyncUrlInput('')).toBe('');
        expect(normalizeSyncUrlInput('   ')).toBe('');
        expect(normalizeSyncUrlInput('dein-webspace.de')).toBe('https://dein-webspace.de/sync.php');
        expect(normalizeSyncUrlInput('dein-webspace.de/')).toBe('https://dein-webspace.de/sync.php');
        expect(normalizeSyncUrlInput('dein-webspace.de/sync')).toBe('https://dein-webspace.de/sync/sync.php');
        expect(normalizeSyncUrlInput('dein-webspace.de/sync/')).toBe('https://dein-webspace.de/sync/sync.php');
        expect(normalizeSyncUrlInput('https://dein-webspace.de/sync/sync.php')).toBe('https://dein-webspace.de/sync/sync.php');
        expect(normalizeSyncUrlInput('https://dein-webspace.de/sync/sync.php/')).toBe('https://dein-webspace.de/sync/sync.php');
        expect(normalizeSyncUrlInput('http://mein-server.de/sync')).toBe('https://mein-server.de/sync/sync.php');
        expect(normalizeSyncUrlInput('http://localhost:8080/sync')).toBe('http://localhost:8080/sync/sync.php');
        expect(normalizeSyncUrlInput('http://127.0.0.1:9000')).toBe('http://127.0.0.1:9000/sync.php');
        expect(normalizeSyncUrlInput('https://example.com:443/sync')).toBe('https://example.com/sync/sync.php');
        expect(normalizeSyncUrlInput('Hier mein Sync: https://sync.example.org/sync.php!')).toBe('https://sync.example.org/sync.php');
        expect(normalizeSyncUrlInput('Sync-Link: https://example.org/test?foo=1)')).toBe('https://example.org/test/sync.php');
    });

    it('renders discovered drops with non-contradictory titles and status badges', async () => {
        vi.mocked(marketService.discoverAvailableSyncs).mockResolvedValue([
            {
                descriptor: {
                    readUrl: 'https://friend1.org/data',
                    writeUrl: 'https://friend1.org/sync.php',
                    dropPub: 'pk1',
                    status: 'pending_auth',
                    failCount: 0,
                },
                isAuthorized: false,
                isSelected: false,
                sourceLabel: 'Alice',
                status: 'pending_auth',
            },
            {
                descriptor: {
                    readUrl: 'https://friend2.org/data',
                    writeUrl: 'https://friend2.org/sync.php',
                    dropPub: 'pk2',
                    status: 'unreachable',
                    failCount: 1,
                },
                isAuthorized: false,
                isSelected: false,
                sourceLabel: 'Bob',
                status: 'unreachable',
            },
            {
                descriptor: {
                    readUrl: 'https://friend3.org/data',
                    writeUrl: 'https://friend3.org/sync.php',
                    dropPub: 'pk3',
                    status: 'active',
                    failCount: 0,
                },
                isAuthorized: true,
                isSelected: true,
                sourceLabel: 'Carol',
                status: 'active',
            },
        ]);

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        await waitFor(() => {
            // Check that the title does not contradict offline/pending status
            expect(screen.getByText('Sync service of Alice')).toBeInTheDocument();
            expect(screen.getByText('Sync service of Bob')).toBeInTheDocument();
            expect(screen.getByText('Sync service of Carol')).toBeInTheDocument();

            // Check badges
            expect(screen.getByText('Reachable, not authorized')).toBeInTheDocument();
            expect(screen.getByText('Unreachable (offline)')).toBeInTheDocument();
            expect(screen.getAllByText('Connected').length).toBeGreaterThan(0);
        });
    });

    it('opens registration URL with key and re-checks authorization for public syncs', async () => {
        const publicConfig = createDefaultConfig();
        publicConfig.securityPolicy.publicSyncs = [
            {
                writeUrl: 'https://community.example.org/sync.php',
                readUrl: 'https://community.example.org/data',
                dropPub: 'a'.repeat(64),
                registerUrl: 'https://community.example.org/register',
                accessPubkey: 'b'.repeat(64),
                enabled: false,
                authorizedChannels: [],
                status: 'pending_auth',
            },
        ];
        vi.mocked(settingsService.getConfig).mockResolvedValue(publicConfig);
        vi.mocked(marketService.probePublicSync).mockResolvedValue({
            writeUrl: 'https://community.example.org/sync.php',
            readUrl: 'https://community.example.org/data',
            dropPub: 'a'.repeat(64),
            registerUrl: 'https://community.example.org/register',
            accessPubkey: 'b'.repeat(64),
            authorizedChannels: ['card', 'wot', 'backup', 'post'],
        });
        vi.mocked(marketService.savePublicSync).mockResolvedValue({
            writeUrl: 'https://community.example.org/sync.php',
            readUrl: 'https://community.example.org/data',
            dropPub: 'a'.repeat(64),
            registerUrl: 'https://community.example.org/register',
            accessPubkey: 'b'.repeat(64),
            authorizedChannels: ['card', 'wot', 'backup', 'post'],
        });

        render(
            <SyncServiceSettingsCard
                config={publicConfig}
                onConfigUpdated={vi.fn()}
            />
        );

        const openRegBtn = await screen.findByRole('button', { name: /Unlock on website/i });
        fireEvent.click(openRegBtn);
        await waitFor(() => {
            expect(openUrl).toHaveBeenCalledWith(
                `https://community.example.org/register?key=${'b'.repeat(64)}`
            );
        });

        const copyKeyBtn = screen.getByRole('button', { name: /Copy key/i });
        fireEvent.click(copyKeyBtn);
        await waitFor(() => {
            expect(copyToClipboard).toHaveBeenCalledWith('b'.repeat(64));
        });

        const checkAuthBtn = screen.getByRole('button', { name: /Check activation/i });
        fireEvent.click(checkAuthBtn);
        await waitFor(() => {
            expect(marketService.probePublicSync).toHaveBeenCalledWith('https://community.example.org/sync.php', 'mock-wallet-pw');
            expect(marketService.savePublicSync).toHaveBeenCalledWith(
                expect.objectContaining({
                    writeUrl: 'https://community.example.org/sync.php',
                    enabled: true,
                }),
                'mock-wallet-pw'
            );
        });
    });

    it('opens delete confirmation dialog for public syncs and respects deleteRemoteSlots checkbox', async () => {
        const publicConfig = createDefaultConfig();
        publicConfig.securityPolicy.publicSyncs = [
            {
                writeUrl: 'https://community.example.org/sync.php',
                readUrl: 'https://community.example.org/data',
                dropPub: 'a'.repeat(64),
                accessPubkey: 'b'.repeat(64),
                enabled: true,
                authorizedChannels: ['card', 'post'],
                status: 'active',
            },
        ];
        let currentConfig: AppConfigDto = publicConfig;
        vi.mocked(settingsService.getConfig).mockImplementation(async () => currentConfig);
        vi.mocked(marketService.removePublicSync).mockImplementation(async () => {
            currentConfig = {
                ...publicConfig,
                securityPolicy: {
                    ...publicConfig.securityPolicy,
                    publicSyncs: [],
                },
            };
        });

        render(
            <SyncServiceSettingsCard
                config={publicConfig}
                onConfigUpdated={vi.fn()}
            />
        );

        const deleteTrigger = await screen.findByRole('button', { name: 'Delete' });
        fireEvent.click(deleteTrigger);

        const dialog = await screen.findByRole('dialog');
        const remoteCheckbox = within(dialog).getByRole('checkbox', {
            name: /Also delete all my stored data/i,
        });
        expect(remoteCheckbox).toBeChecked();

        // Uncheck and verify false is passed
        fireEvent.click(remoteCheckbox);
        expect(remoteCheckbox).not.toBeChecked();

        const confirmBtn = within(dialog).getByRole('button', { name: /^Delete$/i });
        fireEvent.click(confirmBtn);

        await waitFor(() => {
            expect(marketService.removePublicSync).toHaveBeenCalledWith(
                'https://community.example.org/sync.php',
                false,
                'mock-wallet-pw'
            );
        });
    });

    it('copies Rust config.toml from the Host setup wizard', async () => {
        const sampleToml = '[server]\nadmin_pubkey = "abcd"\n';
        vi.mocked(marketService.prepareSyncSetup).mockResolvedValue({
            descriptor: {
                writeUrl: '',
                readUrl: '',
                dropPub: 'abcd',
            },
            dataDir: 'd-1234',
            adminPubkey: 'abcd',
            maxMembers: 300,
            maxTotalStorageMb: 2048,
            maxUserStorageMb: 25,
            policy: {
                directMin: 2,
                hop2Enabled: true,
                bridgeMin: 2,
                hop2Min: 2,
            },
            zipBase64: '',
            rustConfigToml: sampleToml,
        });

        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        const copyTomlBtn = await screen.findByRole('button', { name: /Copy Rust config\.toml/i });
        fireEvent.click(copyTomlBtn);

        await waitFor(() => {
            expect(marketService.prepareSyncSetup).toHaveBeenCalled();
            expect(copyToClipboard).toHaveBeenCalledWith(sampleToml);
        });
    });

    it('renders concise PHP ZIP and Rust config.toml setup instructions in host wizard', async () => {
        render(
            <SyncServiceSettingsCard
                config={createDefaultConfig()}
                onConfigUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        expect(await screen.findByText(/Next steps before entering the URL below:/i)).toBeInTheDocument();
        expect(screen.getByText(/PHP webspace \(humoco-sync-setup\.zip\)/i)).toBeInTheDocument();
        expect(screen.getByText(/Rust server \(config\.toml\)/i)).toBeInTheDocument();
    });
});
