import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ConflictDetailsView } from '../ConflictDetailsView';
import { ConflictListView } from '../ConflictListView';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';
import { UiModeProvider, UI_MODE_STORAGE_KEY } from '../../context/UiModeContext';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
  writeFile: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
  useOptionalSession: () => null,
}));

describe('Conflict Views (.hmcproof export and import)', () => {
  const mockProofId = 'proof-1234567890';
  const mockOnBack = vi.fn();
  const mockOnViewConflict = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.removeItem(UI_MODE_STORAGE_KEY);
  });

  describe('ConflictDetailsView', () => {
    it('exports double spend proof as .hmcproof', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isResolved: false,
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
          });
        }
        if (cmd === 'get_contacts') {
          return Promise.resolve([]);
        }
        if (cmd === 'export_double_spend_proof') {
          return Promise.resolve([10, 20, 30, 40]);
        }
        return Promise.resolve(undefined);
      });

      (save as Mock).mockResolvedValue('/tmp/proof.hmcproof');

      render(<ConflictDetailsView proofId={mockProofId} onBack={mockOnBack} />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Save Fraud Proof \(\.hmcproof\)/i })).toBeInTheDocument();
      });

      const exportBtn = screen.getByRole('button', { name: /Save Fraud Proof \(\.hmcproof\)/i });
      fireEvent.click(exportBtn);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('export_double_spend_proof', { proofId: mockProofId });
        expect(writeFile).toHaveBeenCalledWith('/tmp/proof.hmcproof', expect.any(Uint8Array));
        expect(screen.getByText('/tmp/proof.hmcproof')).toBeInTheDocument();
      });
    });

    it('renders 3-question guide and hides SST, evidence, proofId, and override in simple mode', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:z6MkvOffenderSecretKey123456',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [
                { tId: 'tx-1', tTime: '2024-01-01T00:00:00Z', amount: '10', recipientId: 'did:key:r1' },
                { tId: 'tx-2', tTime: '2024-01-01T00:01:00Z', amount: '10', recipientId: 'did:key:r2' },
              ],
              isResolved: false,
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
          });
        }
        if (cmd === 'get_contacts') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { container } = render(
        <UiModeProvider>
          <ConflictDetailsView proofId={mockProofId} onBack={mockOnBack} />
        </UiModeProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('conflict-three-questions')).toBeInTheDocument();
      });

      expect(screen.getByText('What happened?')).toBeInTheDocument();
      expect(screen.getByText('Is my voucher still valid?')).toBeInTheDocument();
      expect(screen.getByText('What should I do?')).toBeInTheDocument();
      expect(container.textContent).not.toContain('did:key:z6MkvOffender');
      expect(screen.queryByTestId('conflict-evidence-section')).not.toBeInTheDocument();
      expect(screen.queryByTestId('conflict-local-override-section')).not.toBeInTheDocument();
      expect(screen.queryByText(/SST V4/i)).not.toBeInTheDocument();
      expect(screen.queryByText(mockProofId)).not.toBeInTheDocument();
    });

    it('reveals SST attribution, evidence portfolio, and local override in expert mode', async () => {
      window.localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:z6MkvOffenderSecretKey123456',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [
                { tId: 'tx-1', tTime: '2024-01-01T00:00:00Z', amount: '10', recipientId: 'did:key:r1' },
                { tId: 'tx-2', tTime: '2024-01-01T00:01:00Z', amount: '10', recipientId: 'did:key:r2' },
              ],
              isResolved: false,
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
          });
        }
        if (cmd === 'get_contacts') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      render(
        <UiModeProvider>
          <ConflictDetailsView proofId={mockProofId} onBack={mockOnBack} />
        </UiModeProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('conflict-evidence-section')).toBeInTheDocument();
      });

      expect(screen.getByTestId('conflict-local-override-section')).toBeInTheDocument();
      expect(screen.getByText(/SST V4 Cryptographically Proven Identity/i)).toBeInTheDocument();
      expect(screen.getByText(mockProofId)).toBeInTheDocument();
    });
  });

  describe('ConflictListView', () => {
    it('shows calm 1-line summary and hides proofId/forkPoint in simple mode', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_double_spend_conflicts') {
          return Promise.resolve([
            {
              proofId: 'proof-secret-id-999',
              offenderId: 'did:key:z6MkOffenderList123456',
              forkPointPrevHash: 'hash-fork-point-888',
              affectedVoucherName: 'Minuto Sample',
              reportTimestamp: '2024-01-01T00:00:00Z',
              conflictRole: 'victim',
              isResolved: false,
              localOverride: false,
              hasL2Verdict: true,
            },
          ]);
        }
        return Promise.resolve(undefined);
      });

      const { container } = render(
        <UiModeProvider>
          <ConflictListView onBack={mockOnBack} onViewConflict={mockOnViewConflict} />
        </UiModeProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('conflict-card-summary')).toBeInTheDocument();
      });

      expect(screen.getByTestId('conflict-card-summary')).toHaveTextContent(
        'A received voucher was spent more than once.'
      );
      expect(container.textContent).not.toContain('did:key:z6MkOffender');
      expect(container.textContent).not.toContain('proof-secret-id-999');
      expect(container.textContent).not.toContain('hash-fork-point-888');
    });
    it('imports double spend proof from .hmcproof file and navigates to conflict details', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_double_spend_conflicts') {
          return Promise.resolve([]);
        }
        if (cmd === 'import_double_spend_proof') {
          return Promise.resolve('imported-proof-999');
        }
        return Promise.resolve(undefined);
      });

      (open as Mock).mockResolvedValue('/tmp/incoming.hmcproof');
      (readFile as Mock).mockResolvedValue(new Uint8Array([5, 6, 7, 8]));

      render(<ConflictListView onBack={mockOnBack} onViewConflict={mockOnViewConflict} />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Import Fraud Proof \(\.hmcproof\)/i })).toBeInTheDocument();
      });

      const importBtn = screen.getByRole('button', { name: /Import Fraud Proof \(\.hmcproof\)/i });
      fireEvent.click(importBtn);

      await waitFor(() => {
        expect(open).toHaveBeenCalled();
        expect(invoke).toHaveBeenCalledWith('import_double_spend_proof', {
          proofBytes: [5, 6, 7, 8],
          password: 'test-password',
        });
        expect(mockOnViewConflict).toHaveBeenCalledWith('imported-proof-999');
      });
    });
  });
});
