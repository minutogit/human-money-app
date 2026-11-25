import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ExportSigningRequestModal } from '../ExportSigningRequestModal';
import { marketService } from '../../../services/marketService';
import { contactService } from '../../../services/contactService';
import { voucherService } from '../../../services/voucherService';
import { postService } from '../../../services/postService';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

vi.mock('../../../services/marketService', () => ({
  marketService: {
    resolveCardRecipient: vi.fn(),
  },
}));

vi.mock('../../../services/contactService', () => ({
  contactService: {
    getContacts: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../services/voucherService', () => ({
  voucherService: {
    createSigningRequest: vi.fn(),
  },
}));

vi.mock('../../../services/postService', () => ({
  postService: {
    sendSigningRequestViaPost: vi.fn(),
  },
}));

vi.mock('../../../services/settingsService', () => ({
  settingsService: {
    getConfig: vi.fn(),
  },
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn(),
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
    warn: vi.fn(),
  },
}));

describe('ExportSigningRequestModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(contactService.getContacts).mockResolvedValue([]);
  });

  it('H12: disables send action while resolving card URI and discards stale resolution after newer edit', async () => {
    let resolveCard!: (val: { recipientId: string; did: string }) => void;
    vi.mocked(marketService.resolveCardRecipient).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCard = resolve;
        }),
    );

    render(
      <ExportSigningRequestModal
        isOpen={true}
        voucherId="v-123"
        onClose={vi.fn()}
        config={null}
        protectAction={vi.fn()}
        onConfigRefresh={vi.fn()}
        contacts={[]}
      />,
    );

    const input = screen.getByPlaceholderText(/Enter User ID/i) as HTMLInputElement;
    const sendBtn = screen.getByTestId('send-signing-request-post-button');

    fireEvent.change(input, {
      target: { value: 'humoco://card?env=eyJ0ZXN0IjoxfQ' },
    });

    await waitFor(() => {
      expect(marketService.resolveCardRecipient).toHaveBeenCalledTimes(1);
    });
    expect(sendBtn).toBeDisabled();

    // User replaces the input with a newer direct DID before the first async lookup settles
    fireEvent.change(input, {
      target: { value: 'did:key:z6MkNewerSigner111' },
    });
    expect(sendBtn).not.toBeDisabled();

    // Late resolution finishes -> must NOT overwrite the newer recipientId
    resolveCard({
      recipientId: 'did:key:z6MkStaleSigner999',
      did: 'did:key:z6MkStaleSigner999',
    });

    await waitFor(() => {
      expect(input.value).toBe('did:key:z6MkNewerSigner111');
      expect(sendBtn).not.toBeDisabled();
    });
  });

  it('validates password protection rules and exports cleartext request to disk', async () => {
    const onClose = vi.fn();
    const onSuccess = vi.fn();
    vi.mocked(voucherService.createSigningRequest).mockResolvedValue([1, 2, 3]);
    vi.mocked(save).mockResolvedValue('/tmp/req.ask');
    vi.mocked(writeFile).mockResolvedValue(undefined);

    render(
      <ExportSigningRequestModal
        isOpen={true}
        voucherId="v-123"
        onClose={onClose}
        config={null}
        protectAction={async (action) => action(null)}
        onConfigRefresh={vi.fn()}
        contacts={[]}
        onSuccess={onSuccess}
      />,
    );

    const encryptCheckbox = document.getElementById('wallet-encryptToDid') as HTMLInputElement;
    fireEvent.click(encryptCheckbox);

    const protectCheckbox = document.getElementById('wallet-protectWithPassword') as HTMLInputElement;
    fireEvent.click(protectCheckbox);

    const confirmBtn = screen.getByRole('button', { name: en['voucher.export.exportRequest'] });
    fireEvent.click(confirmBtn);
    expect(await screen.findByText(en['voucher.errorEnterPassword'])).toBeInTheDocument();
    expect(voucherService.createSigningRequest).not.toHaveBeenCalled();

    fireEvent.change(screen.getByPlaceholderText(en['voucher.export.passwordPlaceholder']), {
      target: { value: 'secret1' },
    });
    fireEvent.change(screen.getByPlaceholderText(en['voucher.export.confirmPasswordPlaceholder']), {
      target: { value: 'secret2' },
    });
    fireEvent.click(confirmBtn);
    expect(await screen.findByText(en['voucher.export.errorPasswordMismatch'])).toBeInTheDocument();
    expect(voucherService.createSigningRequest).not.toHaveBeenCalled();

    fireEvent.click(protectCheckbox);
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(voucherService.createSigningRequest).toHaveBeenCalledWith(
        'v-123',
        { type: 'Cleartext' },
        undefined,
      );
      expect(writeFile).toHaveBeenCalledWith('/tmp/req.ask', new Uint8Array([1, 2, 3]));
      expect(onSuccess).toHaveBeenCalledWith('/tmp/req.ask');
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it('requires recipient DID for encrypted export and skips writeFile/close on save cancel', async () => {
    const onClose = vi.fn();
    const onSuccess = vi.fn();
    vi.mocked(voucherService.createSigningRequest).mockResolvedValue([4, 5]);
    vi.mocked(save).mockResolvedValue(null);

    render(
      <ExportSigningRequestModal
        isOpen={true}
        voucherId="v-123"
        onClose={onClose}
        config={null}
        protectAction={async (action) => action(null)}
        onConfigRefresh={vi.fn()}
        contacts={[]}
        onSuccess={onSuccess}
      />,
    );

    const confirmBtn = screen.getByRole('button', { name: en['voucher.export.exportRequest'] });
    fireEvent.click(confirmBtn);
    expect(await screen.findByText(en['voucher.errorEnterRecipientDid'])).toBeInTheDocument();
    expect(voucherService.createSigningRequest).not.toHaveBeenCalled();

    const input = screen.getByPlaceholderText(/Enter User ID/i);
    fireEvent.change(input, { target: { value: 'did:key:z6MkNewerSigner111' } });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(voucherService.createSigningRequest).toHaveBeenCalledWith(
        'v-123',
        { type: 'TargetDid', value: ['did:key:z6MkNewerSigner111', 'TrialDecryption'] },
        undefined,
      );
      expect(save).toHaveBeenCalledTimes(1);
    });
    expect(writeFile).not.toHaveBeenCalled();
    expect(onSuccess).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('handles sendSigningRequestViaPost errors, zero syncs, and auto-close after success', async () => {
    const onClose = vi.fn();
    vi.mocked(voucherService.createSigningRequest).mockResolvedValue([9, 8]);

    render(
      <ExportSigningRequestModal
        isOpen={true}
        voucherId="v-123"
        onClose={onClose}
        config={null}
        protectAction={async (action) => action(null)}
        onConfigRefresh={vi.fn()}
        contacts={[]}
      />,
    );

    const input = screen.getByPlaceholderText(/Enter User ID/i);
    fireEvent.change(input, { target: { value: 'did:key:z6MkNewerSigner111' } });
    const sendBtn = screen.getByTestId('send-signing-request-post-button');

    vi.mocked(postService.sendSigningRequestViaPost).mockResolvedValueOnce({
      depositedTo: 0,
      recipientDepositedTo: 0,
      msgId: 'msg-err',
      errors: ['Relay offline'],
    });
    fireEvent.click(sendBtn);
    expect(await screen.findByText('Relay offline')).toBeInTheDocument();

    vi.mocked(postService.sendSigningRequestViaPost).mockResolvedValueOnce({
      depositedTo: 0,
      recipientDepositedTo: 0,
      msgId: 'msg-zero',
      errors: [],
    });
    fireEvent.click(sendBtn);
    expect(await screen.findByText(en['voucher.signRequest.sentViaPostZeroSyncs'])).toBeInTheDocument();

    vi.useFakeTimers();
    try {
      vi.mocked(postService.sendSigningRequestViaPost).mockResolvedValueOnce({
        depositedTo: 2,
        recipientDepositedTo: 2,
        msgId: 'msg-ok',
        errors: [],
      });

      await act(async () => {
        fireEvent.click(sendBtn);
        for (let i = 0; i < 6; i += 1) {
          await Promise.resolve();
        }
      });

      expect(screen.getByText(/deposited to 2 sync service/i)).toBeInTheDocument();
      act(() => {
        vi.advanceTimersByTime(1599);
      });
      expect(onClose).not.toHaveBeenCalled();

      act(() => {
        vi.advanceTimersByTime(1);
      });
      expect(onClose).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
