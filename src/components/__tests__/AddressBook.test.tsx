import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import AddressBook from '../AddressBook';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { Contact } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((action) => action(undefined)),
  }),
}));

const mockNavigateToSettingsSection = vi.fn();
vi.mock('../../context/NavigationContext', () => ({
  useOptionalNavigation: () => ({
    navigateToSettingsSection: mockNavigateToSettingsSection,
  }),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../ContactDialog', () => ({
  default: ({ isOpen, onClose, onSave }: { isOpen: boolean, onClose: () => void, onSave: (c: Contact) => void }) => {
    if (!isOpen) return null;
    return (
      <div data-testid="contact-dialog">
        <button onClick={() => onSave({ did: 'did:key:z789', profile: { id: 'did:key:z789', firstName: 'New', lastName: 'Contact' }, tags: [], addedAt: '2024-01-03' })}>Save</button>
        <button onClick={onClose}>Cancel</button>
      </div>
    );
  },
}));



describe('AddressBook Component', () => {
  const mockContacts: Contact[] = [
    {
      did: 'did:key:z123',
      profile: {
        id: 'did:key:z123',
        firstName: 'John',
        lastName: 'Doe',
      },
      tags: ['friend'],
      addedAt: '2024-01-01',
    },
    {
      did: 'did:key:z456',
      profile: {
        id: 'did:key:z456',
        firstName: 'Jane',
        lastName: 'Smith',
      },
      tags: ['colleague'],
      addedAt: '2024-01-02',
    },
  ];

  const mockOnBack = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') {
        return Promise.resolve(mockContacts);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders address book header and contact list', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Address Book/i)).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
      expect(screen.getByText(/Jane Smith/i)).toBeInTheDocument();
    });
  });

  it('loads contacts from backend on mount', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_contacts');
    });
  });

  it('displays contact tags', async () => {
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getAllByText(/friend/i)[0]).toBeInTheDocument();
      expect(screen.getAllByText(/colleague/i)[0]).toBeInTheDocument();
    });
  });

  it('calls save_contact via protectAction when saving a contact', async () => {
    render(<AddressBook onBack={mockOnBack} />);
    
    // Wait for initial load
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_contacts'));
    (invoke as Mock).mockClear(); // Clear the initial 'get_contacts' call
    
    const newBtn = screen.getByText(/New Contact/i);
    fireEvent.click(newBtn);
    
    await screen.findByTestId('contact-dialog');
    const saveBtn = screen.getByRole('button', { name: /Save/i });
    fireEvent.click(saveBtn);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_contact', expect.objectContaining({
        password: undefined,
        contact: expect.objectContaining({ did: 'did:key:z789' })
      }));
    });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_contacts');
    });
  });

  it('calls delete_contact via protectAction when deleting a contact', async () => {
    render(<AddressBook onBack={mockOnBack} />);
    
    // Wait for contacts to load
    await waitFor(() => screen.getByText(/John Doe/i));
    
    // Click delete button for John Doe
    const deleteBtns = screen.getAllByTitle(/Remove Contact/i);
    fireEvent.click(deleteBtns[0]);
    
    // Confirm in modal - Use more specific selector to avoid ambiguity
    const confirmBtn = screen.getByRole('button', { name: /^Delete Contact$/i });
    fireEvent.click(confirmBtn);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('delete_contact', expect.objectContaining({
        did: 'did:key:z123',
        password: undefined
      }));
    });
  });

  it('displays "Hat eine Ablage" badge when contact has drops configured', async () => {
    const contactsWithDrops: Contact[] = [
      {
        did: 'did:key:zWithDrop',
        profile: {
          id: 'did:key:zWithDrop',
          firstName: 'Alice',
          lastName: 'DropHost',
        },
        tags: ['partner'],
        addedAt: '2024-01-01',
        syncs: [
          {
            readUrl: 'https://drop.example.com/d-test',
            writeUrl: 'https://drop.example.com/sync.php',
            dropPub: 'DropPubTestKey12345',
          },
        ],
      },
    ];

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') {
        return Promise.resolve(contactsWithDrops);
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Alice DropHost/i)).toBeInTheDocument();
    });

    expect(screen.getByTestId('contact-has-sync-badge')).toBeInTheDocument();
    expect(screen.getByText(/Hat eine Ablage|Has a Drop|Hat Sync-Dienst|Has Sync Service/i)).toBeInTheDocument();
  });

  it('triggers market sync via drops and shows feedback message', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') {
        return Promise.resolve(mockContacts);
      }
      if (cmd === 'sync_market_channels') {
        return Promise.resolve({
          skippedWifiOnly: false,
          uploadedTo: 1,
          polledSyncs: 2,
          decryptedSlots: 3,
          importedCards: 2,
          importedWotLists: 1,
          errors: [],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const syncBtn = screen.getByText(/Sync via drops|Über Ablagen synchronisieren|Über Sync-Dienste synchronisieren|Sync via sync services/i).closest('button')!;
    expect(syncBtn).toBeInTheDocument();
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('sync_market_channels', { forceUpload: true });
    });
  });

  it('adaptive trust network banner defaults to collapsed when rated contacts exist, expands on toggle', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_wot_summary') {
        return Promise.resolve({
          ratedContactsCount: 3,
          verifiedHumansCount: 1,
          closeConfidantsCount: 2,
          importedPeerListsCount: 1,
          ownListVersion: 1,
          ownListMaxHops: 3,
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    // In compact mode, expanded options should not be visible initially
    expect(screen.queryByTestId('expanded-network-options')).not.toBeInTheDocument();

    // Toggle button should be present
    const toggleBtn = screen.getByTestId('toggle-network-options-button');
    expect(toggleBtn).toBeInTheDocument();
    expect(toggleBtn).toHaveTextContent(/Netzwerk-Optionen anpassen|Customize Network Options/i);

    // Expand options
    fireEvent.click(toggleBtn);
    expect(screen.getByTestId('expanded-network-options')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /contacts\.wotHops2|2 Hops/i })).toBeInTheDocument();

    // Collapse options again
    fireEvent.click(toggleBtn);
    expect(screen.queryByTestId('expanded-network-options')).not.toBeInTheDocument();
  });

  it('adaptive trust network banner defaults to expanded when 0 contacts are rated', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_wot_summary') {
        return Promise.resolve({
          ratedContactsCount: 0,
          verifiedHumansCount: 0,
          closeConfidantsCount: 0,
          importedPeerListsCount: 0,
          ownListVersion: 1,
          ownListMaxHops: 3,
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    // When 0 contacts rated, options are expanded by default
    expect(screen.getByTestId('expanded-network-options')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /contacts\.wotHops2|2 Hops/i })).toBeInTheDocument();
  });

  it('displays sync service recommendation card when no sync service is connected and navigates to sync settings on click', async () => {
    mockNavigateToSettingsSection.mockClear();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_drop_overview_status') {
        return Promise.resolve({
          connectedCount: 0,
          friendSyncs: 0,
          publicSyncs: 0,
          hasOwnSync: false,
          ownSyncSoftWarning: false,
          lastSyncAt: null,
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByTestId('sync-service-recommendation')).toBeInTheDocument();
    });

    expect(screen.getByText(/Verbinde einen Sync-Dienst|Connect a sync service/i)).toBeInTheDocument();

    // Click connect button
    const connectBtn = screen.getByRole('button', { name: /Sync-Dienst verbinden|Connect Sync Service/i });
    fireEvent.click(connectBtn);

    expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('sync');
  });

  it('hides sync service recommendation card when a sync service is connected', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_sync_overview_status') {
        return Promise.resolve({
          connectedCount: 1,
          friendSyncs: 1,
          publicSyncs: 0,
          hasOwnSync: false,
          ownSyncSoftWarning: false,
          lastSyncAt: 12345678,
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    expect(screen.queryByTestId('sync-service-recommendation')).not.toBeInTheDocument();
  });

  it('displays configured drops with passive health status badges', async () => {
    const contactsWithHealthDrops: Contact[] = [
      {
        did: 'did:key:zHealth',
        profile: {
          id: 'did:key:zHealth',
          firstName: 'Charlie',
          lastName: 'HealthTester',
        },
        tags: [],
        addedAt: '2024-01-01',
        syncs: [
          {
            readUrl: 'https://healthy.example.com/sync.php',
            writeUrl: 'https://healthy.example.com/sync.php',
            dropPub: 'HealthyPub123',
            lastSeenAt: 1700000000,
            failCount: 0,
          },
          {
            readUrl: 'https://failing.example.com/sync.php',
            writeUrl: 'https://failing.example.com/sync.php',
            dropPub: 'FailingPub456',
            lastErrorAt: 1700000050,
            failCount: 3,
          },
        ],
      },
    ];

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(contactsWithHealthDrops);
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Charlie HealthTester/i)).toBeInTheDocument();
    });

    const badgeContainer = screen.getByTestId('contact-has-sync-badge');
    expect(badgeContainer).toBeInTheDocument();
    expect(screen.getByText(/healthy\.example\.com/i)).toBeInTheDocument();
    expect(screen.getByText(/failing\.example\.com/i)).toBeInTheDocument();
  });

  it('displays error feedback when syncMarketChannels returns HTTP 429 error', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'sync_market_channels') {
        return Promise.resolve({
          skippedWifiOnly: false,
          uploadedTo: 0,
          polledSyncs: 1,
          decryptedSlots: 0,
          importedCards: 0,
          importedWotLists: 0,
          errors: ['HTTP 429: Too Many Requests from sync.example.com'],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const syncBtn = screen.getByRole('button', { name: /contacts\.syncServicesButton|Sync via drops|Über Sync-Dienste synchronisieren|Sync via sync services/i });
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(screen.getByText(/Rate-Limit erreicht \(HTTP 429\)/i)).toBeInTheDocument();
      expect(syncBtn).toBeDisabled();
      expect(syncBtn).toHaveTextContent(/warten|wait/i);
    });
  });

  it('copies contact card with sync URLs included from settings', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:zMe12345');
      if (cmd === 'get_user_profile') return Promise.resolve({ firstName: 'Alice', lastName: 'Sender' });
      if (cmd === 'get_app_settings') {
        return Promise.resolve({
          marketSelectedSyncs: [
            {
              writeUrl: 'https://sync1.domain.org/sync.php',
              readUrl: 'https://sync1.domain.org/sync.php',
              dropPub: 'Pub1',
            },
          ],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const copyBtn = screen.getByRole('button', { name: /contacts\.copyContactCard|Copy Contact Card|Meine Kontaktkarte kopieren/i });
    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining('did%3Akey%3AzMe12345'));
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining('sync=https%3A%2F%2Fsync1.domain.org%2Fsync.php'));
    });
  });
});
