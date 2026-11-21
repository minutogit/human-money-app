import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ConflictDetailsView } from '../ConflictDetailsView';
import { ConflictListView } from '../ConflictListView';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';
import { UiModeProvider } from '../../../context/UiModeContext';

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

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

const { mockProtectAction } = vi.hoisted(() => ({
  mockProtectAction: vi.fn((callback: (pwd?: string) => unknown, _options?: { requireFullAccess?: boolean }) =>
    callback('test-password')
  ),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: mockProtectAction,
  }),
  useOptionalSession: () => null,
}));

describe('Conflict Views (.hmcproof export and import)', () => {
  const mockProofId = 'proof-1234567890';
  const mockOnBack = vi.fn();
  const mockOnViewConflict = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockProtectAction.mockImplementation((callback: (pwd?: string) => unknown) =>
      callback('test-password')
    );
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
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: false,
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

    it('renders 3-question guide and endorsements/override while hiding SST, evidence, and proofId in simple mode', async () => {
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
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: false,
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
      expect(screen.getByTestId('conflict-endorsements-section')).toBeInTheDocument();
      expect(screen.getByTestId('conflict-local-override-section')).toBeInTheDocument();
      expect(container.textContent).not.toContain('did:key:z6MkvOffender');
      expect(screen.queryByTestId('conflict-evidence-section')).not.toBeInTheDocument();
      expect(screen.queryByText(/SST V4/i)).not.toBeInTheDocument();
      expect(screen.queryByText(mockProofId)).not.toBeInTheDocument();
    });

    it('reveals SST attribution, evidence portfolio, and proofId in expert mode', async () => {
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
              isCryptographicallyAttributed: true,
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: false,
          });
        }
        if (cmd === 'get_contacts') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      render(
        <UiModeProvider initialMode="expert">
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

    it('renders WoT-enriched endorsements and suppresses 0-trusted warning when trusted contact endorsed', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isCryptographicallyAttributed: true,
              resolutions: [
                {
                  endorsementId: 'end-alice',
                  proofId: mockProofId,
                  endorserId: 'did:key:z6MkAliceOwn',
                  resolutionTimestamp: '2024-01-03T00:00:00Z',
                  notes: 'Own record',
                  endorserSignature: 'sig-alice',
                  endorserName: 'Alice',
                  isOwnIdentity: true,
                  wotTrust: {
                    score: 1,
                    trustPercent: 100,
                    isOwnVoucher: true,
                    blockedByOffender: false,
                    shortestHops: 0,
                    primaryBridgeDid: null,
                    primaryBridgeName: null,
                    additionalPathsCount: 0,
                    viaGuarantor: false,
                    bestPathNames: [],
                  },
                },
                {
                  endorsementId: 'end-bob',
                  proofId: mockProofId,
                  endorserId: 'did:key:z6MkBobTrusted',
                  resolutionTimestamp: '2024-01-02T00:00:00Z',
                  notes: 'Settled in cash',
                  endorserSignature: 'sig-bob',
                  endorserName: 'Bob',
                  isOwnIdentity: false,
                  wotTrust: {
                    score: 0.85,
                    trustPercent: 85,
                    isOwnVoucher: false,
                    blockedByOffender: false,
                    shortestHops: 1,
                    primaryBridgeDid: null,
                    primaryBridgeName: null,
                    additionalPathsCount: 0,
                    viaGuarantor: false,
                    bestPathNames: ['Bob'],
                  },
                },
                {
                  endorsementId: 'end-unknown',
                  proofId: mockProofId,
                  endorserId: 'did:key:z6MkUnknownSybil',
                  resolutionTimestamp: '2024-01-01T12:00:00Z',
                  notes: 'Sybil claim',
                  endorserSignature: 'sig-unknown',
                  endorserName: null,
                  isOwnIdentity: false,
                  wotTrust: {
                    score: 0,
                    trustPercent: 0,
                    isOwnVoucher: false,
                    blockedByOffender: false,
                    shortestHops: null,
                    primaryBridgeDid: null,
                    primaryBridgeName: null,
                    additionalPathsCount: 0,
                    viaGuarantor: false,
                    bestPathNames: [],
                  },
                },
              ],
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: true,
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
        expect(screen.getByText('Bob')).toBeInTheDocument();
      });

      expect(screen.getByText('You (Alice)')).toBeInTheDocument();
      expect(screen.getByText('Your confirmation')).toBeInTheDocument();
      expect(screen.getByText(/85%/)).toBeInTheDocument();
      expect(screen.getByText('Unknown identity (no trust path)')).toBeInTheDocument();

      const warningBanner = screen.getByTestId('conflict-local-override-warning');
      expect(warningBanner).not.toHaveTextContent(
        'Warning: None of your trusted Web-of-Trust contacts have signed a resolution note for this conflict yet.'
      );
      expect(warningBanner).toHaveTextContent(
        'This decision applies strictly locally on this device and never reactivates quarantined vouchers.'
      );
    });

    it('shows strong 0-trusted warning when only unknown identities have endorsed', async () => {
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isCryptographicallyAttributed: true,
              resolutions: [
                {
                  endorsementId: 'end-unknown',
                  proofId: mockProofId,
                  endorserId: 'did:key:z6MkUnknownSybil',
                  resolutionTimestamp: '2024-01-01T12:00:00Z',
                  notes: 'Trust me bro',
                  endorserSignature: 'sig-unknown',
                  endorserName: null,
                  isOwnIdentity: false,
                  wotTrust: {
                    score: 0,
                    trustPercent: 0,
                    isOwnVoucher: false,
                    blockedByOffender: false,
                    shortestHops: null,
                    primaryBridgeDid: null,
                    primaryBridgeName: null,
                    additionalPathsCount: 0,
                    viaGuarantor: false,
                    bestPathNames: [],
                  },
                },
              ],
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: true,
            hasOwnEndorsement: false,
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
        expect(screen.getByTestId('conflict-local-override-warning')).toBeInTheDocument();
      });

      const warningBanner = screen.getByTestId('conflict-local-override-warning');
      expect(warningBanner).toHaveTextContent(
        'Warning: None of your trusted Web-of-Trust contacts have signed a resolution note for this conflict yet.'
      );
      expect(screen.getByText('Unknown identity (no trust path)')).toBeInTheDocument();
    });

    it('signs resolution endorsement with full access and default local override', async () => {
      const updatedDetails = {
        proof: {
          proofId: mockProofId,
          offenderId: 'did:key:offender',
          reportTimestamp: '2024-01-01T00:00:00Z',
          affectedVoucherName: 'Minuto Test',
          conflictingTransactions: [],
          isCryptographicallyAttributed: true,
          resolutions: [
            {
              endorsementId: 'end-own-1',
              proofId: mockProofId,
              endorserId: 'did:key:z6MkAliceOwn',
              resolutionTimestamp: '2024-01-02T00:00:00Z',
              notes: 'Paid back in cash',
              endorserSignature: 'sig-own',
              endorserName: 'Alice',
              isOwnIdentity: true,
              wotTrust: {
                score: 1,
                trustPercent: 100,
                isOwnVoucher: true,
                blockedByOffender: false,
                shortestHops: 0,
                primaryBridgeDid: null,
                primaryBridgeName: null,
                additionalPathsCount: 0,
                viaGuarantor: false,
                bestPathNames: [],
              },
            },
          ],
        },
        localOverride: true,
        conflictRole: 'victim',
        canEndorse: false,
        hasOwnEndorsement: true,
      };

      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isCryptographicallyAttributed: true,
              resolutions: [],
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: true,
            hasOwnEndorsement: false,
          });
        }
        if (cmd === 'endorse_conflict_resolution') {
          return Promise.resolve(updatedDetails);
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
        expect(screen.getByRole('button', { name: /Confirm & Sign Resolution/i })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole('button', { name: /Confirm & Sign Resolution/i }));

      const checkbox = screen.getByRole('checkbox');
      expect(checkbox).toBeChecked();

      const noteInput = screen.getByPlaceholderText(/Optional note/i);
      fireEvent.change(noteInput, { target: { value: 'Paid back in cash' } });

      fireEvent.click(screen.getByRole('button', { name: /Sign & Attach Confirmation/i }));

      await waitFor(() => {
        expect(mockProtectAction).toHaveBeenCalledWith(expect.any(Function), { requireFullAccess: true });
        expect(invoke).toHaveBeenCalledWith('endorse_conflict_resolution', {
          proofId: mockProofId,
          notes: 'Paid back in cash',
          alsoSetLocalOverride: true,
          password: 'test-password',
        });
        expect(screen.getByText('Resolution Confirmation Signed')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /Save Updated Proof \(\.hmcproof\)/i })).toBeInTheDocument();
      });
    });

    it('enables and revokes manual local override with full access protection', async () => {
      let currentOverride = false;
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'get_proof_of_double_spend') {
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isCryptographicallyAttributed: true,
              resolutions: [],
            },
            localOverride: currentOverride,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: false,
          });
        }
        if (cmd === 'set_conflict_local_override') {
          currentOverride = Boolean(args?.value);
          return Promise.resolve(undefined);
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
        expect(screen.getByRole('button', { name: /Mark Conflict as Settled Locally/i })).toBeInTheDocument();
      });

      fireEvent.change(screen.getByPlaceholderText(/Describe how this dispute was settled/i), {
        target: { value: 'Direct settlement' },
      });
      fireEvent.click(screen.getByRole('button', { name: /Mark Conflict as Settled Locally/i }));

      await waitFor(() => {
        expect(mockProtectAction).toHaveBeenCalledWith(expect.any(Function), { requireFullAccess: true });
        expect(invoke).toHaveBeenCalledWith('set_conflict_local_override', {
          proofId: mockProofId,
          value: true,
          note: 'Direct settlement',
          password: 'test-password',
        });
        expect(screen.getByRole('button', { name: /Revoke Local Unblock/i })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole('button', { name: /Revoke Local Unblock/i }));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('set_conflict_local_override', {
          proofId: mockProofId,
          value: false,
          note: undefined,
          password: 'test-password',
        });
        expect(screen.getByRole('button', { name: /Mark Conflict as Settled Locally/i })).toBeInTheDocument();
      });
    });

    it('imports updated .hmcproof directly inside ConflictDetailsView and merges new endorsements', async () => {
      let fetchCount = 0;
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'get_proof_of_double_spend') {
          fetchCount += 1;
          return Promise.resolve({
            proof: {
              proofId: mockProofId,
              offenderId: 'did:key:offender',
              reportTimestamp: '2024-01-01T00:00:00Z',
              affectedVoucherName: 'Minuto Test',
              conflictingTransactions: [],
              isCryptographicallyAttributed: true,
              resolutions: [],
            },
            localOverride: false,
            conflictRole: 'victim',
            canEndorse: false,
            hasOwnEndorsement: false,
          });
        }
        if (cmd === 'import_double_spend_proof') {
          return Promise.resolve({
            proofId: mockProofId,
            alreadyKnown: true,
            addedEndorsements: 2,
          });
        }
        if (cmd === 'get_contacts') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      (open as Mock).mockResolvedValue('/tmp/updated.hmcproof');
      (readFile as Mock).mockResolvedValue(new Uint8Array([9, 8, 7]));

      render(<ConflictDetailsView proofId={mockProofId} onBack={mockOnBack} />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Import Updated Proof \(\.hmcproof\)/i })).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole('button', { name: /Import Updated Proof \(\.hmcproof\)/i }));

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('import_double_spend_proof', {
          proofBytes: [9, 8, 7],
          password: 'test-password',
        });
        expect(screen.getByText('2 new resolution note(s) imported into this conflict.')).toBeInTheDocument();
        expect(fetchCount).toBeGreaterThanOrEqual(2);
      });
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
              endorsementCount: 0,
              localOverride: false,
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
          return Promise.resolve({
            proofId: 'imported-proof-999',
            alreadyKnown: false,
            addedEndorsements: 0,
          });
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
