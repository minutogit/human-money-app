import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import ContactDialog from '../ContactDialog';
import AddressBook from '../AddressBook';
import { ReceiveView } from '../ReceiveView';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';
import { Contact, WotSummaryDto, WotImportResultDto, WotProviderDto } from '../../types';

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn(async (action) => await action(undefined)),
  }),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
    warn: vi.fn(),
  },
}));

describe('WoT Contacts & Trust Ratings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('ContactDialog', () => {
    it('removes "Trusted" from predefined tags for new contacts, but includes Friends, Business, Favorites', () => {
      render(
        <ContactDialog
          isOpen={true}
          onClose={vi.fn()}
          onSave={vi.fn()}
        />
      );

      // Standard tags should be visible
      expect(screen.getByText('Friends')).toBeInTheDocument();
      expect(screen.getByText('Business')).toBeInTheDocument();
      expect(screen.getByText('Favorites')).toBeInTheDocument();

      // "Trusted" should NOT be in predefined tags for new contacts
      expect(screen.queryByRole('button', { name: /^Trusted$/i })).not.toBeInTheDocument();
    });

    it('preserves "Trusted" tag if already present in an existing contact', () => {
      const existing: Contact = {
        did: 'did:key:zTrustedUser',
        profile: { id: 'did:key:zTrustedUser', firstName: 'Alice' },
        tags: ['Trusted', 'Friends'],
        addedAt: '2024-01-01T00:00:00Z',
      };

      render(
        <ContactDialog
          isOpen={true}
          onClose={vi.fn()}
          onSave={vi.fn()}
          existingContact={existing}
        />
      );

      // "Trusted" should still show up because existingContact had it in its tags
      expect(screen.getByRole('button', { name: /^Trusted$/i })).toBeInTheDocument();
    });

    it('selecting Close Confidant sets trustLevel: 3 and default desiredHops: null', async () => {
      const onSaveMock = vi.fn().mockResolvedValue(undefined);

      render(
        <ContactDialog
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSaveMock}
          initialDid="did:key:zAliceConfidant"
        />
      );

      // Find the trust level 3 option (either by translation text or key)
      const level3Btn = screen.getByText(/contacts\.trustLevel3|Close Confidant/i).closest('button');
      expect(level3Btn).toBeInTheDocument();
      fireEvent.click(level3Btn!);

      // Reachability section was removed from per-contact dialog (managed globally)
      expect(screen.queryByText(/contacts\.reachabilityLabel|Recommendation Reach/i)).not.toBeInTheDocument();

      // Click save
      const saveBtn = screen.getByText(/contacts\.saveContact|Save Contact/i);
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(onSaveMock).toHaveBeenCalledTimes(1);
        const savedContact: Contact = onSaveMock.mock.calls[0][0];
        expect(savedContact.trustLevel).toBe(3);
        expect(savedContact.desiredHops).toBeNull();
      });
    });


    it('selecting Real Person sets trustLevel: 0 and desiredHops: 0', async () => {
      const onSaveMock = vi.fn().mockResolvedValue(undefined);

      render(
        <ContactDialog
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSaveMock}
          initialDid="did:key:zBobRealPerson"
        />
      );

      // Find trust level 0 option
      const level0Btn = screen.getByText(/contacts\.trustLevel0|Real Person/i).closest('button');
      expect(level0Btn).toBeInTheDocument();
      fireEvent.click(level0Btn!);

      // Reachability section should NOT be visible for level 0
      expect(screen.queryByText(/contacts\.reachabilityLabel|Recommendation Reach/i)).not.toBeInTheDocument();

      // Click save
      const saveBtn = screen.getByText(/contacts\.saveContact|Save Contact/i);
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(onSaveMock).toHaveBeenCalledTimes(1);
        const savedContact: Contact = onSaveMock.mock.calls[0][0];
        expect(savedContact.trustLevel).toBe(0);
        expect(savedContact.desiredHops).toBe(0);
      });
    });

    it('unrated contact has trustLevel: null and desiredHops: null', async () => {
      const onSaveMock = vi.fn().mockResolvedValue(undefined);

      render(
        <ContactDialog
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSaveMock}
          initialDid="did:key:zCharlieUnrated"
        />
      );

      // Unrated is default for new contact
      const saveBtn = screen.getByText(/contacts\.saveContact|Save Contact/i);
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(onSaveMock).toHaveBeenCalledTimes(1);
        const savedContact: Contact = onSaveMock.mock.calls[0][0];
        expect(savedContact.trustLevel).toBeNull();
        expect(savedContact.desiredHops).toBeNull();
      });
    });
  });

  describe('AddressBook Component', () => {
    const mockSummary: WotSummaryDto = {
      ratedContactsCount: 5,
      verifiedHumansCount: 2,
      closeConfidantsCount: 3,
      importedPeerListsCount: 2,
      ownListVersion: 1,
    };

    const mockContacts: Contact[] = [
      {
        did: 'did:key:zAliceConfidant',
        profile: { id: 'did:key:zAliceConfidant', firstName: 'Alice' },
        tags: ['Friends'],
        addedAt: '2024-01-01T00:00:00Z',
        trustLevel: 3,
        desiredHops: 3,
        hasPeerList: true,
      },
      {
        did: 'did:key:zBobReal',
        profile: { id: 'did:key:zBobReal', firstName: 'Bob' },
        tags: [],
        addedAt: '2024-01-02T00:00:00Z',
        trustLevel: 0,
        desiredHops: 0,
        hasPeerList: false,
      },
    ];

    it('loads and displays WoT banner stats and contact card trust pills', async () => {
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve(mockSummary);
        return Promise.resolve();
      });

      render(<AddressBook onBack={vi.fn()} />);

      // Wait for contacts to load
      await waitFor(() => {
        expect(screen.getByText('Alice')).toBeInTheDocument();
      });

      // Contact trust pills should display
      expect(screen.getByText(/contacts\.trustLevel3|Close Confidant/i)).toBeInTheDocument();
      expect(screen.getByText(/contacts\.trustLevel0|Real Person/i)).toBeInTheDocument();

      // Synced peer list badge should display for Alice
      expect(screen.getByText(/contacts\.peerListSynced|Recommendations synced/i)).toBeInTheDocument();
    });

    it('sharing WoT for a specific contact exports package with recipientDid', async () => {
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve(mockSummary);
        if (cmd === 'get_app_settings') return Promise.resolve({ lastUsedDirectory: '/tmp' });
        if (cmd === 'export_wot_package') {
          return Promise.resolve([1, 2, 3, 4]);
        }
        return Promise.resolve();
      });

      (save as unknown as Mock).mockResolvedValue('/tmp/trust-alice.wot');

      render(<AddressBook onBack={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText('Alice')).toBeInTheDocument();
      });

      // Click the share button for Alice
      const shareButtons = screen.getAllByTitle(/contacts\.shareWotForContact|Create encrypted trust file for this contact/i);
      expect(shareButtons.length).toBeGreaterThan(0);
      fireEvent.click(shareButtons[0]);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('export_wot_package', {
          recipientDid: 'did:key:zAliceConfidant',
          containerPassword: undefined,
          shareWithAllContacts: null,
          password: null,
        });
        expect(writeFile).toHaveBeenCalled();
      });
    });

    it('clicking a hop button in AddressBook invokes set_wot_max_hops', async () => {
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve({ ...mockSummary, ownListMaxHops: 3 });
        if (cmd === 'get_wot_providers') return Promise.resolve([]);
        if (cmd === 'set_wot_max_hops') return Promise.resolve({ ...mockSummary, ownListMaxHops: 2 });
        return Promise.resolve();
      });

      render(<AddressBook onBack={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText('Alice')).toBeInTheDocument();
      });

      // Find the "2 Hops" button
      const hop2Btn = screen.getByRole('button', { name: /contacts\.wotHops2|2 Hops/i });
      expect(hop2Btn).toBeInTheDocument();
      fireEvent.click(hop2Btn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('set_wot_max_hops', {
          maxHops: 2,
          password: null,
        });
      });
    });

    it('multi-recipient .wot share modal defaults to sharing with all rated contacts and invokes export_wot_package', async () => {
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve({ ...mockSummary, ratedContactsCount: 5 });
        if (cmd === 'get_wot_providers') return Promise.resolve([]);
        if (cmd === 'get_app_settings') return Promise.resolve({ lastUsedDirectory: '/tmp' });
        if (cmd === 'export_wot_package') return Promise.resolve([1, 2, 3]);
        return Promise.resolve();
      });

      (save as unknown as Mock).mockResolvedValue('/tmp/trust-network.wot');

      render(<AddressBook onBack={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText('Alice')).toBeInTheDocument();
      });

      // Open export modal
      const exportBtn = screen.getByRole('button', { name: /contacts\.exportWotButton|Share Trust File/i });
      fireEvent.click(exportBtn);

      // Verify modal is open and shows "all contacts" option
      await waitFor(() => {
        expect(screen.getByText(/contacts\.exportModeAllContacts|Share with All Rated Contacts/i)).toBeInTheDocument();
      });

      // Confirm export button inside modal footer
      const confirmExportBtn = screen.getAllByRole('button', { name: /contacts\.exportWotButton|Share Trust File/i })[1];
      fireEvent.click(confirmExportBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('export_wot_package', {
          recipientDid: undefined,
          containerPassword: undefined,
          shareWithAllContacts: true,
          password: null,
        });
        expect(writeFile).toHaveBeenCalled();
      });
    });

    it('progressive disclosure: providers tab is hidden when empty and appears when provider exists', async () => {
      const mockProvider: WotProviderDto = {
        issuerDid: 'did:key:zProvider123',
        name: 'Regional Alliance',
        version: 1,
        issuedAt: Math.floor(Date.now() / 1000),
        sourceUrl: 'https://alliance.example/filter.wot',
        memberCount: 42,
        trustLevel: 1,
        importedAt: Math.floor(Date.now() / 1000),
        isStale: false,
      };

      // Test 1: Empty providers -> Tab switcher is NOT rendered
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve(mockSummary);
        if (cmd === 'get_wot_providers') return Promise.resolve([]);
        return Promise.resolve();
      });

      const { unmount } = render(<AddressBook onBack={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText('Alice')).toBeInTheDocument();
      });

      expect(screen.queryByText(/contacts\.tabProviders|Community Seals & Providers/i)).not.toBeInTheDocument();
      unmount();

      // Test 2: Provider present -> Tab switcher is rendered, switch tab, update trust
      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
        if (cmd === 'get_wot_summary') return Promise.resolve(mockSummary);
        if (cmd === 'get_wot_providers') return Promise.resolve([mockProvider]);
        if (cmd === 'set_wot_provider_trust') return Promise.resolve([{ ...mockProvider, trustLevel: 2 }]);
        return Promise.resolve();
      });

      render(<AddressBook onBack={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText(/contacts\.tabProviders|Community Seals & Providers/i)).toBeInTheDocument();
      });

      // Switch to providers tab
      const providersTabBtn = screen.getByRole('tab', { name: /contacts\.tabProviders|Community Seals & Providers/i });
      fireEvent.click(providersTabBtn);

      await waitFor(() => {
        expect(screen.getByText('Regional Alliance')).toBeInTheDocument();
        expect(screen.getByText(/contacts\.providerMembers|42 Members/i)).toBeInTheDocument();
      });

      // Click trust level 2 button for provider
      const trustLevel2Btn = screen.getByRole('button', { name: /contacts\.trustLevel2|Good Contact/i });
      fireEvent.click(trustLevel2Btn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('set_wot_provider_trust', {
          issuerDid: 'did:key:zProvider123',
          level: 2,
          password: null,
        });
      });
    });
  });


  describe('ReceiveView with .wot files', () => {
    it('processes .wot file and displays import summary modal', async () => {
      const mockImportResult: WotImportResultDto = {
        senderDid: 'did:key:zAliceSender',
        senderProfile: { id: 'did:key:zAliceSender', firstName: 'Alice' },
        imported: 4,
        updated: 1,
        merged: 0,
        skippedUnchanged: 0,
        rejectedSpam: 0,
        isKnownContact: true,
      };

      (invoke as unknown as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_voucher_standards') return Promise.resolve([]);
        if (cmd === 'get_app_settings') return Promise.resolve({});
        if (cmd === 'import_wot_package') return Promise.resolve(mockImportResult);
        return Promise.resolve();
      });

      (open as unknown as Mock).mockResolvedValue('/tmp/recommendations.wot');
      (readFile as unknown as Mock).mockResolvedValue(new Uint8Array([10, 20, 30]));

      render(<ReceiveView onBack={vi.fn()} onReceiveSuccess={vi.fn()} />);

      // Trigger file selection
      const selectBtn = screen.getByText(/transfer\.importFile\.selectFile|Select File/i);
      fireEvent.click(selectBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('import_wot_package', {
          containerBytes: [10, 20, 30],
          containerPassword: null,
          password: null,
        });
      });

      // Summary modal should be visible
      await waitFor(() => {
        expect(screen.getByText(/transfer\.wotImport\.title|Trust Network Updated/i)).toBeInTheDocument();
      });

      // Click update contact action button
      const actionBtn = screen.getByText(/transfer\.wotImport\.updateContactAction|Adjust Trust Rating/i);
      fireEvent.click(actionBtn);

      // ContactDialog should now be open with sender DID
      await waitFor(() => {
        expect(screen.getByDisplayValue('did:key:zAliceSender')).toBeInTheDocument();
      });
    });
  });
});
