// src/__tests__/beginnerFlows.test.tsx
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { FirstStepsChecklist } from '../components/wallet/FirstStepsChecklist';
import { BackupSettingsCard } from '../components/settings/BackupSettingsCard';
import { AddContactModal } from '../components/contacts/AddContactModal';
import ContactDialog from '../components/contacts/ContactDialog';
import { ProfileCardEditor } from '../components/marketplace/ProfileCardEditor';
import { AdEditorModal } from '../components/marketplace/AdEditorModal';
import { SendView } from '../components/transfer/SendView';
import { ReceiveView } from '../components/transfer/ReceiveView';
import { SettingsView } from '../components/settings/SettingsView';
import { CreateVoucher } from '../components/wallet/CreateVoucher';
import { HelpCenterView } from '../components/support/HelpCenterView';
import { ConflictDetailsView } from '../components/conflicts/ConflictDetailsView';
import { UiModeProvider } from '../context/UiModeContext';
import { UiModeToggle } from '../components/ui/UiModeToggle';
import { Advanced } from '../components/ui/Advanced';
import { TechId } from '../components/ui/TechId';
import { backupService } from '../services/backupService';
import { contactService } from '../services/contactService';
import { settingsService } from '../services/settingsService';
import { AppConfigDto, FrontendMyMarketProfile, Contact } from '../types';

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
        isAuthenticated: true,
        hasCachedSession: true,
    }),
    useOptionalSession: () => null,
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
        getMyMarketProfile: vi.fn().mockResolvedValue(null),
        saveMyMarketProfile: vi.fn(),
        setCardPublished: vi.fn(),
        upsertAd: vi.fn(),
        renewAd: vi.fn(),
        deleteAd: vi.fn(),
        getMarketView: vi.fn(),
        previewContactTrustChange: vi.fn().mockResolvedValue(null),
        probePublicSync: vi.fn(),
        discoverAvailableSyncs: vi.fn().mockResolvedValue([]),
        getSyncOverviewStatus: vi.fn().mockResolvedValue({
            configuredCount: 1,
            activeCount: 1,
            pendingItemsCount: 0,
            lastSyncTimestamp: null,
        }),
    },
}));

function createBeginnerConfig(backupSyncFolder: string | null = '/fake/backup/path'): AppConfigDto {
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
            sessionTimeoutSeconds: 300,
            privacyDefault: 'ask',
            devModeUnlocked: false,
            autoBackupEnabled: true,
            syncBackupEnabled: true,
            backupSyncFolder,
            backupProtectionMode: null,
            ownNominatedLockServer: null,
            marketSelectedSyncs: [],
            publicSyncs: [],
        },
        lifecycle: {
            pendingRestoreL2Reconciliation: false,
            backupDirty: false,
            lastBackupAt: null,
            lastSyncBackupAt: null,
            l2ProbeCache: {},
            l2AnchorTemplatesMeta: [],
        },
    };
}

vi.mock('../services/settingsService', () => ({
    settingsService: {
        getConfig: vi.fn().mockImplementation(async () => createBeginnerConfig()),
        patchUiPreferences: vi.fn().mockImplementation(async (patch: any) => ({
            ...createBeginnerConfig().uiPreferences,
            ...patch,
        })),
        patchSecurityPolicy: vi.fn().mockImplementation(async (patch: any) => ({
            ...createBeginnerConfig().securityPolicy,
            ...patch,
        })),
        prewarmL2Checkout: vi.fn().mockResolvedValue(undefined),
        getReachPreset: vi.fn().mockResolvedValue('medium'),
        applyReachPreset: vi.fn().mockResolvedValue({}),
    },
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(),
    save: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
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
        const mockConfig = createBeginnerConfig('/home/user/Backups');

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

        render(<BackupSettingsCard config={mockConfig} onConfigUpdated={vi.fn()} />);

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

        // Step 1: User opens AddContactModal in Simple mode and chooses manual entry via Other ways disclosure
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

        expect(screen.getByTestId('smart-add-contact-panel')).toBeInTheDocument();
        expect(screen.queryByTestId('add-contact-tiles')).not.toBeInTheDocument();
        fireEvent.click(screen.getByTestId('add-contact-other-ways-toggle'));
        fireEvent.click(screen.getByTestId('add-other-manual'));

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

        // Verify Simple mode shows visibility presets instead of 11 individual badges
        expect(screen.getByTestId('preset-option-local')).toBeInTheDocument();
        expect(screen.queryByTestId('visibility-btn-name')).not.toBeInTheDocument();

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

        // Step 2: Test AdEditorModal for creating a 90-day ad using Simple price chips
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

        fireEvent.click(screen.getByTestId('price-chip-minuto'));
        fireEvent.change(screen.getByTestId('ad-price-amount-input'), { target: { value: '15' } });

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

    it('Task 4: hides technical Advanced elements and raw did:key in Simple mode and reveals them in Expert mode', () => {
        const sampleDid = 'did:key:z6MkuBeginnerSampleDid987654';

        const { container } = render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <TechId value={sampleDid} kind="did" />
                <Advanced>
                    <div data-testid="advanced-raw-panel">Raw Transaction JSON</div>
                </Advanced>
            </UiModeProvider>
        );

        // Simple mode by default: no Advanced panel, no raw did:key in DOM
        expect(screen.queryByTestId('advanced-raw-panel')).not.toBeInTheDocument();
        expect(screen.getByTestId('tech-id-value')).toHaveTextContent('…987654');
        expect(container.innerHTML).not.toContain('did:key:');

        // Switch to Expert mode
        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.getByTestId('advanced-raw-panel')).toBeInTheDocument();
        expect(screen.getByTestId('tech-id-value')).toHaveTextContent(sampleDid);
        expect(localStorage.length).toBe(0);

        // Switch back to Simple mode
        fireEvent.click(screen.getByTestId('ui-mode-simple'));
        expect(screen.queryByTestId('advanced-raw-panel')).not.toBeInTheDocument();
        expect(container.innerHTML).not.toContain('did:key:');
    });

    it('Task 5 (APP-UX-09 P3): keeps SendView and ReceiveView simple, unified, and free of raw did:key in Simple mode', async () => {
        const ownDid = 'did:key:z6MkuBeginnerOwnDid000111';
        const aliceDid = 'did:key:z6MkuBeginnerAlice222333';
        const sampleConfig = createBeginnerConfig('/fake/backup/path');
        const sampleStandard = {
            id: 'std-minuto',
            displayName: 'Minuto (Min)',
            content: 'name = "Minuto"\nuuid = "std-minuto"\nunit = "Min"\nabbreviation = "Min"\namount_decimal_places = 2',
        };

        const mockContacts: Contact[] = [
            {
                did: aliceDid,
                profile: { id: aliceDid, firstName: 'Alice', lastName: 'Beginner' },
                tags: [],
                addedAt: '2026-01-01T00:00:00Z',
            },
        ];

        vi.mocked(contactService.getContacts).mockResolvedValue(mockContacts);
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        });

        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
            if (cmd === 'get_voucher_summaries') {
                return [
                    {
                        localInstanceId: 'v-real-1',
                        status: 'active',
                        validUntil: '2099-01-01',
                        creatorId: ownDid,
                        description: 'Zeitgutschein',
                        currentAmount: '50.00',
                        unit: 'Min',
                        rawStandardName: 'Minuto',
                        voucherStandardUuid: 'std-minuto',
                        transactionCount: 1,
                        signaturesCount: 1,
                        hasCollateral: false,
                        isTestVoucher: false,
                        displayCurrency: 'Minuto',
                        displayStandardName: 'Minuto',
                        allowPartialTransfers: true,
                    },
                ];
            }
            if (cmd === 'get_active_asset_classes') {
                return [
                    {
                        standardUuid: 'std-minuto',
                        isTestVoucher: false,
                        displayStandardName: 'Minuto',
                        displayCurrency: 'Minuto',
                    },
                ];
            }
            if (cmd === 'get_voucher_standards') return [sampleStandard];
            if (cmd === 'get_contacts') return mockContacts;
            if (cmd === 'get_user_profile') {
                return { id: ownDid, firstName: 'Max', lastName: 'Mustermann' };
            }
            if (cmd === 'get_user_id') return ownDid;
            if (cmd === 'get_app_config') return sampleConfig;
            if (cmd === 'encode_text_qr_svg') return '<svg data-testid="mock-beginner-qr"></svg>';
            return undefined;
        });

        // 1. SendView in Simple mode
        const sendRender = render(
            <UiModeProvider>
                <SendView />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(sendRender.container.querySelector('#target-amount')).toBeInTheDocument();
        });

        expect(sendRender.container.textContent).not.toContain('did:key:');
        expect(screen.getByTestId('manual-voucher-selection-toggle')).toBeInTheDocument();
        expect(screen.queryByTestId(`quick-contact-${aliceDid}`)).toBeInTheDocument();

        fireEvent.click(screen.getByTestId(`quick-contact-${aliceDid}`));
        const amountInput = sendRender.container.querySelector('#target-amount') as HTMLInputElement;
        fireEvent.change(amountInput, { target: { value: '20' } });
        expect(amountInput.value).toBe('20');

        sendRender.unmount();

        // 2. ReceiveView in Simple mode
        const receiveRender = render(
            <UiModeProvider>
                <ReceiveView onBack={vi.fn()} onReceiveSuccess={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('receive-qr-svg')).toBeInTheDocument();
            expect(receiveRender.container.querySelector('#bundle-drop-zone')).toBeInTheDocument();
        });

        expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
        expect(receiveRender.container.textContent).not.toContain('did:key:');
    });

    /**
     * Task 6: Simple-mode settings show 6 clear blocks without expert tabs or Save button,
     * hide technical DIDs, and auto-save all 4 sync channels via the master toggle.
     */
    it('Task 6: beginner uses simple 6-block settings with auto-save master sync toggle', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'ok',
            lastBackupAt: '2026-11-15T12:00:00Z',
            backupDirty: false,
            syncFolder: '/fake/backup/path',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        } as Awaited<ReturnType<typeof backupService.getBackupStatus>>);
        vi.mocked(settingsService.patchUiPreferences).mockResolvedValue({
            ...createBeginnerConfig().uiPreferences,
            postAcceptFromContacts: false,
            marketSyncEnabled: false,
            wotShareViaSync: false,
        });
        vi.mocked(settingsService.patchSecurityPolicy).mockResolvedValue({
            ...createBeginnerConfig().securityPolicy,
            syncBackupEnabled: false,
        });

        const settingsRender = render(
            <UiModeProvider>
                <SettingsView onBack={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('simple-blocks-layout')).toBeInTheDocument();
        });

        expect(screen.queryByTestId('expert-tabs-layout')).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: /Save System Changes|Einstellungen speichern/i })).not.toBeInTheDocument();
        expect(settingsRender.container.textContent).not.toContain('did:key:');

        expect(settingsRender.container.querySelector('#settings-profile-section')).toBeInTheDocument();
        expect(settingsRender.container.querySelector('#settings-security-section')).toBeInTheDocument();
        expect(settingsRender.container.querySelector('#settings-reach-section')).toBeInTheDocument();
        expect(settingsRender.container.querySelector('#settings-sync-section')).toBeInTheDocument();
        expect(settingsRender.container.querySelector('#settings-view-section')).toBeInTheDocument();
        expect(settingsRender.container.querySelector('#settings-danger-section')).toBeInTheDocument();

        const masterToggle = screen.getByTestId('auto-sync-master-toggle');
        fireEvent.click(masterToggle);

        await waitFor(() => {
            expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
                { syncBackupEnabled: false },
                undefined
            );
            expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
                {
                    postAcceptFromContacts: false,
                    marketSyncEnabled: false,
                    wotShareViaSync: false,
                },
                undefined
            );
        });
    });

    /**
     * Task 7: Simple-mode voucher creation adopts profile summary, defaults to Test mode,
     * hides expert fields (validity, manage standards, gender for FreeTaler, collateral),
     * and opens confirmation modal.
     */
    it('Task 7: beginner creates a test voucher in simple mode with adopted profile and no expert fields', async () => {
        const simpleStandards = [
            {
                id: 'freetaler_v1',
                displayName: 'FreeTaler',
                content: 'name = "FreeTaler"\nunit = "Taler"',
            },
        ];

        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
            if (cmd === 'get_voucher_standards') return simpleStandards;
            if (cmd === 'get_user_profile') {
                return {
                    firstName: 'Alice',
                    lastName: 'Musterfrau',
                    gender: '0',
                    serviceOffer: 'Gardening',
                    address: { city: 'Freiburg' },
                };
            }
            if (cmd === 'parse_standard_toml') {
                return {
                    immutable: {
                        identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
                        blueprint: {
                            unit: 'Taler',
                            primaryRedemptionType: 'goodsOrServices',
                            collateralType: 'personalGuarantee',
                        },
                        features: {
                            allowPartialTransfers: true,
                            balancesAreSummable: true,
                            amountDecimalPlaces: 4,
                            privacyMode: 'public',
                            allowedTTypes: [],
                        },
                        issuance: {
                            validityDurationRange: [],
                            issuanceMinimumValidityDuration: 'P1Y',
                            additionalSignaturesRange: [],
                            allowedSignatureRoles: [],
                        },
                        customRules: {},
                    },
                    mutable: {
                        metadata: { issuerName: 'Human Money Project', keywords: [] },
                        appConfig: { defaultValidityDuration: 'P1Y' },
                        i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
                    },
                };
            }
            return undefined;
        });

        const { container } = render(
            <UiModeProvider>
                <CreateVoucher onVoucherCreated={vi.fn()} onCancel={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('creator-profile-summary')).toHaveTextContent('Alice Musterfrau, Freiburg');
        });

        expect(screen.queryByTestId('manage-standards-button')).not.toBeInTheDocument();
        expect(screen.queryByTestId('voucher-validity-toggle')).not.toBeInTheDocument();
        expect(screen.queryByTestId('creator-gender-select')).not.toBeInTheDocument();
        expect(container.textContent).not.toContain('did:key:');

        const typeSelect = container.querySelector('#voucher-type') as HTMLSelectElement;
        expect(typeSelect).toBeInTheDocument();
        fireEvent.change(typeSelect, { target: { value: 'freetaler_v1' } });

        await waitFor(() => {
            expect(screen.getByTestId('standard-short-description')).toBeInTheDocument();
        });

        const amountInput = container.querySelector('#voucher-amount') as HTMLInputElement;
        fireEvent.change(amountInput, { target: { value: '25' } });

        const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
        fireEvent.click(submitButton);

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
        });
    });

    it('Task 8: P10 organizes Help Center for beginners and answers 3 conflict questions calmly', async () => {
        render(
            <UiModeProvider>
                <HelpCenterView onBack={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('help-group-heading-firstSteps')).toBeInTheDocument();
        });

        expect(screen.getByTestId('help-group-heading-faq')).toBeInTheDocument();
        expect(screen.getByTestId('help-center-expert-topics-toggle')).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: 'For Advanced Users' })).not.toBeInTheDocument();
        expect(screen.getByText('Which Backup Do I Need?')).toBeInTheDocument();
        expect(screen.getByText('Is This Bitcoin or Cryptocurrency?')).toBeInTheDocument();

        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
            if (cmd === 'get_proof_of_double_spend') {
                return {
                    proof: {
                        proofId: 'proof-task8-abc',
                        offenderId: 'did:key:z6MkTask8OffenderXYZ999',
                        reportTimestamp: '2024-06-01T12:00:00Z',
                        affectedVoucherName: 'Minuto Neighbor',
                        conflictingTransactions: [
                            { tId: 'tx-a', tTime: '2024-06-01T10:00:00Z', amount: '15', recipientId: 'did:key:rec1' },
                            { tId: 'tx-b', tTime: '2024-06-01T10:05:00Z', amount: '15', recipientId: 'did:key:rec2' },
                        ],
                        isResolved: false,
                        isCryptographicallyAttributed: true,
                    },
                    localOverride: false,
                    conflictRole: 'victim',
                };
            }
            if (cmd === 'get_contacts') return [];
            return undefined;
        });

        const { container: conflictContainer } = render(
            <UiModeProvider>
                <ConflictDetailsView proofId="proof-task8-abc" onBack={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByTestId('conflict-three-questions')).toBeInTheDocument();
        });

        expect(screen.getByText('What happened?')).toBeInTheDocument();
        expect(screen.getByText('Is my voucher still valid?')).toBeInTheDocument();
        expect(screen.getByText('What should I do?')).toBeInTheDocument();
        expect(conflictContainer.textContent).not.toContain('did:key:z6MkTask8');
        expect(screen.queryByTestId('conflict-evidence-section')).not.toBeInTheDocument();
        expect(screen.queryByTestId('conflict-local-override-section')).not.toBeInTheDocument();
    });
});
