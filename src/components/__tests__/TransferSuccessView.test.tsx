import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TransferSuccessView } from '../TransferSuccessView';
import { postService } from '../../services/postService';
import { invoke } from '@tauri-apps/api/core';

// Mock dependencies
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn().mockResolvedValue('/tmp/test.transfer'),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/clipboard', () => ({
  copyToClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../services/settingsService', () => ({
  settingsService: {
    getSettings: vi.fn().mockResolvedValue({
      lastUsedDirectory: '/tmp',
    }),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn(async (cb) => cb('dummy-password')),
  }),
}));

vi.mock('../../services/postService', () => ({
  postService: {
    depositTransferToPost: vi.fn(),
  },
  POST_INBOX_UPDATED_EVENT: 'humoco://post-inbox-updated',
}));

describe('TransferSuccessView Component (Dual-Delivery & SST-V4 Retry)', () => {
  const dummyBundleData = [1, 2, 3, 4, 5];
  const recipientId = 'did:key:z6Mku7...alice';
  const bundleId = 'bundle-xyz-42';

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders emerald success card when initially deposited to 1 or more drops', async () => {
    render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={2}
        onDone={vi.fn()}
      />
    );

    // Emerald card is visible
    expect(screen.getByText(/Payment deposited in Post/i)).toBeInTheDocument();
    expect(screen.getByText(/transfer bundle was securely deposited into 2 Post drop/i)).toBeInTheDocument();

    // Direct sharing options remain fully accessible (100% offline sovereignty)
    expect(screen.getByText(/Show QR Code/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
  });

  it('renders amber pending card when initially deposited to 0 drops', async () => {
    render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={0}
        depositError="No active drops"
        onDone={vi.fn()}
      />
    );

    // Amber pending card is displayed
    expect(screen.getByText(/Deposit Pending or Incomplete/i)).toBeInTheDocument();
    expect(screen.getByText("No active drops")).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Redeposit into Post/i })).toBeInTheDocument();
  });

  it('CRITICAL SST-V4 RETRY INVARIANT: clicking retry calls depositTransferToPost and NEVER create_transfer_bundle', async () => {
    const depositMock = vi.mocked(postService.depositTransferToPost).mockResolvedValue({
      msgId: 'msg-deposit-123',
      depositedTo: 1,
      errors: [],
    });

    render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={0}
        onDone={vi.fn()}
      />
    );

    const retryBtn = screen.getByRole('button', { name: /Redeposit into Post/i });
    fireEvent.click(retryBtn);

    await waitFor(() => {
      // Must call postService.depositTransferToPost with exact bundleId
      expect(depositMock).toHaveBeenCalledWith(bundleId, 'dummy-password');
      // Must NEVER invoke create_transfer_bundle
      const invokeMock = vi.mocked(invoke);
      const createBundleCalls = invokeMock.mock.calls.filter(
        ([cmd]) => cmd === 'create_transfer_bundle'
      );
      expect(createBundleCalls).toHaveLength(0);
    });

    // Successfully transitioned to deposited status
    await waitFor(() => {
      expect(screen.getByTestId('post-deposit-success-card')).toBeInTheDocument();
      expect(screen.getAllByText(/Payment deposited in Post/i).length).toBeGreaterThanOrEqual(1);
    });
  });

  it('opens QR modal and permits copying raw bundle data for direct offline transfer', async () => {
    render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={1}
        onDone={vi.fn()}
      />
    );

    const qrBtn = screen.getByRole('button', { name: /Show QR Code/i });
    fireEvent.click(qrBtn);

    expect(screen.getByText(/Transfer QR Code & Data/i)).toBeInTheDocument();
    expect(screen.getByText(/Direct Transfer \(Offline\)/i)).toBeInTheDocument();

    const copyDataBtn = screen.getByRole('button', { name: /Copy Bundle Data/i });
    fireEvent.click(copyDataBtn);

    const { copyToClipboard } = await import('../../utils/clipboard');
    expect(copyToClipboard).toHaveBeenCalled();
  });
});
