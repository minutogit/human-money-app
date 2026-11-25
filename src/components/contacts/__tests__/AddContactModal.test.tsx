// src/components/__tests__/AddContactModal.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, Mock } from 'vitest';
import { AddContactModal } from '../AddContactModal';
import { UiModeProvider } from '../../../context/UiModeContext';
import { readFromClipboard } from '../../../utils/clipboard';
import { marketService } from '../../../services/marketService';
import { profileService } from '../../../services/profileService';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';

const mockProtectAction = vi.fn(
  async (action: (password: string | null) => Promise<unknown>) => await action(null),
);

vi.mock('../../../utils/clipboard', () => ({
  readFromClipboard: vi.fn(),
}));

vi.mock('../../../services/marketService', () => ({
  marketService: {
    importContactCard: vi.fn(),
    probeContactSyncs: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn().mockResolvedValue('did:key:z6MkTestDid2222222222222222222222222222222222222'),
  },
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: mockProtectAction,
  }),
  useOptionalSession: () => null,
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
    warn: vi.fn(),
  },
}));

describe('AddContactModal Component', () => {
  const mockOnClose = vi.fn();
  const mockOnOpenManualAdd = vi.fn();
  const mockOnContactAdded = vi.fn();
  const mockOnSuccess = vi.fn();
  const mockOnError = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    mockProtectAction.mockImplementation(
      async (action: (password: string | null) => Promise<unknown>) => await action(null),
    );
    vi.mocked(marketService.probeContactSyncs).mockResolvedValue([]);
    vi.mocked(profileService.getUserId).mockResolvedValue(
      'did:key:z6MkTestDid2222222222222222222222222222222222222',
    );
  });

  it('renders smart input and disclosure in Simple mode and hides 4 expert tiles', async () => {
    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    expect(screen.getByTestId('smart-add-contact-panel')).toBeInTheDocument();
    expect(screen.getByTestId('smart-contact-input')).toBeInTheDocument();
    expect(screen.getByTestId('smart-paste-clipboard-btn')).toBeInTheDocument();
    expect(screen.getByTestId('smart-contact-submit')).toBeInTheDocument();
    expect(screen.getByTestId('smart-add-qr-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('add-contact-tiles')).not.toBeInTheDocument();

    // Invalid payload shows inline error
    fireEvent.change(screen.getByTestId('smart-contact-input'), { target: { value: 'invalid-garbage' } });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));
    expect(await screen.findByTestId('smart-add-error')).toHaveTextContent(
      /Ungültiges Kontakt- oder Visitenkarten-Format|Invalid contact card or business card format/i
    );

    // Valid DID delegates to manual add
    fireEvent.change(screen.getByTestId('smart-contact-input'), { target: { value: 'did:key:z6MkSimp2eDid1111111111111111111111111111111' } });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));
    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalled();
      expect(mockOnOpenManualAdd).toHaveBeenCalledWith('did:key:z6MkSimp2eDid1111111111111111111111111111111');
    });
  });

  it('reveals file import and manual add under Other ways disclosure in Simple mode', () => {
    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    expect(screen.queryByTestId('add-other-manual')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('add-contact-other-ways-toggle'));

    expect(screen.getByTestId('add-other-file')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('add-other-manual'));
    expect(mockOnClose).toHaveBeenCalled();
    expect(mockOnOpenManualAdd).toHaveBeenCalled();
  });

  it('renders all 4 tiles in initial selection view (Expert mode)', () => {
    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    expect(screen.getByTestId('add-contact-tiles')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-qr')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-clipboard')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-file')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-manual')).toBeInTheDocument();
  });

  it('switches to QR scanner view when clicking QR tile', () => {
    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-qr'));

    expect(screen.getByTestId('qr-scanner-view')).toBeInTheDocument();
  });

  it('handles DID in clipboard by delegating directly to manual add with prefilled DID', async () => {
    (readFromClipboard as Mock).mockResolvedValue({ text: 'did:key:z6MkhaXGzCsKoDbKyW31111111111111111111111' });

    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalled();
      expect(mockOnOpenManualAdd).toHaveBeenCalledWith('did:key:z6MkhaXGzCsKoDbKyW31111111111111111111111');
    });
  });

  it('switches to paste input view if clipboard read fails or is empty', async () => {
    (readFromClipboard as Mock).mockResolvedValue({ text: '', error: 'denied' });

    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(screen.getByTestId('paste-input-view')).toBeInTheDocument();
    });
  });

  it('imports business card from file when clicking file tile', async () => {
    const canonicalCardJson = '{"card_wire":"header.payload.sig"}';
    (open as unknown as Mock).mockResolvedValue('/path/to/contact.hmcard');
    (readFile as unknown as Mock).mockResolvedValue(new TextEncoder().encode(canonicalCardJson));
    vi.mocked(marketService.importContactCard).mockResolvedValue({ status: 'accepted', did: 'did:key:zTest', replaced: false });

    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-file'));

    await waitFor(() => {
      expect(open).toHaveBeenCalled();
      expect(marketService.importContactCard).toHaveBeenCalledWith(canonicalCardJson, undefined);
      expect(mockOnContactAdded).toHaveBeenCalled();
      expect(mockOnSuccess).toHaveBeenCalled();
      expect(mockOnClose).toHaveBeenCalled();
    });
  });

  it('delegates to manual entry dialog when clicking manual tile', () => {
    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-manual'));

    expect(mockOnClose).toHaveBeenCalled();
    expect(mockOnOpenManualAdd).toHaveBeenCalled();
  });

  it('switches to paste input without error if clipboard has invalid/unrelated content', async () => {
    (readFromClipboard as Mock).mockResolvedValue({ text: 'https://random-website.example.org' });

    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(screen.getByTestId('paste-input-view')).toBeInTheDocument();
      expect(mockOnError).not.toHaveBeenCalled();
    });
  });

  it('shows error when submitting invalid content in paste input view', async () => {
    render(
      <UiModeProvider initialMode="expert">
        <AddContactModal
          isOpen={true}
          onClose={mockOnClose}
          onOpenManualAdd={mockOnOpenManualAdd}
          onContactAdded={mockOnContactAdded}
          onSuccess={mockOnSuccess}
          onError={mockOnError}
        />
      </UiModeProvider>
    );

    // Switch to paste view
    fireEvent.click(screen.getByTestId('add-tile-clipboard'));
    await waitFor(() => {
      expect(screen.getByTestId('paste-input-view')).toBeInTheDocument();
    });

    const textarea = screen.getByRole('textbox');
    fireEvent.change(textarea, { target: { value: 'not-a-valid-contact-or-card' } });

    const submitBtn = screen.getByRole('button', { name: /übernehmen|apply|import/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(mockOnError).toHaveBeenCalledWith(
        expect.stringMatching(/Ungültiges Kontakt- oder Visitenkarten-Format|Invalid contact card or business card format/i)
      );
    });
  });

  it('blocks importing own active DID in smart input without opening manual dialog', async () => {
    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    fireEvent.change(screen.getByTestId('smart-contact-input'), {
      target: { value: 'did:key:z6MkTestDid2222222222222222222222222222222222222' },
    });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));

    const errNode = await screen.findByTestId('smart-add-error');
    expect(errNode.textContent).toMatch(/eigenes Profil|currently active own profile/i);
    expect(mockOnError).toHaveBeenCalledWith(errNode.textContent);
    expect(mockOnOpenManualAdd).not.toHaveBeenCalled();
    expect(mockOnClose).not.toHaveBeenCalled();
  });

  it('parses humoco://contact URI with multi-word name and sync URLs and forwards them to manual add', async () => {
    const contactUri =
      'humoco://contact?did=did%3Akey%3Az6MkAnna111111111111111111111111111111111111&name=Anna+Maria+Schmidt&sync=https%3A%2F%2Fsync.example.org%2Fsync.php';

    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    fireEvent.change(screen.getByTestId('smart-contact-input'), {
      target: { value: contactUri },
    });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));

    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalledTimes(1);
      expect(mockOnOpenManualAdd).toHaveBeenCalledWith(
        'did:key:z6MkAnna111111111111111111111111111111111111',
        { firstName: 'Anna Maria', lastName: 'Schmidt' },
        ['https://sync.example.org/sync.php'],
      );
    });
  });

  it('H1: never forwards consumed hmc_grant_* one-shot token to probeContactSyncs after card import', async () => {
    const cardPayload = '{"card_wire":"header.payload.sig"}';
    mockProtectAction.mockImplementationOnce(
      async (action: (pwd: string | null) => Promise<unknown>) => await action('hmc_grant_oneshot_abc'),
    );
    vi.mocked(marketService.importContactCard).mockResolvedValueOnce({
      status: 'accepted',
      did: 'did:key:zCardImported',
      replaced: true,
    });

    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    fireEvent.change(screen.getByTestId('smart-contact-input'), {
      target: { value: cardPayload },
    });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));

    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledWith(cardPayload, 'hmc_grant_oneshot_abc');
      expect(marketService.probeContactSyncs).toHaveBeenCalledWith(false, undefined);
      expect(mockOnContactAdded).toHaveBeenCalledTimes(1);
      expect(mockOnSuccess).toHaveBeenCalledWith(
        expect.stringMatching(/newer version of this business card was successfully saved|Visitenkarte erfolgreich aktualisiert/i),
      );
      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });
  });

  it('maps non-accepted card import statuses to inline error without closing', async () => {
    const cases: Array<{ status: 'ignored_stale' | 'rejected_expired' | 'rejected_hops' | 'rejected_offender'; re: RegExp }> = [
      { status: 'ignored_stale', re: /gleicher oder neuerer Version|same or newer version/i },
      { status: 'rejected_expired', re: /abgelaufen|expired/i },
      { status: 'rejected_hops', re: /maximum allowed recommendation distance|Reichweite|hop/i },
      { status: 'rejected_offender', re: /Known Offender|malicious|Betrugsverdacht|böswilliger/i },
    ];

    render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    for (const c of cases) {
      vi.mocked(marketService.importContactCard).mockResolvedValueOnce({
        status: c.status,
        replaced: false,
      });
      fireEvent.change(screen.getByTestId('smart-contact-input'), {
        target: { value: '{"card_wire":"h.p.s"}' },
      });
      fireEvent.click(screen.getByTestId('smart-contact-submit'));

      await waitFor(() => {
        expect(screen.getByTestId('smart-add-error')).toHaveTextContent(c.re);
      });
      expect(mockOnError).toHaveBeenLastCalledWith(expect.stringMatching(c.re));
      expect(mockOnClose).not.toHaveBeenCalled();
    }
  });

  it('surfaces empty smart paste error and ignores stale async card import after closing modal', async () => {
    (readFromClipboard as Mock).mockResolvedValueOnce({ text: '   ', error: undefined });
    let resolveDeferred!: (res: { status: 'accepted'; did: string; replaced: boolean }) => void;
    vi.mocked(marketService.importContactCard).mockImplementationOnce(
      () => new Promise((resolve) => {
        resolveDeferred = resolve;
      }),
    );

    const { rerender } = render(
      <AddContactModal
        isOpen={true}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );

    fireEvent.click(screen.getByTestId('smart-paste-clipboard-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('smart-add-error')).toHaveTextContent(/Zwischenablage ist leer|Clipboard is empty/i);
    });

    fireEvent.change(screen.getByTestId('smart-contact-input'), {
      target: { value: '{"card_wire":"slow.p.s"}' },
    });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));
    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledTimes(1);
    });

    // Close modal before deferred promise resolves -> stale completion must do nothing
    rerender(
      <AddContactModal
        isOpen={false}
        onClose={mockOnClose}
        onOpenManualAdd={mockOnOpenManualAdd}
        onContactAdded={mockOnContactAdded}
        onSuccess={mockOnSuccess}
        onError={mockOnError}
      />
    );
    resolveDeferred({ status: 'accepted', did: 'did:key:zStale', replaced: false });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockOnSuccess).not.toHaveBeenCalled();
    expect(mockOnContactAdded).not.toHaveBeenCalled();
  });
});
