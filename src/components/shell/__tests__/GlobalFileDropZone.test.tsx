import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GlobalFileDropZone } from '../GlobalFileDropZone';
import { marketService } from '../../../services/marketService';
import { backupService } from '../../../services/backupService';

const mockNavigate = vi.fn();
const mockSetPendingDroppedFile = vi.fn();
const mockProtectAction = vi.fn(
  async (fn: (pwd: string | null) => Promise<unknown>) => await fn(null),
);
let mockNavView = 'wallet';
let mockIsAuthenticated = true;

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    appState: { view: mockNavView },
    navigate: mockNavigate,
    setPendingDroppedFile: mockSetPendingDroppedFile,
  }),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    isAuthenticated: mockIsAuthenticated,
    protectAction: mockProtectAction,
  }),
}));

vi.mock('../../../services/marketService', () => ({
  marketService: {
    importContactCard: vi.fn(),
  },
}));

vi.mock('../../../services/backupService', () => ({
  backupService: {
    stageDroppedBackupBytes: vi.fn(),
  },
}));

const makeDropFile = (name: string, content: string): File => {
  const bytes = new TextEncoder().encode(content);
  return {
    name,
    size: bytes.byteLength,
    arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer),
  } as unknown as File;
};

describe('GlobalFileDropZone', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockNavView = 'wallet';
    mockIsAuthenticated = true;
    mockProtectAction.mockImplementation(
      async (fn: (pwd: string | null) => Promise<unknown>) => await fn(null),
    );
  });

  it('shows localized common.fileTooLarge message when dropped file exceeds 64 MiB', async () => {
    render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement;
    expect(dropRoot).not.toBeNull();
    const oversizedFile = {
      name: 'huge.hmcbackup',
      size: 64 * 1024 * 1024 + 1,
      arrayBuffer: vi.fn(),
    } as unknown as File;

    fireEvent.drop(dropRoot as HTMLElement, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [oversizedFile],
      },
    });

    await waitFor(() => {
      expect(
        screen.getByText(/File exceeds the maximum supported size \(64 MiB\)/i)
      ).toBeInTheDocument();
    });
    expect(screen.queryByText(/^Error$/)).not.toBeInTheDocument();
  });

  it('dispatches humoco:deeplink when dropping a humoco:// URI payload', () => {
    const dispatched: string[] = [];
    const listener = (e: Event) => {
      dispatched.push((e as CustomEvent<string>).detail);
    };
    window.addEventListener('humoco:deeplink', listener);

    try {
      render(
        <GlobalFileDropZone>
          <div data-testid="drop-child">content</div>
        </GlobalFileDropZone>
      );

      const dropRoot = screen.getByTestId('drop-child').parentElement as HTMLElement;
      fireEvent.drop(dropRoot, {
        dataTransfer: {
          types: ['text/plain'],
          getData: (type: string) =>
            type === 'text/plain' ? 'humoco://pay?to=did:key:z6MkBob&amount=5' : '',
          files: [],
        },
      });

      expect(dispatched).toEqual(['humoco://pay?to=did:key:z6MkBob&amount=5']);
    } finally {
      window.removeEventListener('humoco:deeplink', listener);
    }
  });

  it('stages dropped .hmcbackup file and surfaces error when staging rejects', async () => {
    vi.mocked(backupService.stageDroppedBackupBytes).mockResolvedValueOnce('/tmp/staged.hmcbackup');
    render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement as HTMLElement;
    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('wallet.hmcbackup', '{"backup":true}')],
      },
    });

    await waitFor(() => {
      expect(backupService.stageDroppedBackupBytes).toHaveBeenCalledTimes(1);
      expect(mockNavigate).toHaveBeenCalledWith({
        view: 'restore_backup',
        initialMethod: 'file_or_folder',
        initialBackupPath: '/tmp/staged.hmcbackup',
      });
    });

    vi.mocked(backupService.stageDroppedBackupBytes).mockRejectedValueOnce(
      new Error('corrupt backup container'),
    );
    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('broken.hmcbackup', 'bad')],
      },
    });

    await waitFor(() => {
      expect(screen.getByText(/corrupt backup container/i)).toBeInTheDocument();
    });
  });

  it('queues .transfer file when logged out and routes directly to receive_bundle when logged in', async () => {
    mockIsAuthenticated = false;
    const { rerender } = render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement as HTMLElement;
    const transferJson = JSON.stringify({ c: 'TransactionBundle', payload: 'abc' });

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('pay.transfer', transferJson)],
      },
    });

    await waitFor(() => {
      expect(mockSetPendingDroppedFile).toHaveBeenCalledWith({
        name: 'pay.transfer',
        bytes: Array.from(new TextEncoder().encode(transferJson)),
      });
      expect(mockNavigate).not.toHaveBeenCalled();
    });

    mockIsAuthenticated = true;
    rerender(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('pay2.transfer', transferJson)],
      },
    });

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith({
        view: 'receive_bundle',
        droppedFile: {
          name: 'pay2.transfer',
          bytes: Array.from(new TextEncoder().encode(transferJson)),
        },
      });
    });
  });

  it('imports .hmcard via protectAction, handles rejected_offender feedback, and rejects unknown files', async () => {
    vi.mocked(marketService.importContactCard)
      .mockResolvedValueOnce({ status: 'accepted', did: 'did:key:zCardAlice', replaced: false })
      .mockResolvedValueOnce({ status: 'rejected_offender', replaced: false });

    render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement as HTMLElement;
    const cardPayload = JSON.stringify({ card_wire: 'hdr.pay.sig' });

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('alice.hmcard', cardPayload)],
      },
    });

    await waitFor(() => {
      expect(marketService.importContactCard).toHaveBeenCalledWith(cardPayload, undefined);
      expect(mockNavigate).toHaveBeenCalledWith({
        view: 'marketplace',
        selectedDid: 'did:key:zCardAlice',
      });
    });

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('evil.hmcard', cardPayload)],
      },
    });

    await waitFor(() => {
      expect(screen.getByText(/Known Offender|malicious actor|böswilliger/i)).toBeInTheDocument();
    });

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('notes.txt', 'hello world')],
      },
    });

    await waitFor(() => {
      expect(screen.getByText(/Unsupported file|Nicht unterstützte Datei/i)).toBeInTheDocument();
    });
  });

  it('ignores drops when already on receive_bundle view', () => {
    mockNavView = 'receive_bundle';
    render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement as HTMLElement;
    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [makeDropFile('wallet.hmcbackup', '{"backup":true}')],
      },
    });

    expect(backupService.stageDroppedBackupBytes).not.toHaveBeenCalled();
    expect(mockNavigate).not.toHaveBeenCalled();
  });
});
