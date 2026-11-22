import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import AddressBook from '../AddressBook';
import { UiModeProvider } from '../../../context/UiModeContext';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { Contact } from '../../../types';
import { rateLimitManager } from '../../../sync/rateLimitManager';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../../context/SessionContext', () => {
  const protectAction = vi.fn((action) => action(undefined));
  return {
    useSession: () => ({ protectAction }),
    useOptionalSession: () => ({ protectAction }),
  };
});

const mockNavigateToSettingsSection = vi.fn();
const mockGoBack = vi.fn();
vi.mock('../../../context/NavigationContext', () => ({
  useOptionalNavigation: () => ({
    navigateToSettingsSection: mockNavigateToSettingsSection,
    goBack: mockGoBack,
  }),
}));

vi.mock('../../../utils/log', () => ({
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
    localStorage.clear();
    rateLimitManager.clearCooldown();
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
      expect(screen.getByText(/Contacts & Trust|Kontakte & Vertrauen/i)).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
      expect(screen.getByText(/Jane Smith/i)).toBeInTheDocument();
    });
  });

  it('triggers onBack when back button in header is clicked', async () => {
    mockOnBack.mockClear();
    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Contacts & Trust|Kontakte & Vertrauen/i)).toBeInTheDocument();
    });

    const backButton = screen.getByRole('button', { name: /Back|Zurück/i });
    expect(backButton).toBeInTheDocument();
    fireEvent.click(backButton);

    expect(mockOnBack).toHaveBeenCalledTimes(1);
  });

  it('falls back to navigation.goBack when onBack is omitted and back button is clicked', async () => {
    mockGoBack.mockClear();
    render(<AddressBook />);

    await waitFor(() => {
      expect(screen.getByText(/Contacts & Trust|Kontakte & Vertrauen/i)).toBeInTheDocument();
    });

    const backButton = screen.getByRole('button', { name: /Back|Zurück/i });
    expect(backButton).toBeInTheDocument();
    fireEvent.click(backButton);

    expect(mockGoBack).toHaveBeenCalledTimes(1);
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
    
    const addBtn = screen.getByRole('button', { name: /contacts\.addContact|Add Contact|Kontakt hinzufügen/i });
    fireEvent.click(addBtn);

    const otherToggle = await screen.findByTestId('add-contact-other-ways-toggle');
    fireEvent.click(otherToggle);
    const manualBtn = await screen.findByTestId('add-other-manual');
    fireEvent.click(manualBtn);
    
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

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

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
          status: 'completed',
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

    fireEvent.click(screen.getByTestId('page-header-more-actions'));
    const syncBtn = await screen.findByTestId('sync-services-button');
    expect(syncBtn).toBeInTheDocument();
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('sync_market_channels', {
        forceUpload: true,
        isWifi: true,
      });
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

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    // In compact mode, expanded options should not be visible initially
    expect(screen.queryByTestId('expanded-network-options')).not.toBeInTheDocument();

    // Toggle button should be present
    const toggleBtn = screen.getByTestId('toggle-network-options-button');
    expect(toggleBtn).toBeInTheDocument();
    expect(toggleBtn).toHaveTextContent(/Netzwerk & Reichweite|Network & Reach|Netzwerk-Optionen anpassen|Customize Network Options/i);

    // Expand options
    fireEvent.click(toggleBtn);
    expect(screen.getByTestId('expanded-network-options')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-medium')).toBeInTheDocument();

    // Collapse options again
    fireEvent.click(toggleBtn);
    expect(screen.queryByTestId('expanded-network-options')).not.toBeInTheDocument();
  });

  it('adaptive trust network banner shows slim status bar and allows opening network options modal', async () => {
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

    // Options start closed in slim status bar mode
    expect(screen.queryByTestId('expanded-network-options')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('page-header-more-actions'));
    const toggleBtn = await screen.findByTestId('toggle-network-options-button');
    fireEvent.click(toggleBtn);
    expect(screen.getByTestId('expanded-network-options')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-near')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-medium')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-far')).toBeInTheDocument();
    expect(screen.queryByTestId('trust-list-menu-button')).not.toBeInTheDocument();
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
          lastSyncAt: null,
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    // Open network options panel
    const toggleBtn = screen.getByTestId('toggle-network-options-button');
    fireEvent.click(toggleBtn);

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

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    // Open options panel
    const toggleBtn = screen.getByTestId('toggle-network-options-button');
    fireEvent.click(toggleBtn);

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

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

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
          status: 'completed',
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

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const syncBtn = screen.getByRole('button', { name: /contacts\.syncServicesButton|Sync via drops|Über Sync-Dienste synchronisieren|Sync via sync services/i });
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(screen.getByText(/Rate-Limit erreicht \(HTTP 429\)/i)).toBeInTheDocument();
      expect(syncBtn).toBeDisabled();
      expect(syncBtn).toHaveAttribute('aria-label', expect.stringMatching(/warten|wait/i));
      expect(syncBtn).toHaveTextContent(/60s/);
    });
  });

  it('displays warning feedback when syncing with no sync services configured', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_sync_overview_status') {
        return Promise.resolve({
          connectedCount: 0,
          pendingCount: 0,
          friendSyncs: 0,
          publicSyncs: 0,
          lastSyncAt: null,
        });
      }
      if (cmd === 'sync_market_channels') {
        return Promise.resolve({
          status: 'completed',
          uploadedTo: 0,
          polledSyncs: 0,
          decryptedSlots: 0,
          importedCards: 0,
          importedWotLists: 0,
          errors: [],
        });
      }
      if (cmd === 'sync_post_channels') {
        return Promise.resolve({
          uploadedTo: 0,
          polledSyncs: 0,
          importedPayments: 0,
          importedReceipts: 0,
          importedSignatures: 0,
          newSignRequests: 0,
          newWotHandshakes: 0,
          newInquiries: 0,
          newInquiryMessages: 0,
          prunedBoxes: 0,
          errors: [],
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const syncBtn = screen.getByRole('button', { name: /contacts\.syncServicesButton|Sync via drops|Über Sync-Dienste synchronisieren|Sync via sync services/i });
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(screen.getByText(/Kein Sync-Dienst eingerichtet|No sync service configured/i)).toBeInTheDocument();
      expect(screen.queryByText(/Sync-Dienste synchronisiert: 0|Sync services synchronized: 0/i)).not.toBeInTheDocument();
    });

    const connectBtn = screen.getAllByRole('button', { name: /Sync-Dienst verbinden|Connect sync service/i })[0];
    fireEvent.click(connectBtn);
    expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('sync');
  });

  it('shares business card and copies link with sync URLs included from settings', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:zMe12345');
      if (cmd === 'get_user_profile') return Promise.resolve({ firstName: 'Alice', lastName: 'Sender' });
      if (cmd === 'get_my_market_profile') {
        return Promise.resolve({
          profile: { id: 'did:key:zMe12345', firstName: 'Alice', lastName: 'Sender' },
          visibility: { firstName: 'all', lastName: 'all' },
          skills: [],
          ads: [],
          maxHops: 2,
          allowInquiries: true,
          published: true,
          shareUri: 'humoco://card?env=test&did=did%3Akey%3AzMe12345&sync=https%3A%2F%2Fsync1.domain.org%2Fsync.php',
          fileWire: '{"card_jws":"test"}',
          qrSvg: '<svg></svg>',
          qrIsPartial: false,
        });
      }
      if (cmd === 'get_my_quick_contact') {
        return Promise.resolve({
          did: 'did:key:zMe12345',
          name: 'Alice Sender',
          syncUrls: ['https://sync1.domain.org/sync.php'],
          uri: 'humoco://contact?did=did%3Akey%3AzMe12345&sync=https%3A%2F%2Fsync1.domain.org%2Fsync.php',
          qrSvg: '<svg></svg>',
        });
      }
      if (cmd === 'get_app_config') {
        return Promise.resolve({
          uiPreferences: {},
          securityPolicy: {
            marketSelectedSyncs: [
              {
                writeUrl: 'https://sync1.domain.org/sync.php',
                readUrl: 'https://sync1.domain.org/sync.php',
                dropPub: 'Pub1',
              },
            ],
          },
          lifecycle: {},
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const qrBtn = screen.getByTestId('my-quick-contact-button');
    fireEvent.click(qrBtn);

    const copyBtn = await screen.findByTestId('quick-contact-copy-button');
    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining('did%3Akey%3AzMe12345'));
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining('sync=sync1.domain.org'));
    });
  });

  it('hides sync badges and extra tags on contact cards in Simple mode and renders smart Add Contact modal', async () => {
    const multiTagContact: Contact[] = [
      {
        did: 'did:key:zSimple1',
        profile: { id: 'did:key:zSimple1', firstName: 'Clara', lastName: 'Simple' },
        tags: ['firstTag', 'secondTag'],
        addedAt: '2024-01-01',
        syncs: [{ readUrl: 'https://s.example/d', writeUrl: 'https://s.example/s', dropPub: 'PubS' }],
      },
    ];
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(multiTagContact);
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);
    await waitFor(() => {
      expect(screen.getByText(/Clara Simple/i)).toBeInTheDocument();
    });

    expect(screen.getAllByText('firstTag').some((el) => el.tagName === 'SPAN')).toBe(true);
    expect(screen.getAllByText('secondTag').some((el) => el.tagName === 'SPAN')).toBe(false);
    expect(screen.queryByTestId('contact-has-sync-badge')).not.toBeInTheDocument();

    const addBtn = screen.getByRole('button', { name: /contacts\.addContact|Add Contact|Kontakt hinzufügen/i });
    fireEvent.click(addBtn);
    expect(await screen.findByTestId('smart-add-contact-panel')).toBeInTheDocument();
    expect(screen.queryByTestId('add-contact-tiles')).not.toBeInTheDocument();
  });

  it('renders 4-tile Add Contact modal when clicking primary button (Expert mode)', async () => {
    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const addBtn = screen.getByRole('button', { name: /contacts\.addContact|Add Contact|Kontakt hinzufügen/i });
    fireEvent.click(addBtn);

    expect(await screen.findByTestId('add-contact-tiles')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-qr')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-clipboard')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-file')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-manual')).toBeInTheDocument();
  });

  it('displays 3-tier reach presets in simple mode and custom granular controls in expert mode', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_wot_summary') {
        return Promise.resolve({
          ratedContactsCount: 3,
          verifiedHumansCount: 1,
          closeConfidantsCount: 2,
          importedPeerListsCount: 1,
          ownListVersion: 1,
          ownListMaxHops: 2,
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <UiModeProvider initialMode="expert">
        <AddressBook onBack={mockOnBack} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const toggleBtn = screen.getByTestId('toggle-network-options-button');
    fireEvent.click(toggleBtn);

    expect(screen.getByTestId('expanded-network-options')).toBeInTheDocument();
    expect(screen.getByTestId('preset-or-custom')).toBeInTheDocument();
    expect(screen.getByTestId('reach-reciprocal-hint')).toBeInTheDocument();
    expect(screen.getByTestId('reach-custom-controls')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /contacts\.wotHops2|\+ deren Kontakte|\+ their contacts/i })).toBeInTheDocument();
    expect(screen.getByTestId('trust-list-menu-button')).toBeInTheDocument();
  });

  it('separates empty states: no contacts vs search without match', async () => {
    // 1. Completely empty contacts
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { unmount } = render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: /contacts\.emptyNoContactsTitle|No contacts yet|Noch keine Kontakte/i })).toBeInTheDocument();
    });
    expect(screen.getByText(/Füge die erste Person hinzu, der du vertraust|Add the first person you trust/i)).toBeInTheDocument();
    unmount();

    // 2. Contacts exist, but filter/search has no match
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/search/i);
    fireEvent.change(searchInput, { target: { value: 'NonExistentPersonXYZ' } });

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: /contacts\.emptyNoMatchesTitle|No matches|Keine Treffer/i })).toBeInTheDocument();
      expect(screen.getByText(/Keine Treffer für „NonExistentPersonXYZ“|No matches for “NonExistentPersonXYZ”/i)).toBeInTheDocument();
    });

    const resetBtn = screen.getByRole('button', { name: /contacts\.resetFilterBtn|Reset filter|Filter zurücksetzen/i });
    expect(resetBtn).toBeInTheDocument();
    fireEvent.click(resetBtn);

    await waitFor(() => {
      expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
    });
  });

  it('displays incoming handshakes with counter badge, toggle button, and human phrasing', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_post_inbox_summary') {
        return Promise.resolve({
          pendingWotHandshakes: [
            {
              msgId: 'msg-123',
              senderDid: 'did:key:zAlice999',
              senderName: 'Alice Wunder',
              proposedLevel: 2,
              replyRequested: true,
              receivedAt: 1700000000,
            },
          ],
        });
      }
      return Promise.resolve(undefined);
    });

    render(<AddressBook onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByTestId('pending-handshakes-section')).toBeInTheDocument();
    });

    expect(screen.getByText(/Incoming Trust Confirmations|Eingehende Vertrauensbestätigungen|contacts\.incomingHandshakesTitle/i)).toBeInTheDocument();
    expect(screen.getByText(/1 Kontakt möchte dich|1 contact would like to add you/i)).toBeInTheDocument();

    const toggleBtn = screen.getByTestId('toggle-handshakes-button');
    expect(toggleBtn).toHaveTextContent(/contacts\.hideHandshakesBtn|Hide|Ausblenden/i);

    expect(screen.getByText(/Alice Wunder möchte euch gegenseitig als vertraut eintragen|Alice Wunder would like to add each other as trusted contacts/i)).toBeInTheDocument();
    expect(screen.getByTestId('handshake-item-msg-123')).toBeInTheDocument();

    fireEvent.click(toggleBtn);
    expect(toggleBtn).toHaveTextContent(/contacts\.viewHandshakesBtn|View|Ansehen/i);
    expect(screen.queryByTestId('handshake-item-msg-123')).not.toBeInTheDocument();

    fireEvent.click(toggleBtn);
    await waitFor(() => {
      expect(screen.getByTestId('handshake-item-msg-123')).toBeInTheDocument();
    });
  });

  it('S10-X4: surfaces silent WoT sync imports from PoST in sync feedback count and suppresses up-to-date toast', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_sync_overview_status') {
        return Promise.resolve({
          connectedCount: 1,
          hosts: ['sync.example'],
          isBackingUp: false,
          lastBackupSuccessAt: null,
          pendingActionsCount: 0,
        });
      }
      if (cmd === 'sync_market_channels') {
        return Promise.resolve({
          uploadedTo: 0,
          polledSyncs: 1,
          importedCards: 0,
          importedWotLists: 0,
          errors: [],
        });
      }
      if (cmd === 'sync_post_channels') {
        return Promise.resolve({
          uploadedTo: 0,
          polledSyncs: 1,
          importedPayments: 0,
          importedReceipts: 0,
          importedSignatures: 0,
          newSignRequests: 0,
          newWotHandshakes: 0,
          importedWotLists: 1,
          newInquiries: 0,
          newInquiryMessages: 0,
          prunedBoxes: 0,
          errors: [],
        });
      }
      return Promise.resolve(undefined);
    });

    const toasts: Array<{ message: string; variant?: string }> = [];
    const onToast = (e: Event) => {
      toasts.push((e as CustomEvent<{ message: string; variant?: string }>).detail);
    };
    window.addEventListener('humoco:transient-sync-toast', onToast);

    try {
      render(
        <UiModeProvider initialMode="expert">
          <AddressBook onBack={mockOnBack} />
        </UiModeProvider>
      );

      await waitFor(() => {
        expect(screen.getByText(/John Doe/i)).toBeInTheDocument();
      });

      const syncBtn = screen.getByRole('button', {
        name: /contacts\.syncServicesButton|Sync via drops|Über Sync-Dienste synchronisieren|Sync via sync services/i,
      });
      fireEvent.click(syncBtn);

      await waitFor(() => {
        expect(screen.getByText(/1 trust list\(s\)|1 Vertrauensliste\(n\)/i)).toBeInTheDocument();
      });
      expect(toasts).toHaveLength(0);
    } finally {
      window.removeEventListener('humoco:transient-sync-toast', onToast);
    }
  });
});
