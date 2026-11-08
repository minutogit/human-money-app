import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, Mock } from 'vitest';
import { ShareCardModal } from '../ShareCardModal';
import { copyToClipboard } from '../../../utils/clipboard';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';

vi.mock('../../../utils/clipboard', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve(true)),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn(),
}));

describe('ShareCardModal Component', () => {
  const defaultProps = {
    isOpen: true,
    onClose: vi.fn(),
    envelopeJson: '{"card_jws":"jwt","hops":1}',
    shareUri: 'humoco://card?env=eyJ0ZXN0IjoxfQ',
    did: 'did:key:z6MkuTest123',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders QR code when qrSvg is provided', () => {
    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect fill="#fff"/></svg>'
      />
    );

    const img = screen.getByAltText(/HuMoCo Contact Card QR Code/i);
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute('src', expect.stringContaining('data:image/svg+xml;utf8'));
    expect(screen.queryByText(/Too large for QR code|Zu groß für QR-Code/i)).not.toBeInTheDocument();
  });

  it('renders fallback notice when qrSvg is null (too large for QR code)', () => {
    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg={null}
      />
    );

    expect(screen.queryByAltText(/HuMoCo Contact Card QR Code/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Too large for QR code|Zu groß für QR-Code/i)).toBeInTheDocument();
    expect(screen.getByText(/Due to extensive listings|Aufgrund umfangreicher Inserate/i)).toBeInTheDocument();
  });

  it('copies share URI to clipboard when clicking Copy Link', async () => {
    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg></svg>'
      />
    );

    const copyBtn = screen.getByRole('button', { name: /Copy Link|Link kopieren/i });
    fireEvent.click(copyBtn);

    expect(copyToClipboard).toHaveBeenCalledWith(defaultProps.shareUri);
    await waitFor(() => {
      expect(screen.getByText(/Link copied!|Link kopiert!/i)).toBeInTheDocument();
    });
  });

  it('saves .hmcard file when clicking Save as File', async () => {
    (save as Mock).mockResolvedValue('/downloads/my-card.hmcard');
    (writeFile as Mock).mockResolvedValue(undefined);

    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg></svg>'
      />
    );

    const saveFileBtn = screen.getByRole('button', { name: /Save as File|Als Datei speichern/i });
    fireEvent.click(saveFileBtn);

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(expect.objectContaining({
        filters: expect.arrayContaining([
          expect.objectContaining({ extensions: ['hmcard'] })
        ])
      }));
      expect(writeFile).toHaveBeenCalledWith(
        '/downloads/my-card.hmcard',
        expect.anything()
      );
    });

    const writtenBytes = (writeFile as Mock).mock.calls[0][1] as Uint8Array;
    expect(writtenBytes.length).toBeGreaterThan(0);
    expect(screen.getByText(/File saved successfully|Datei erfolgreich gespeichert/i)).toBeInTheDocument();
  });
});
