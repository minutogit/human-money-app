import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { DropManagementModal } from '../DropManagementModal';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import {
  FrontendDropAdminStatus,
  FrontendDiscoveredDrop,
  FrontendDropSyncResult,
  FrontendDropSetupDraft,
  FrontendDropConnectionTestResult,
} from '../../../types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(() => Promise.resolve('/path/to/humoco-drop.zip')),
  open: vi.fn(() => Promise.resolve('/path/to/file')),
}));

vi.mock('../../../utils/clipboard', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve(true)),
}));

describe('DropManagementModal Component', () => {
  const mockDiscoveredDrops: FrontendDiscoveredDrop[] = [
    {
      descriptor: {
        writeUrl: 'https://drop1.example.com/drop.php',
        readUrl: 'https://drop1.example.com/d-1111',
        dropPub: 'DropPub11111111111111111111111111111111111',
      },
      isAuthorized: true,
      isSelected: false,
      sourceLabel: 'Alice Honey Farm',
    },
    {
      descriptor: {
        writeUrl: 'https://drop2.example.com/drop.php',
        readUrl: 'https://drop2.example.com/d-2222',
        dropPub: 'DropPub22222222222222222222222222222222222',
      },
      isAuthorized: false,
      isSelected: false,
      sourceLabel: 'Bob Builder',
    },
    {
      descriptor: {
        writeUrl: 'https://drop3.example.com/drop.php',
        readUrl: 'https://drop3.example.com/d-3333',
        dropPub: 'DropPub33333333333333333333333333333333333',
      },
      isAuthorized: true,
      isSelected: true,
      sourceLabel: 'Eigene Ablage',
    },
  ];

  const mockAdminStatus: FrontendDropAdminStatus = {
    descriptor: {
      writeUrl: 'https://mydrop.org/drop.php',
      readUrl: 'https://mydrop.org/d-9999',
      dropPub: 'DropPubAdmin9999999999999999999999999999',
    },
    dataDir: 'd-9999',
    maxMembers: 300,
    cardPolicy: {
      directMin: 2,
      hop2Enabled: true,
      bridgeMin: 2,
      hop2Min: 2,
    },
    allow: ['did:key:zCustomFriend'],
    deny: ['did:key:zBadActor'],
    blockedAccessIds: ['deadbeef000011112222333344445555'],
    directMemberCount: 12,
    hop2MemberCount: 45,
    totalMemberCount: 57,
    activeSlotIds: ['0123456789abcdef0123456789abcdef'],
  };

  const mockSyncResult: FrontendDropSyncResult = {
    skippedWifiOnly: false,
    uploadedTo: 2,
    polledDrops: 3,
    decryptedSlots: 5,
    importedCards: 2,
    errors: [],
  };

  const mockSetupDraft: FrontendDropSetupDraft = {
    descriptor: {
      writeUrl: '',
      readUrl: '',
      dropPub: 'DraftPub12345',
    },
    dataDir: 'd-draft123',
    adminTokenHash: 'hash123456',
    maxMembers: 300,
    policy: {
      directMin: 2,
      hop2Enabled: true,
      bridgeMin: 2,
      hop2Min: 2,
    },
    zipBase64: 'UEsDBBQAAAAIA...',
  };

  const mockTestConnResult: FrontendDropConnectionTestResult = {
    success: true,
    message: 'Connection verified successfully (57 active members).',
    writeOk: true,
    readOk: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_drop_admin_status') {
        return Promise.resolve(mockAdminStatus);
      }
      if (cmd === 'discover_available_drops') {
        return Promise.resolve(mockDiscoveredDrops);
      }
      if (cmd === 'sync_market_drops') {
        return Promise.resolve(mockSyncResult);
      }
      if (cmd === 'prepare_drop_setup') {
        return Promise.resolve(mockSetupDraft);
      }
      if (cmd === 'save_drop_setup_zip') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'test_drop_connection') {
        return Promise.resolve(mockTestConnResult);
      }
      if (cmd === 'finalize_drop_setup') {
        return Promise.resolve(mockAdminStatus);
      }
      if (cmd === 'update_drop_admin_config') {
        return Promise.resolve(mockAdminStatus);
      }
      if (cmd === 'block_drop_slot') {
        return Promise.resolve({
          ...mockAdminStatus,
          blockedAccessIds: [...mockAdminStatus.blockedAccessIds, 'newblockedaccessid'],
          activeSlotIds: [],
        });
      }
      if (cmd === 'delete_drop_admin') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'set_selected_upload_drops') {
        return Promise.resolve([]);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders Tab 1 (Use Drops): lists discovered drops, displays badges, and triggers sync', async () => {
    const onSyncMock = vi.fn();
    render(
      <DropManagementModal
        isOpen={true}
        onClose={vi.fn()}
        onSyncComplete={onSyncMock}
      />
    );

    // Initial load discovers drops
    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
      expect(screen.getByText('Bob Builder')).toBeInTheDocument();
      expect(screen.getByText('Eigene Ablage')).toBeInTheDocument();
    });

    // Alice is authorized, Bob is read-only
    expect(screen.getAllByText(/Schreibberechtigt|Write Authorized/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Nur Lesen|Read Only/i)).toBeInTheDocument();

    // Trigger sync
    const syncBtn = screen.getByRole('button', { name: /Jetzt synchronisieren|Sync Now/i });
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('sync_market_drops', expect.objectContaining({
        forceUpload: false,
      }));
    });

    // Verification of feedback counters
    await waitFor(() => {
      expect(screen.getByText(/Auf 2 Ablage\(n\) hochgeladen|Uploaded to 2 drop\(s\)/i)).toBeInTheDocument();
      expect(screen.getByText(/3 Ablage\(n\) abgefragt|Polled 3 drop\(s\)/i)).toBeInTheDocument();
      expect(screen.getByText(/5 Slot\(s\) entschlüsselt|Decrypted 5 slot\(s\)/i)).toBeInTheDocument();
      expect(screen.getByText(/2 Karte\(n\) importiert|Imported 2 card\(s\)/i)).toBeInTheDocument();
    });

    expect(onSyncMock).toHaveBeenCalledWith(2);
  });

  it('triggers force upload of own card when clicking "Eigene Karte neu hochladen"', async () => {
    render(
      <DropManagementModal
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });

    const forceBtn = screen.getByRole('button', { name: /Eigene Karte neu hochladen|Force Upload Own Card/i });
    fireEvent.click(forceBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('sync_market_drops', expect.objectContaining({
        forceUpload: true,
      }));
    });
  });

  it('allows selection and saving of upload drops up to 3 targets', async () => {
    render(
      <DropManagementModal
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });

    // Find checkboxes
    const checkboxes = screen.getAllByRole('checkbox');
    // Drop 3 is already selected, let's select Drop 1 (Alice)
    fireEvent.click(checkboxes[0]);

    // Save selection
    const saveTargetsBtn = screen.getByRole('button', { name: /Upload-Ablagen speichern|Save Upload Targets/i });
    fireEvent.click(saveTargetsBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('set_selected_upload_drops', expect.objectContaining({
        drops: expect.arrayContaining([
          expect.objectContaining({ readUrl: 'https://drop1.example.com/d-1111' })
        ])
      }));
    });
  });

  it('renders Tab 2 Admin View when drop is active: shows counters, updates policy, and blocks slot', async () => {
    render(
      <DropManagementModal
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Alice Honey Farm')).toBeInTheDocument();
    });

    // Switch to Tab 2
    const adminTab = screen.getByRole('tab', { name: /Eigene Ablage|Own Drop/i });
    fireEvent.click(adminTab);

    // Verify Admin View information
    await waitFor(() => {
      expect(screen.getByText('https://mydrop.org/d-9999')).toBeInTheDocument();
      expect(screen.getByText('https://mydrop.org/drop.php')).toBeInTheDocument();
      expect(screen.getByText('12')).toBeInTheDocument(); // direct members
      expect(screen.getByText('45')).toBeInTheDocument(); // hop 2 members
      expect(screen.getByText('57 / 300')).toBeInTheDocument(); // total / max
    });

    // Save & push policy
    const savePolicyBtn = screen.getByRole('button', { name: /Konfiguration speichern & übertragen|Save & Push Configuration/i });
    fireEvent.click(savePolicyBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_drop_admin_config', expect.objectContaining({
        pushNow: true,
      }));
    });

    // Block active slot
    const blockBtn = screen.getAllByRole('button', { name: /Sperren|Block/i })[1];
    fireEvent.click(blockBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('block_drop_slot', expect.objectContaining({
        accessId: '0123456789abcdef0123456789abcdef',
      }));
    });

    // Manual block slot input
    const manualInput = screen.getByPlaceholderText(/32-stellige hexadezimale Access-ID|32-character hex Access-ID/i);
    fireEvent.change(manualInput, { target: { value: 'cafebabecafebabecafebabecafebabe' } });
    const manualBlockBtn = screen.getAllByRole('button', { name: /Sperren|Block/i })[0];
    fireEvent.click(manualBlockBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('block_drop_slot', expect.objectContaining({
        accessId: 'cafebabecafebabecafebabecafebabe',
      }));
    });
  });

  it('runs Wizard setup when no drop admin exists: generates ZIP, tests connection, and finalizes', async () => {
    // Return null for get_drop_admin_status
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_drop_admin_status') return Promise.resolve(null);
      if (cmd === 'discover_available_drops') return Promise.resolve([]);
      if (cmd === 'prepare_drop_setup') return Promise.resolve(mockSetupDraft);
      if (cmd === 'save_drop_setup_zip') return Promise.resolve(undefined);
      if (cmd === 'test_drop_connection') return Promise.resolve(mockTestConnResult);
      if (cmd === 'finalize_drop_setup') return Promise.resolve(mockAdminStatus);
      return Promise.resolve(undefined);
    });

    render(
      <DropManagementModal
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    // Switch to Tab 2
    const adminTab = screen.getByRole('tab', { name: /Eigene Ablage|Own Drop/i });
    fireEvent.click(adminTab);

    // Step 1: Configuration & ZIP
    await waitFor(() => {
      expect(screen.getByText(/Schritt 1: Konfiguration & ZIP-Paket|Step 1: Configuration & ZIP Package/i)).toBeInTheDocument();
    });

    // Generate config & ZIP
    const generateBtn = screen.getByRole('button', { name: /Konfiguration & ZIP erzeugen|Generate Configuration & ZIP/i });
    fireEvent.click(generateBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('prepare_drop_setup', expect.objectContaining({
        maxMembers: 300,
        policy: expect.objectContaining({ directMin: 2 }),
      }));
    });

    // Save ZIP package
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /ZIP-Paket speichern|Save ZIP Package/i })).toBeInTheDocument();
    });
    const saveZipBtn = screen.getByRole('button', { name: /ZIP-Paket speichern|Save ZIP Package/i });
    fireEvent.click(saveZipBtn);

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(expect.objectContaining({
        defaultPath: 'humoco-drop.zip',
      }));
      expect(invoke).toHaveBeenCalledWith('save_drop_setup_zip', expect.objectContaining({
        filePath: '/path/to/humoco-drop.zip',
      }));
    });

    // Next to step 2
    const nextBtn = screen.getByRole('button', { name: /Weiter zu Schritt 2|Next to Step 2/i });
    fireEvent.click(nextBtn);

    // Step 2: URL & connection test
    await waitFor(() => {
      expect(screen.getByText(/Schritt 2: Server-URL & Verbindungstest|Step 2: Server URL & Connection Test/i)).toBeInTheDocument();
    });

    const urlInput = screen.getByPlaceholderText(/https:\/\/ablage\.beispiel\.de|https:\/\/drop\.example\.com/i);
    fireEvent.change(urlInput, { target: { value: 'https://testdrop.net' } });

    // Test connection
    const testBtn = screen.getByRole('button', { name: /Verbindung testen|Test Connection/i });
    fireEvent.click(testBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('test_drop_connection', expect.objectContaining({
        baseUrl: 'https://testdrop.net',
      }));
    });

    await waitFor(() => {
      expect(screen.getByText(/Connection verified successfully/i)).toBeInTheDocument();
    });

    // Finalize setup
    const finalizeBtn = screen.getByRole('button', { name: /Einrichtung abschließen|Finalize Setup/i });
    fireEvent.click(finalizeBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('finalize_drop_setup', expect.objectContaining({
        baseUrl: 'https://testdrop.net',
      }));
    });
  });
});
