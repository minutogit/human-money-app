// src/__tests__/beginnerFlows.test.tsx
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FirstStepsChecklist } from '../components/FirstStepsChecklist';
import { BackupSettingsCard } from '../components/BackupSettingsCard';
import { AddContactModal } from '../components/AddContactModal';
import ContactDialog from '../components/ContactDialog';
import { ProfileCardEditor } from '../components/marketplace/ProfileCardEditor';
import { AdEditorModal } from '../components/marketplace/AdEditorModal';
import { backupService } from '../services/backupService';
import { FrontendMyMarketProfile, AppSettings } from '../types';

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();
const mockProtectAction = vi.fn((action: (pwd?: string) => Promise<unknown>) => action(undefined));

vi.mock('../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'marketplace' },
        navigateToSettingsSection: vi.fn(),
    }),
    useOptionalNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'address_book' },
        navigateToSettingsSection: vi.fn(),
    }),
}));

vi.mock('../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: mockProtectAction,
        isSessionActive: true,
    }),
}));

vi.mock('../services/backupService', () => ({
    backupService: {
        getBackupStatus: vi.fn(),
        syncBackupFolder: vi.fn(),
        exportBackupFile: vi.fn(),
    },
}));

vi.mock('../services/contactService', () => ({
    contactService: {
        saveContact: vi.fn(),
        getContacts: vi.fn(),
    },
}));

vi.mock('../services/marketService', () => ({
    marketService: {
        getMyMarketProfile: vi.fn(),
        saveMyMarketProfile: vi.fn(),
        setCardPublished: vi.fn(),
        upsertAd: vi.fn(),
        renewAd: vi.fn(),
        deleteAd: vi.fn(),
        getMarketView: vi.fn(),
        previewContactTrustChange: vi.fn().mockResolvedValue(null),
        probePublicSync: vi.fn(),
        getSyncOverviewStatus: vi.fn().mockResolvedValue({
            configuredCount: 1,
            activeCount: 1,
            pendingItemsCount: 0,
            lastSyncTimestamp: null,
        }),
    },
}));

vi.mock('../services/settingsService', () => ({
    settingsService: {
        getSettings: vi.fn().mockResolvedValue({
            autoLockMinutes: 15,
            sessionTimeoutSeconds: 300,
            devModeUnlocked: false,
            privacyDefault: 'ask',
            preferredLanguage: 'de',
            customServers: [],
            regionalMeshSources: [],
            backupTargetFolder: '/fake/backup/path',
            backupFrequency: 'always',
        }),
        saveSettings: vi.fn(),
    },
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(),
    save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
    readFile: vi.fn(),
    writeFile: vi.fn(),
}));

describe('Beginner User Tasks (Integration Flows)', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
    });

    /**
     * Task 1: "Backup machen"
     * Flow:
     * - User sees FirstStepsChecklist on Dashboard with unconfigured backup.
     * - Clicks "Backup einrichten" step -> deep-links to { view: 'settings', initialSection: 'backup' }.
     * - In Security & Backup section (BackupSettingsCard), user clicks "Backup jetzt erstellen".
     * - Immediate backup runs and feedback is shown.
     */
    it('Task 1: guides beginner to create a backup from checklist to BackupSettingsCard', async () => {
        // Step 1: Render FirstStepsChecklist
        render(
            <FirstStepsChecklist
                hasBackup={false}
                hasContact={false}
                hasVoucher={false}
                hasSync={false}
                userId="did:key:alice"
            />
        );

        const backupStep = screen.getByTestId('checklist-step-backup');
        expect(backupStep).toBeInTheDocument();
        fireEvent.click(backupStep);

        // Verify deep link to settings backup section
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'settings',
            initialSection: 'backup',
        });

        // Step 2: Render BackupSettingsCard in the Security & Backup tab
        const mockSettings: AppSettings = {
            sessionTimeoutSeconds: 300,
            devModeUnlocked: false,
            privacyDefault: 'ask',
            customServers: [],
            regionalMeshSources: [],
            backupSyncFolder: '/home/user/Backups',
            backupFrequency: 'after_mutation',
        };

        (backupService.getBackupStatus as any).mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: true,
            syncFolder: '/home/user/Backups',
            changedBlobs: 2,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        });
        (backupService.syncBackupFolder as any).mockResolvedValue({
            status: 'ok',
            lastBackupAt: new Date().toISOString(),
            backupDirty: false,
            syncFolder: '/home/user/Backups',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: new Date().toISOString(),
            hasAuthorizedSyncBackupTarget: true,
        });

        render(<BackupSettingsCard settings={mockSettings} onSettingsUpdated={vi.fn()} />);

        // Wait for backup card to load
        await waitFor(() => {
            expect(screen.getByTestId('backup-now-button')).toBeInTheDocument();
        });

        // Click "Backup jetzt erstellen"
        const backupNowBtn = screen.getByTestId('backup-now-button');
        fireEvent.click(backupNowBtn);

        await waitFor(() => {
            expect(backupService.syncBackupFolder).toHaveBeenCalledWith(
                expect.objectContaining({
                    folderPath: '/home/user/Backups',
                })
            );
        });
    });

    /**
     * Task 2: "Kontakt hinzufügen und als 'Freund' / 'Enger Vertrauter' einstufen"
     * Flow:
     * - User opens AddContactModal (4 tiles: QR, clipboard, file, manual).
     * - User clicks "Manuell eingeben" -> onOpenManualAdd is called.
     * - ContactDialog opens with TrustLevelRadioCards.
     * - User selects Level 2 ("Freund") or Level 3 ("Enger Vertrauter") and saves.
     * - Contact is saved with correct trust level.
     */
    it('Task 2: guides beginner through adding a contact and assigning friend/confidant trust level', async () => {
        const mockOpenManualAdd = vi.fn();
        const mockContactAdded = vi.fn();

        // Step 1: User opens AddContactModal and chooses "Manuell eingeben"
        const { unmount } = render(
            <AddContactModal
                isOpen={true}
                onClose={vi.fn()}
                onOpenManualAdd={mockOpenManualAdd}
                onContactAdded={mockContactAdded}
                onSuccess={vi.fn()}
                onError={vi.fn()}
            />
        );

        expect(screen.getByTestId('add-contact-tiles')).toBeInTheDocument();
        const manualTile = screen.getByTestId('add-tile-manual');
        fireEvent.click(manualTile);

        expect(mockOpenManualAdd).toHaveBeenCalled();
        unmount();

        // Step 2: ContactDialog opens for manual input
        const mockSaveContact = vi.fn().mockResolvedValue(undefined);
        render(
            <ContactDialog
                isOpen={true}
                onClose={vi.fn()}
                onSave={mockSaveContact}
            />
        );

        // Enter contact DID and name
        const didInput = screen.getByTestId('contact-did-input');
        fireEvent.change(didInput, { target: { value: 'did:key:z6MkuBobSampleId123456789' } });

        const firstNameInput = screen.getByTestId('contact-firstname-input');
        fireEvent.change(firstNameInput, { target: { value: 'Bob' } });

        // Select Trust Level 2 ("Freund")
        const friendRadio = screen.getByTestId('trust-level-2');
        expect(friendRadio).toBeInTheDocument();
        fireEvent.click(friendRadio);

        // Click Save
        const saveBtn = screen.getByRole('button', { name: /Save|Speichern/i });
        fireEvent.click(saveBtn);

        await waitFor(() => {
            expect(mockSaveContact).toHaveBeenCalledWith(
                expect.objectContaining({
                    did: 'did:key:z6MkuBobSampleId123456789',
                    trustLevel: 2,
                    profile: expect.objectContaining({
                        firstName: 'Bob',
                    }),
                })
            );
        });
    });

    /**
     * Task 3: "Visitenkarte mit einer Fähigkeit veröffentlichen und ein Inserat aufgeben"
     * Flow:
     * - In MarketplaceView, user accesses "Meins" (MineTab).
     * - Opens ProfileCardEditor, adds a skill ("Gartenarbeit"), sets visibility, and saves.
     * - Toggles card publication on.
     * - Opens AdEditorModal, creates a 90-day offer ("Bio-Saatgut"), and publishes it.
     */
    it('Task 3: guides beginner through publishing business card skill and creating an ad', async () => {
        const mockProfile: FrontendMyMarketProfile = {
            profile: {
                id: 'did:key:alice',
                firstName: 'Alice',
                lastName: 'Musterfrau',
            },
            visibility: {
                name: 'everyone',
                organization: 'everyone',
                community: 'everyone',
                city: 'everyone',
                url: 'everyone',
                picture: 'everyone',
                street: 'contacts',
                email: 'contacts',
                phone: 'contacts',
                coordinates: 'nobody',
                gender: 'nobody',
            },
            skills: [],
            ads: [],
            maxHops: 2,
            allowInquiries: true,
            published: true,
            qrIsPartial: false,
        };

        // Step 1: Test ProfileCardEditor skill addition
        const mockSaveProfile = vi.fn().mockResolvedValue(undefined);
        const { unmount: unmountEditor } = render(
            <ProfileCardEditor
                isOpen={true}
                onClose={vi.fn()}
                myProfile={mockProfile}
                onSave={mockSaveProfile}
            />
        );

        // Click "Fähigkeit hinzufügen"
        const addSkillBtn = screen.getByTestId('add-skill-button');
        fireEvent.click(addSkillBtn);

        // Enter skill title and pricing
        const skillTitleInput = screen.getByTestId('skill-title-0');
        fireEvent.change(skillTitleInput, { target: { value: 'Gartenberatung & Permakultur' } });

        const skillPricingInput = screen.getByTestId('skill-pricing-0');
        fireEvent.change(skillPricingInput, { target: { value: '60 Minuto / Std.' } });

        // Save profile
        const saveProfileBtn = screen.getByTestId('save-profile-card-button');
        fireEvent.click(saveProfileBtn);

        await waitFor(() => {
            expect(mockSaveProfile).toHaveBeenCalledWith(
                expect.objectContaining({
                    skills: expect.arrayContaining([
                        expect.objectContaining({
                            title: 'Gartenberatung & Permakultur',
                            pricing: '60 Minuto / Std.',
                        }),
                    ]),
                })
            );
        });
        unmountEditor();

        // Step 2: Test AdEditorModal for creating a 90-day ad
        const mockSaveAd = vi.fn().mockResolvedValue(undefined);
        render(
            <AdEditorModal
                isOpen={true}
                onClose={vi.fn()}
                onSave={mockSaveAd}
            />
        );

        // Fill in Ad title and description
        const titleInput = screen.getByTestId('ad-title-input');
        fireEvent.change(titleInput, { target: { value: 'Bio-Saatgut und Tomatenpflanzen' } });

        const textInput = screen.getByTestId('ad-text-input');
        fireEvent.change(textInput, { target: { value: 'Verschiedene samenfeste Sorten aus Freilandkultur.' } });

        const pricingInput = screen.getByTestId('ad-pricing-input');
        fireEvent.change(pricingInput, { target: { value: '15 Minuto' } });

        // Submit Ad
        const submitBtn = screen.getByTestId('ad-submit-button');
        fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(mockSaveAd).toHaveBeenCalledWith(
                expect.objectContaining({
                    kind: 'offer',
                    title: 'Bio-Saatgut und Tomatenpflanzen',
                    text: 'Verschiedene samenfeste Sorten aus Freilandkultur.',
                    pricing: '15 Minuto',
                    durationDays: 90,
                    allowInquiries: true,
                })
            );
        });
    });
});
