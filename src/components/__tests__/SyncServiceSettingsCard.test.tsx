/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SyncServiceSettingsCard } from '../settings/SyncServiceSettingsCard';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { save } from '@tauri-apps/plugin-dialog';
import { AppSettings, FrontendSyncAdminStatus } from '../../types';

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

vi.mock('../../services/marketService', () => ({
    marketService: {
        discoverAvailableSyncs: vi.fn(),
        getSyncAdminStatus: vi.fn(),
        probePublicSync: vi.fn(),
        savePublicSync: vi.fn(),
        prepareSyncSetup: vi.fn(),
        saveSyncSetupZip: vi.fn(),
        testSyncConnection: vi.fn(),
        finalizeSyncSetup: vi.fn(),
        updateSyncAdminConfig: vi.fn(),
        rotateSyncAdminKey: vi.fn(),
        deleteSyncAdmin: vi.fn(),
        getSyncOverviewStatus: vi.fn(),
    },
}));

vi.mock('../../services/settingsService', () => ({
    settingsService: {
        getSettings: vi.fn(),
        saveSettings: vi.fn().mockResolvedValue(undefined),
    },
}));

const mockProtectAction = vi.fn(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: mockProtectAction,
    }),
}));

describe('SyncServiceSettingsCard Component', () => {
    const defaultSettings: AppSettings = {
        sessionTimeoutSeconds: 300,
        privacyDefault: 'public',
        activeNetwork: 'mainnet',
        postAcceptFromContacts: true,
        marketSendCardOnPayment: false,
        marketSyncEnabled: true,
        wotShareViaSync: true,
        syncBackupEnabled: true,
        publicSyncs: [],
    };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(marketService.discoverAvailableSyncs).mockResolvedValue([]);
        vi.mocked(marketService.getSyncAdminStatus).mockResolvedValue(null);
        vi.mocked(settingsService.getSettings).mockResolvedValue(defaultSettings);
    });

    it('renders card header and default connected tab with channel toggles', async () => {
        render(
            <SyncServiceSettingsCard
                settings={defaultSettings}
                onSettingsUpdated={vi.fn()}
            />
        );

        expect(screen.getByText(/HuMoCo Sync-Dienst|HuMoCo Sync Service/i)).toBeInTheDocument();
        expect(screen.getByText(/Verbundene Dienste|Connected Services/i)).toBeInTheDocument();
        expect(screen.getByText(/Eigenen Dienst bereitstellen|Host Own Service/i)).toBeInTheDocument();

        // Check channel labels
        expect(screen.getByText(/Postfach aktivieren|mailbox/i)).toBeInTheDocument();
        expect(screen.getByText(/Marktplatz.*synchronisieren|marketplace/i)).toBeInTheDocument();
        expect(screen.getAllByText(/Vertrauens(liste|einstufungen).*teilen|web of trust|trust ratings/i).length).toBeGreaterThan(0);
        expect(screen.getByText(/Backups erlauben|Cloud-Sicherung nutzen|Allow Backups|Use Cloud Backup/i)).toBeInTheDocument();
    });

    it('toggles channel switch and calls settingsService.saveSettings', async () => {
        const onUpdated = vi.fn();
        render(
            <SyncServiceSettingsCard
                settings={defaultSettings}
                onSettingsUpdated={onUpdated}
            />
        );

        const mailboxToggle = screen.getByLabelText(/Postfach aktivieren|mailbox/i);
        expect(mailboxToggle).toBeChecked();

        fireEvent.click(mailboxToggle);

        await waitFor(() => {
            expect(settingsService.saveSettings).toHaveBeenCalledWith(
                expect.objectContaining({
                    postAcceptFromContacts: false,
                })
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
                settings={defaultSettings}
                onSettingsUpdated={vi.fn()}
            />
        );

        const input = screen.getByPlaceholderText(/https:\/\//i);
        fireEvent.change(input, { target: { value: 'https://test-drop.org/sync.php' } });

        const probeBtn = screen.getByRole('button', { name: /Verbinden & Prüfen|Connect & Probe/i });
        fireEvent.click(probeBtn);

        await waitFor(() => {
            expect(marketService.probePublicSync).toHaveBeenCalledWith('https://test-drop.org/sync.php');
            expect(marketService.savePublicSync).toHaveBeenCalledWith(
                expect.objectContaining({
                    writeUrl: 'https://test-drop.org/sync.php',
                    enabled: true,
                })
            );
        });
    });

    it('switches to host tab and triggers setup ZIP generation', async () => {
        vi.mocked(save).mockResolvedValue('/tmp/humoco-sync-setup.zip');
        vi.mocked(marketService.prepareSyncSetup).mockResolvedValue(undefined as any);
        vi.mocked(marketService.saveSyncSetupZip).mockResolvedValue(undefined as any);

        render(
            <SyncServiceSettingsCard
                settings={defaultSettings}
                onSettingsUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        await waitFor(() => {
            expect(screen.getByRole('button', { name: /humoco-sync-setup\.zip herunterladen|Download humoco-sync-setup\.zip/i })).toBeInTheDocument();
        });

        const downloadBtn = screen.getByRole('button', { name: /humoco-sync-setup\.zip herunterladen|Download humoco-sync-setup\.zip/i });
        fireEvent.click(downloadBtn);

        await waitFor(() => {
            expect(save).toHaveBeenCalled();
            expect(marketService.prepareSyncSetup).toHaveBeenCalled();
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
            generation: 1,
            dataDir: 'data',
            maxMembers: 300,
            maxTotalStorageMb: 50,
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
                settings={defaultSettings}
                onSettingsUpdated={vi.fn()}
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
            generation: 1,
            dataDir: 'data',
            maxMembers: 300,
            maxTotalStorageMb: 50,
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
                settings={defaultSettings}
                onSettingsUpdated={vi.fn()}
            />
        );

        const hostTabBtn = screen.getByRole('tab', { name: /Eigenen Dienst bereitstellen|Host Own Service/i });
        fireEvent.click(hostTabBtn);

        await waitFor(() => {
            expect(screen.getByText(/Zugriffsregeln|Access Rules/i)).toBeInTheDocument();
        });

        // Select level 3
        const level3Radio = screen.getByRole('radio', { name: /Engster Kreis|Inner Circle/i });
        fireEvent.click(level3Radio);
        expect(level3Radio).toBeChecked();

        // Click update rules
        const updateBtn = screen.getByRole('button', { name: /Zugriffsregeln.*aktualisieren|Update Access Rules on Server/i });
        fireEvent.click(updateBtn);

        await waitFor(() => {
            expect(marketService.updateSyncAdminConfig).toHaveBeenCalledWith(
                expect.objectContaining({
                    cardPolicy: expect.objectContaining({
                        directMin: 3,
                    }),
                }),
                true,
                'mock-wallet-pw'
            );
        });
    });
});
