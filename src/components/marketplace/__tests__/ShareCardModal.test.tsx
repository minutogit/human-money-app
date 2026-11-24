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
    fileWire: '{"card_jws":"jwt","hops":1}',
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

    const img = screen.getByAltText(/HuMoCo (Contact|Business) Card QR Code/i);
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

    expect(screen.queryByAltText(/HuMoCo (Contact|Business) Card QR Code/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Too large for QR code|Zu groß für QR-Code/i)).toBeInTheDocument();
    expect(screen.getByText(/Due to extensive (listings|ads)|Aufgrund umfangreicher Inserate/i)).toBeInTheDocument();
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
        defaultPath: expect.stringMatching(/^humoco-.*\.hmcard$/),
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
    expect(new TextDecoder().decode(writtenBytes)).toBe(defaultProps.fileWire);
    expect(screen.getByText(/File saved successfully|Datei erfolgreich gespeichert/i)).toBeInTheDocument();
  });

  it('renders partial QR hint when qrIsPartial is true', () => {
    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect fill="#fff"/></svg>'
        qrIsPartial={true}
      />
    );

    expect(screen.getByText(/wichtigsten Angaben|most important information/i)).toBeInTheDocument();
  });

  it('disables save and copy actions when fileWire and shareUri are null', () => {
    render(
      <ShareCardModal
        isOpen={true}
        onClose={vi.fn()}
        fileWire={null}
        shareUri={null}
        qrSvg={null}
      />
    );

    const saveBtn = screen.getByTestId('share-card-save-file-button');
    const copyBtn = screen.getByTestId('share-card-copy-link-button');
    expect(saveBtn).toBeDisabled();
    expect(copyBtn).toBeDisabled();

    fireEvent.click(saveBtn);
    fireEvent.click(copyBtn);
    expect(save).not.toHaveBeenCalled();
    expect(copyToClipboard).not.toHaveBeenCalled();
  });

  it('handles cancelled save dialog cleanly and displays error when writeFile fails', async () => {
    (save as Mock).mockResolvedValueOnce(null).mockResolvedValueOnce('/downloads/fail.hmcard');
    (writeFile as Mock).mockRejectedValueOnce(new Error('Disk read-only'));

    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>'
      />
    );

    const saveBtn = screen.getByTestId('share-card-save-file-button');
    fireEvent.click(saveBtn);
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    expect(writeFile).not.toHaveBeenCalled();
    expect(screen.queryByText(/File saved successfully|Datei erfolgreich gespeichert/i)).not.toBeInTheDocument();

    fireEvent.click(saveBtn);
    expect(await screen.findByText(/Speichern der Datei fehlgeschlagen|Failed to save file/i)).toBeInTheDocument();
    expect(saveBtn).toBeEnabled();
  });

  it('renders custom or default headings and wires footer Close button to onClose', () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <ShareCardModal
        {...defaultProps}
        onClose={onClose}
        title="Custom Export"
        subtitle="Custom Subtitle"
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: 'Custom Export' })).toBeInTheDocument();
    expect(screen.getByText('Custom Subtitle')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^Schließen$|^Close$/i }));
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(<ShareCardModal {...defaultProps} onClose={onClose} />);
    expect(screen.getByRole('heading', { level: 2, name: /Visitenkarte teilen|Share Business Card/i })).toBeInTheDocument();
    expect(screen.getByText(/Teilen per QR-Code, Datei oder Link|Share via QR code, file, or link/i)).toBeInTheDocument();
  });

  it('resets transient copy and save feedback state when modal closes and reopens', async () => {
    (save as Mock).mockResolvedValue('/downloads/my-card.hmcard');
    (writeFile as Mock).mockResolvedValue(undefined);

    const { rerender } = render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>'
      />
    );

    fireEvent.click(screen.getByTestId('share-card-copy-link-button'));
    expect(await screen.findByText(/Link copied!|Link kopiert!/i)).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('share-card-save-file-button'));
    expect(await screen.findByText(/File saved successfully|Datei erfolgreich gespeichert/i)).toBeInTheDocument();

    rerender(
      <ShareCardModal
        {...defaultProps}
        isOpen={false}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>'
      />
    );
    rerender(
      <ShareCardModal
        {...defaultProps}
        isOpen={true}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>'
      />
    );

    expect(screen.queryByText(/Link copied!|Link kopiert!/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/File saved successfully|Datei erfolgreich gespeichert/i)).not.toBeInTheDocument();
  });

  it('rejects unsafe script-bearing QR SVG markup in QrCodeDisplay', () => {
    render(
      <ShareCardModal
        {...defaultProps}
        qrSvg='<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><rect/></svg>'
      />
    );

    expect(screen.queryByAltText(/HuMoCo (Contact|Business) Card QR Code/i)).not.toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'HuMoCo Contact Card QR Code' })).toBeInTheDocument();
  });
});
