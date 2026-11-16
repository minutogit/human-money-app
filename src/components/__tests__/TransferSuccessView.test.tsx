import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TransferSuccessView } from '../TransferSuccessView';
import { postService } from '../../services/postService';
import { invoke } from '@tauri-apps/api/core';
import { UI_MODE_STORAGE_KEY } from '../../context/UiModeContext';

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
  useOptionalSession: () => null,
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
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_contacts') return [];
      return undefined;
    });
  });

  it('renders emerald success card when initially deposited to 1 or more sync endpoints', async () => {
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
    expect(screen.getByText(/transfer bundle was securely deposited into 2 (Post sync|sync service)/i)).toBeInTheDocument();

    // Direct sharing options remain fully accessible (100% offline sovereignty)
    expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
  });

  it('renders amber pending card when initially deposited to 0 sync endpoints', async () => {
    render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={0}
        depositError="No active sync endpoints"
        onDone={vi.fn()}
      />
    );

    // Amber pending card is displayed
    expect(screen.getByText(/Deposit Pending or Incomplete/i)).toBeInTheDocument();
    expect(screen.getByText("No active sync endpoints")).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Redeposit into Post/i })).toBeInTheDocument();
  });

  it('CRITICAL SST-V4 RETRY INVARIANT: clicking retry calls depositTransferToPost and NEVER create_transfer_bundle', async () => {
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'simple');
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

  it('APP-UX-09 P3 Simple Mode: shows clear delivered-to heading without raw did:key and collapses direct handover buttons', async () => {
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'simple');

    const { container } = render(
      <TransferSuccessView
        bundleData={dummyBundleData}
        recipientId={recipientId}
        summary="10.00 Minuto"
        bundleId={bundleId}
        initialDepositedTo={2}
        onDone={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('delivered-to-heading')).toBeInTheDocument();
    });

    expect(screen.getByTestId('delivered-to-heading').textContent).toContain('✓');
    expect(screen.getByTestId('delivered-to-heading').textContent).toMatch(/Delivered to/i);
    expect(container.innerHTML).not.toContain('did:key:');

    const handoverToggle = screen.getByTestId('direct-handover-toggle');
    expect(handoverToggle).toBeInTheDocument();
    expect(screen.queryByTestId('save-transfer-file-button')).not.toBeInTheDocument();

    fireEvent.click(handoverToggle);
    expect(screen.getByTestId('save-transfer-file-button')).toBeInTheDocument();
  });
});
