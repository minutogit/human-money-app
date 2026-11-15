// src/components/__tests__/AddContactModal.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, Mock } from 'vitest';
import { AddContactModal } from '../AddContactModal';
import { readFromClipboard } from '../../utils/clipboard';
import { marketService } from '../../services/marketService';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';

vi.mock('../../utils/clipboard', () => ({
  readFromClipboard: vi.fn(),
}));

vi.mock('../../services/marketService', () => ({
  marketService: {
    importContactCard: vi.fn(),
    probeContactSyncs: vi.fn().mockResolvedValue([]),
  },
}));

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

describe('AddContactModal Component', () => {
  const mockOnClose = vi.fn();
  const mockOnOpenManualAdd = vi.fn();
  const mockOnContactAdded = vi.fn();
  const mockOnSuccess = vi.fn();
  const mockOnError = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
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
    fireEvent.change(screen.getByTestId('smart-contact-input'), { target: { value: 'did:key:z6MkSimpleDid' } });
    fireEvent.click(screen.getByTestId('smart-contact-submit'));
    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalled();
      expect(mockOnOpenManualAdd).toHaveBeenCalledWith('did:key:z6MkSimpleDid');
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
    localStorage.setItem('humoco_ui_mode', 'expert');
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

    expect(screen.getByTestId('add-contact-tiles')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-qr')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-clipboard')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-file')).toBeInTheDocument();
    expect(screen.getByTestId('add-tile-manual')).toBeInTheDocument();
  });

  it('switches to QR scanner view when clicking QR tile', () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
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

    fireEvent.click(screen.getByTestId('add-tile-qr'));

    expect(screen.getByTestId('qr-scanner-view')).toBeInTheDocument();
  });

  it('handles DID in clipboard by delegating directly to manual add with prefilled DID', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    (readFromClipboard as Mock).mockResolvedValue({ text: 'did:key:z6MkhaXGzCsKoDbKyW3' });

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

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalled();
      expect(mockOnOpenManualAdd).toHaveBeenCalledWith('did:key:z6MkhaXGzCsKoDbKyW3');
    });
  });

  it('switches to paste input view if clipboard read fails or is empty', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    (readFromClipboard as Mock).mockResolvedValue({ text: '', error: 'denied' });

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

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(screen.getByTestId('paste-input-view')).toBeInTheDocument();
    });
  });

  it('imports business card from file when clicking file tile', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    (open as unknown as Mock).mockResolvedValue('/path/to/contact.hmcard');
    (readFile as unknown as Mock).mockResolvedValue(new TextEncoder().encode('{"envelope":"test"}'));
    vi.mocked(marketService.importContactCard).mockResolvedValue({ status: 'accepted', did: 'did:key:zTest', replaced: false });

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

    fireEvent.click(screen.getByTestId('add-tile-file'));

    await waitFor(() => {
      expect(open).toHaveBeenCalled();
      expect(marketService.importContactCard).toHaveBeenCalledWith('{"envelope":"test"}', undefined);
      expect(mockOnContactAdded).toHaveBeenCalled();
      expect(mockOnSuccess).toHaveBeenCalled();
      expect(mockOnClose).toHaveBeenCalled();
    });
  });

  it('delegates to manual entry dialog when clicking manual tile', () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
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

    fireEvent.click(screen.getByTestId('add-tile-manual'));

    expect(mockOnClose).toHaveBeenCalled();
    expect(mockOnOpenManualAdd).toHaveBeenCalled();
  });

  it('switches to paste input without error if clipboard has invalid/unrelated content', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    (readFromClipboard as Mock).mockResolvedValue({ text: 'https://random-website.example.org' });

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

    fireEvent.click(screen.getByTestId('add-tile-clipboard'));

    await waitFor(() => {
      expect(screen.getByTestId('paste-input-view')).toBeInTheDocument();
      expect(mockOnError).not.toHaveBeenCalled();
    });
  });

  it('shows error when submitting invalid content in paste input view', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
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
});
