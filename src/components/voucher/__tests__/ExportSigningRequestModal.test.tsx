import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ExportSigningRequestModal } from '../ExportSigningRequestModal';
import { marketService } from '../../../services/marketService';
import { contactService } from '../../../services/contactService';

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
});
