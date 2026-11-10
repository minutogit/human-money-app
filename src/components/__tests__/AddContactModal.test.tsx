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
  });

  it('renders all 4 tiles in initial selection view', () => {
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
});
