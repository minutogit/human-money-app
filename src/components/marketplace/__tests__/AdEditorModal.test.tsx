import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AdEditorModal } from '../AdEditorModal';
import { FrontendMyAd } from '../../../types';
import { marketService } from '../../../services/marketService';

describe('AdEditorModal Component', () => {
  const nowSecs = Math.floor(Date.now() / 1000);

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('renders Simple mode with price chips, hides duration tabs & allowInquiries toggle, and saves new ad', () => {
    const onSave = vi.fn();
    const onClose = vi.fn();

    render(
      <AdEditorModal
        isOpen={true}
        onClose={onClose}
        onSave={onSave}
        activeAdsCount={1}
        allowInquiries={false}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Neues Inserat|New ad/i })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /90 Tage|90 days/i })).not.toBeInTheDocument();
    expect(screen.queryByTestId('ad-pricing-input')).not.toBeInTheDocument();
    expect(screen.queryByTestId('ad-is-remote-checkbox')).not.toBeInTheDocument();

    // Select "Gesuch"
    fireEvent.click(screen.getByTestId('ad-kind-request-btn'));

    // Title & text
    fireEvent.change(screen.getByPlaceholderText(/z\. B\. Suche Leiter|e\.g\. Looking for a ladder/i), {
      target: { value: 'Suche Werkzeugkoffer' },
    });
    fireEvent.change(screen.getByTestId('ad-text-input'), {
      target: { value: 'Brauche Ratschenkasten für Samstag' },
    });

    // Price chip + numeric amount -> "25 Minuto"
    fireEvent.click(screen.getByTestId('price-chip-minuto'));
    fireEvent.change(screen.getByTestId('ad-price-amount-input'), { target: { value: '25' } });

    // Open More details disclosure to toggle isRemote
    fireEvent.click(screen.getByTestId('ad-more-details-toggle'));
    fireEvent.click(screen.getByTestId('ad-is-remote-checkbox'));

    // Submit
    fireEvent.click(screen.getByRole('button', { name: /^Speichern$|^Save$/i }));

    expect(onSave).toHaveBeenCalledWith({
      kind: 'request',
      title: 'Suche Werkzeugkoffer',
      text: 'Brauche Ratschenkasten für Samstag',
      pricing: '25 Minuto',
      durationDays: 90,
      allowInquiries: false,
      isRemote: true,
    });
  });

  it('allows custom pricing text and duration selection in Expert mode', () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    const onSave = vi.fn();

    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        activeAdsCount={1}
        allowInquiries={true}
      />
    );

    fireEvent.click(screen.getByTestId('ad-kind-offer-btn'));
    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Fahrrad-Service' } });
    fireEvent.change(screen.getByTestId('ad-text-input'), { target: { value: 'Schnelle Hilfe bei Reifen und Kette' } });
    fireEvent.change(screen.getByTestId('ad-pricing-input'), { target: { value: '15 Minuto' } });

    const duration30Tab = screen.getByRole('tab', { name: /30 Tage|30 days/i });
    fireEvent.click(duration30Tab);

    fireEvent.click(screen.getByTestId('ad-is-remote-checkbox'));
    fireEvent.click(screen.getByRole('button', { name: /^Speichern$|^Save$/i }));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'offer',
      title: 'Fahrrad-Service',
      text: 'Schnelle Hilfe bei Reifen und Kette',
      pricing: '15 Minuto',
      durationDays: 30,
      allowInquiries: true,
      isRemote: true,
    }));
  });

  it('renders editing form prefilled with existing ad data and propagates profile allowInquiries', () => {
    const existingAd: FrontendMyAd = {
      id: 'ad-existing',
      kind: 'offer',
      title: 'Brennholz Buche',
      text: 'Trockenes Buchenholz abzugeben',
      pricing: '80 Minuto / Ster',
      publishedAt: nowSecs - 86400 * 10,
      expiresAt: nowSecs + 86400 * 20,
      allowInquiries: false,
      status: 'active',
      daysLeft: 20,
    };

    const onSave = vi.fn();

    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        editingAd={existingAd}
        activeAdsCount={1}
        allowInquiries={true}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Inserat bearbeiten|Edit ad/i })).toBeInTheDocument();
    expect(screen.getByDisplayValue('Brennholz Buche')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Trockenes Buchenholz abzugeben')).toBeInTheDocument();
    expect(screen.getByTestId('price-chip-custom-clear')).toBeInTheDocument();

    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith({
      id: 'ad-existing',
      kind: 'offer',
      title: 'Brennholz Buche',
      text: 'Trockenes Buchenholz abzugeben',
      pricing: '80 Minuto / Ster',
      durationDays: 90,
      allowInquiries: true,
      isRemote: false,
    });
  });

  it('shows limit warning and disables submit when limit is reached on create', () => {
    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={vi.fn()}
        activeAdsCount={5}
        maxAds={5}
      />
    );

    expect(screen.getByText(/Du hast das Limit von 5 aktiven Inseraten erreicht|limit of 5 active ads/i)).toBeInTheDocument();
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    expect(saveBtn).toBeDisabled();
  });

  it('renders quota status badge and loads ad image when editing ad has imageToken', async () => {
    localStorage.setItem('humoco_ui_mode', 'expert');
    const existingAdWithImage: FrontendMyAd = {
      id: 'ad-img-1',
      kind: 'offer',
      title: 'Kamera Nikon FM2',
      text: 'Klassische SLR Kamera',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      imageToken: 'token-abc-123',
    };

    const mockDataUri = 'data:image/webp;base64,mockpreviewdata';
    vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);

    const onSave = vi.fn();

    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        editingAd={existingAdWithImage}
        ownerDid="did:key:z6MkuSeller"
        mediaMaxCount={5}
        mediaUsedCount={2}
      />
    );

    // Verify quota badge
    expect(screen.getByTestId('media-quota-badge')).toBeInTheDocument();
    expect(screen.getByText(/2 von 5 Bildern belegt|2 of 5 images used/i)).toBeInTheDocument();

    // Verify getAdImage was called and preview thumbnail is rendered
    await waitFor(() => {
      expect(marketService.getAdImage).toHaveBeenCalledWith('did:key:z6MkuSeller', 'token-abc-123');
    });

    const previewImg = await screen.findByAltText('Ad preview');
    expect(previewImg).toHaveAttribute('src', mockDataUri);

    // Click remove button
    const removeBtn = screen.getByTestId('ad-image-remove-btn');
    fireEvent.click(removeBtn);

    // Preview should be gone
    expect(screen.queryByAltText('Ad preview')).not.toBeInTheDocument();

    // Save and verify imageWebpDataUri is null
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      id: 'ad-img-1',
      imageWebpDataUri: null,
    }));
  });
});
