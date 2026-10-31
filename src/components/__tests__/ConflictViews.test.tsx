import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ConflictDetailsView } from '../ConflictDetailsView';
import { ConflictListView } from '../ConflictListView';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';

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
}));

describe('Conflict Views (.hmcproof export and import)', () => {
  const mockProofId = 'proof-1234567890';
  const mockOnBack = vi.fn();
  const mockOnViewConflict = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
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
  });

  describe('ConflictListView', () => {
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
