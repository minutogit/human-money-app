import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import ContactDialog from '../ContactDialog';
import { UiModeProvider } from '../../../context/UiModeContext';
import { contactService } from '../../../services/contactService';
import { marketService } from '../../../services/marketService';
import { profileService } from '../../../services/profileService';
import { Contact } from '../../../types';

vi.mock('../../../services/contactService', () => ({
  contactService: {
    getContacts: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../services/marketService', () => ({
  marketService: {
    previewContactTrustChange: vi.fn().mockResolvedValue(null),
    probePublicSync: vi.fn(),
    probeContactSyncs: vi.fn().mockResolvedValue([]),
    resolveCardRecipient: vi.fn(),
  },
}));

vi.mock('../../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn().mockResolvedValue('did:key:zOwnRootIdentity999999'),
  },
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
    warn: vi.fn(),
  },
}));

describe('ContactDialog Component', () => {
  const mockOnClose = vi.fn();
  const mockOnSave = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(contactService.getContacts).mockResolvedValue([]);
  });

  it('hides sync endpoints and edit-mode DID input in Simple mode while keeping new-contact DID input visible', () => {
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: {
        id: 'did:key:z12345',
        firstName: 'Alice',
        lastName: 'Tester',
      },
      tags: ['friends'],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
        },
      ],
    };

    const { rerender } = render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    expect(screen.getByTestId('contact-did-input')).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/https:\/\/sync-server\./i)).not.toBeInTheDocument();

    rerender(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
        existingContact={existing}
      />
    );

    expect(screen.queryByTestId('contact-did-input')).not.toBeInTheDocument();
    expect(screen.queryByText('sync1.example.com')).not.toBeInTheDocument();
  });

  it('renders existing contact with drops in Expert mode', () => {
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: {
        id: 'did:key:z12345',
        firstName: 'Alice',
        lastName: 'Tester',
      },
      tags: ['friends'],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
          lastSeenAt: 1700000000,
          failCount: 0,
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
          existingContact={existing}
        />
      </UiModeProvider>
    );

    expect(screen.getByDisplayValue('Alice')).toBeInTheDocument();
    expect(screen.getByText('sync1.example.com')).toBeInTheDocument();
  });

  it('probes and adds a new sync service', async () => {
    vi.mocked(marketService.probePublicSync).mockResolvedValue({
      writeUrl: 'https://new-sync.example.com/sync.php',
      readUrl: 'https://new-sync.example.com/sync.php',
      dropPub: 'NewDropPub789',
      registerUrl: null,
      accessPubkey: 'AccessKey123',
      authorizedChannels: ['post'],
    });

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
        />
      </UiModeProvider>
    );

    const didInput = screen.getByPlaceholderText(/did:key:z\.\.\.|Enter User ID|Benutzer-ID/i);
    fireEvent.change(didInput, { target: { value: 'did:key:zNewContact' } });

    const syncInput = screen.getByPlaceholderText(/https:\/\/sync-server\./i);
    fireEvent.change(syncInput, { target: { value: 'https://new-sync.example.com/sync.php' } });

    const probeBtn = screen.getByRole('button', { name: /contacts\.syncProbeAndAdd|Prüfen & Hinzufügen|Probe & Add/i });
    fireEvent.click(probeBtn);

    await waitFor(() => {
      expect(marketService.probePublicSync).toHaveBeenCalledWith('https://new-sync.example.com/sync.php');
      expect(screen.getByText('new-sync.example.com')).toBeInTheDocument();
    });

    // Save and verify drops are passed to onSave
    const saveBtn = screen.getByRole('button', { name: /contacts\.saveContact|Save Contact|Kontakt speichern/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(mockOnSave).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'did:key:zNewContact',
          syncs: [
            expect.objectContaining({
              writeUrl: 'https://new-sync.example.com/sync.php',
              dropPub: 'NewDropPub789',
            }),
          ],
        })
      );
    });
  });

  it('confirms and deletes a configured sync service', async () => {
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: {
        id: 'did:key:z12345',
        firstName: 'Alice',
      },
      tags: [],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
          existingContact={existing}
        />
      </UiModeProvider>
    );

    expect(screen.getByText('sync1.example.com')).toBeInTheDocument();

    // Click trash button
    const deleteBtn = screen.getByTitle(/contacts\.syncDelete|Remove|Entfernen/i);
    fireEvent.click(deleteBtn);

    // Confirmation appears
    expect(screen.getByText(/contacts\.syncDeleteConfirm|Really remove sync service\?|Wirklich entfernen\?/i)).toBeInTheDocument();

    // Confirm deletion
    const confirmBtn = screen.getByRole('button', { name: /contacts\.syncDeleteConfirmBtn|Yes|Ja/i });
    fireEvent.click(confirmBtn);

    // Drop should be gone
    expect(screen.queryByText('sync1.example.com')).not.toBeInTheDocument();

    // Save should now submit drops as []
    const saveBtn = screen.getByRole('button', { name: /contacts\.updateContact|Update Contact|Kontakt aktualisieren/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(mockOnSave).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'did:key:z12345',
          syncs: [],
        })
      );
    });
  });

  it('rejects adding duplicate sync service', async () => {
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: { id: 'did:key:z12345' },
      tags: [],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
          existingContact={existing}
        />
      </UiModeProvider>
    );

    const syncInput = screen.getByPlaceholderText(/https:\/\/sync-server\./i);
    fireEvent.change(syncInput, { target: { value: 'https://sync1.example.com/sync.php' } });

    const probeBtn = screen.getByRole('button', { name: /contacts\.syncProbeAndAdd|Prüfen & Hinzufügen|Probe & Add/i });
    fireEvent.click(probeBtn);

    expect(screen.getByText(/contacts\.syncAlreadyAdded|already.*added|bereits vorhanden/i)).toBeInTheDocument();
    expect(marketService.probePublicSync).not.toHaveBeenCalled();
  });

  it('enforces limit of max 2 sync services', () => {
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: { id: 'did:key:z12345' },
      tags: [],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
        },
        {
          readUrl: 'https://sync2.example.com/sync.php',
          writeUrl: 'https://sync2.example.com/sync.php',
          dropPub: 'Pub2',
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
          existingContact={existing}
        />
      </UiModeProvider>
    );

    expect(screen.getByText('sync1.example.com')).toBeInTheDocument();
    expect(screen.getByText('sync2.example.com')).toBeInTheDocument();

    // Input field should not be present; limit message shown
    expect(screen.getByText(/contacts\.syncMaxReached|Maximal 2 Sync-Ablagen erlaubt|Maximum 2 sync services allowed/i)).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/https:\/\/sync-server\./i)).not.toBeInTheDocument();
  });

  it('blocks saving exact own active profile and hides trust rating for own subaccount', async () => {
    vi.mocked(profileService.getUserId).mockResolvedValue('did:key:zOwnRootIdentity999999');

    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    const didInput = screen.getByTestId('contact-did-input');
    const saveBtn = screen.getByRole('button', { name: /contacts\.saveContact|Kontakt speichern|Save Contact/i });

    // 1. Exact own active profile -> warning shown, save disabled
    fireEvent.change(didInput, { target: { value: 'did:key:zOwnRootIdentity999999' } });
    await waitFor(() => {
      expect(screen.getByTestId('self-active-profile-warning')).toBeInTheDocument();
    });
    expect(saveBtn).toBeDisabled();

    // 2. Own subaccount (different prefix, same root) -> trust rating hidden, save allowed with trustLevel: null
    fireEvent.change(didInput, { target: { value: 'laden:abc@did:key:zOwnRootIdentity999999' } });
    await waitFor(() => {
      expect(screen.getByTestId('own-subaccount-trust-notice')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('self-active-profile-warning')).not.toBeInTheDocument();
    expect(marketService.previewContactTrustChange).not.toHaveBeenCalled();
    expect(saveBtn).not.toBeDisabled();

    fireEvent.click(saveBtn);
    await waitFor(() => {
      expect(mockOnSave).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'laden:abc@did:key:zOwnRootIdentity999999',
          trustLevel: null,
        })
      );
    });
  });

  it('normalizes quoted and mixed-case DIDs without whitespace in ContactDialog input', () => {
    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    const didInput = screen.getByTestId('contact-did-input');
    fireEvent.change(didInput, {
      target: { value: '„ALICE:3K4@DID:KEY:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v“' },
    });
    expect(didInput).toHaveValue('alice:3K4@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v');
  });

  it('detects sibling prefix, shows shared-trust notice, and prefills trustLevel on save', async () => {
    const sibling: Contact = {
      did: 'laden:abc@did:key:zSiblingKey999',
      profile: {
        id: 'laden:abc@did:key:zSiblingKey999',
        firstName: 'Lisa',
        lastName: 'Laden',
      },
      tags: [],
      trustLevel: 2,
      addedAt: '2026-01-01',
      syncs: [],
    };
    vi.mocked(contactService.getContacts).mockResolvedValue([sibling]);

    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    const didInput = screen.getByTestId('contact-did-input');
    fireEvent.change(didInput, { target: { value: 'privat:def@did:key:zSiblingKey999' } });

    await waitFor(() => {
      expect(screen.getByTestId('sibling-prefix-trust-notice')).toHaveTextContent('Lisa Laden');
    });

    const saveBtn = screen.getByRole('button', { name: /contacts\.saveContact|Kontakt speichern|Save Contact/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(mockOnSave).toHaveBeenCalledWith(
        expect.objectContaining({
          did: 'privat:def@did:key:zSiblingKey999',
          trustLevel: 2,
        })
      );
    });
  });

  it('H12: disables Save while resolving card URI and discards stale resolve result after newer edit', async () => {
    let resolveCard!: (val: { recipientId: string; did: string; displayName?: string }) => void;
    vi.mocked(marketService.resolveCardRecipient).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCard = resolve;
        }),
    );

    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    const didInput = screen.getByTestId('contact-did-input') as HTMLInputElement;
    const firstNameInput = screen.getByTestId('contact-firstname-input') as HTMLInputElement;
    const saveBtn = screen.getByRole('button', { name: /contacts\.saveContact|Kontakt speichern|Save Contact/i });

    fireEvent.change(didInput, {
      target: { value: 'humoco://card?env=eyJ0ZXN0IjoxfQ' },
    });

    await waitFor(() => {
      expect(marketService.resolveCardRecipient).toHaveBeenCalledTimes(1);
    });
    expect(saveBtn).toBeDisabled();

    // User overwrites with a fresh valid DID before the async card lookup finishes
    fireEvent.change(didInput, {
      target: { value: 'did:key:z6MkNewerDirectDid111' },
    });
    expect(saveBtn).not.toBeDisabled();

    // Stale resolution finishes afterwards -> must be completely ignored
    resolveCard({
      recipientId: 'did:key:z6MkStaleCardDid999',
      did: 'did:key:z6MkStaleCardDid999',
      displayName: 'Stale Name',
    });

    await waitFor(() => {
      expect(didInput.value).toBe('did:key:z6MkNewerDirectDid111');
      expect(firstNameInput.value).toBe('');
    });
  });

  it('returns null when isOpen is false', () => {
    const { container } = render(
      <ContactDialog
        isOpen={false}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it('surfaces error when probing sync endpoint fails and keeps list intact on cancel delete', async () => {
    vi.mocked(marketService.probePublicSync).mockRejectedValueOnce(new Error('network timeout'));
    const existing: Contact = {
      did: 'did:key:z12345',
      profile: { id: 'did:key:z12345', firstName: 'Alice' },
      tags: [],
      addedAt: '2024-01-01',
      syncs: [
        {
          readUrl: 'https://sync1.example.com/sync.php',
          writeUrl: 'https://sync1.example.com/sync.php',
          dropPub: 'Pub1',
        },
      ],
    };

    render(
      <UiModeProvider initialMode="expert">
        <ContactDialog
          isOpen={true}
          onClose={mockOnClose}
          onSave={mockOnSave}
          existingContact={existing}
        />
      </UiModeProvider>
    );

    const syncInput = screen.getByPlaceholderText(/https:\/\/sync-server\./i);
    fireEvent.change(syncInput, { target: { value: 'https://broken.example.com/sync.php' } });
    const probeBtn = screen.getByRole('button', { name: /contacts\.syncProbeAndAdd|Prüfen & Hinzufügen|Probe & Add/i });
    fireEvent.click(probeBtn);

    await waitFor(() => {
      expect(screen.getByText(/Could not reach sync service|Sync-Dienst konnte nicht erreicht werden/i)).toBeInTheDocument();
    });
    expect(screen.queryByText('broken.example.com')).not.toBeInTheDocument();

    const deleteBtn = screen.getByTitle(/contacts\.syncDelete|Remove|Entfernen/i);
    fireEvent.click(deleteBtn);
    const cancelBtn = screen.getByRole('button', { name: /contacts\.syncDeleteCancelBtn|^No$|^Nein$/i });
    fireEvent.click(cancelBtn);
    expect(screen.getByText('sync1.example.com')).toBeInTheDocument();
  });

  it('keeps dialog open and shows error when onSave rejects', async () => {
    mockOnSave.mockRejectedValueOnce(new Error('save boom'));

    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
    );

    fireEvent.change(screen.getByTestId('contact-did-input'), {
      target: { value: 'did:key:z6MkSaveFail111' },
    });
    const saveBtn = screen.getByRole('button', { name: /contacts\.saveContact|Kontakt speichern|Save Contact/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(mockOnSave).toHaveBeenCalledTimes(1);
      expect(screen.getByText(/save boom/)).toBeInTheDocument();
    });
    expect(mockOnClose).not.toHaveBeenCalled();
  });
});
