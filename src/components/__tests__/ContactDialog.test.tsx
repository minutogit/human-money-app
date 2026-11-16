import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import ContactDialog from '../ContactDialog';
import { marketService } from '../../services/marketService';
import { profileService } from '../../services/profileService';
import { Contact } from '../../types';

vi.mock('../../services/marketService', () => ({
  marketService: {
    previewContactTrustChange: vi.fn().mockResolvedValue(null),
    probePublicSync: vi.fn(),
    probeContactSyncs: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn().mockResolvedValue('did:key:zOwnRootIdentity999999'),
  },
}));

vi.mock('../../utils/log', () => ({
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
    localStorage.setItem('humoco_ui_mode', 'expert');
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
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
        existingContact={existing}
      />
    );

    expect(screen.getByDisplayValue('Alice')).toBeInTheDocument();
    expect(screen.getByText('sync1.example.com')).toBeInTheDocument();
  });

  it('probes and adds a new sync service', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    vi.mocked(marketService.probePublicSync).mockResolvedValue({
      writeUrl: 'https://new-sync.example.com/sync.php',
      readUrl: 'https://new-sync.example.com/sync.php',
      dropPub: 'NewDropPub789',
      registerUrl: null,
      accessPubkey: 'AccessKey123',
      authorizedChannels: ['post'],
    });

    render(
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
      />
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
    localStorage.setItem('humoco_ui_mode', 'expert');
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
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
        existingContact={existing}
      />
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
    localStorage.setItem('humoco_ui_mode', 'expert');
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
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
        existingContact={existing}
      />
    );

    const syncInput = screen.getByPlaceholderText(/https:\/\/sync-server\./i);
    fireEvent.change(syncInput, { target: { value: 'https://sync1.example.com/sync.php' } });

    const probeBtn = screen.getByRole('button', { name: /contacts\.syncProbeAndAdd|Prüfen & Hinzufügen|Probe & Add/i });
    fireEvent.click(probeBtn);

    expect(screen.getByText(/contacts\.syncAlreadyAdded|already.*added|bereits vorhanden/i)).toBeInTheDocument();
    expect(marketService.probePublicSync).not.toHaveBeenCalled();
  });

  it('enforces limit of max 2 sync services', () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
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
      <ContactDialog
        isOpen={true}
        onClose={mockOnClose}
        onSave={mockOnSave}
        existingContact={existing}
      />
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
});
