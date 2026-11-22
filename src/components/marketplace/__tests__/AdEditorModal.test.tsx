import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AdEditorModal } from '../AdEditorModal';
import { UiModeProvider } from '../../../context/UiModeContext';
import { FrontendMyAd } from '../../../types';
import { marketService } from '../../../services/marketService';
import * as imageCompression from '../../../utils/imageCompression';

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

    // Submit (defaults to saving as local draft)
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    expect(onSave).toHaveBeenCalledWith({
      kind: 'request',
      title: 'Suche Werkzeugkoffer',
      text: 'Brauche Ratschenkasten für Samstag',
      pricing: '25 Minuto',
      durationDays: 90,
      allowInquiries: false,
      isRemote: true,
      published: false,
    });
  });

  it('allows custom pricing text, publishNow toggle, and duration selection in Expert mode', () => {
    const onSave = vi.fn();

    render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          activeAdsCount={1}
          allowInquiries={true}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('ad-kind-offer-btn'));
    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Fahrrad-Service' } });
    fireEvent.change(screen.getByTestId('ad-text-input'), { target: { value: 'Schnelle Hilfe bei Reifen und Kette' } });
    fireEvent.change(screen.getByTestId('ad-pricing-input'), { target: { value: '15 Minuto' } });

    const duration30Tab = screen.getByRole('tab', { name: /30 Tage|30 days/i });
    fireEvent.click(duration30Tab);

    fireEvent.click(screen.getByTestId('ad-is-remote-checkbox'));
    fireEvent.click(screen.getByTestId('ad-publish-now-checkbox'));
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'offer',
      title: 'Fahrrad-Service',
      text: 'Schnelle Hilfe bei Reifen und Kette',
      pricing: '15 Minuto',
      durationDays: 30,
      allowInquiries: true,
      isRemote: true,
      published: true,
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
      published: true,
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

  it('disables publish-now checkbox when limit is reached on create while still allowing local draft save', () => {
    const onSave = vi.fn();
    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        activeAdsCount={5}
        maxAds={5}
      />
    );

    expect(screen.getByText(/Du hast das Limit von 5 aktiven Inseraten erreicht|limit of 5 active ads/i)).toBeInTheDocument();
    expect(screen.getByTestId('ad-publish-now-checkbox')).toBeDisabled();

    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Entwurf trotz Limit' } });
    const saveBtn = screen.getByTestId('ad-submit-button');
    expect(saveBtn).toBeEnabled();
    fireEvent.click(saveBtn);
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Entwurf trotz Limit',
      published: false,
    }));
  });

  it('renders quota status badge and loads ad image when editing ad has imageToken', async () => {
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
      published: true,
      imageToken: 'token-abc-123',
    };

    const mockDataUri = 'data:image/webp;base64,mockpreviewdata';
    vi.spyOn(marketService, 'getAdImage').mockResolvedValue(mockDataUri);

    const onSave = vi.fn();

    render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={existingAdWithImage}
          ownerDid="did:key:z6MkuSeller"
          mediaMaxCount={5}
          mediaUsedCount={2}
        />
      </UiModeProvider>
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

  it('resets durationDays to 90 when switching from Create (30 days) through closed to Edit', () => {
    const existingAd: FrontendMyAd = {
      id: 'ad-reset-duration',
      kind: 'offer',
      title: 'Bestehendes Inserat',
      text: 'Text',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      published: true,
    };
    const onSave = vi.fn();

    const { rerender } = render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    const tab30 = screen.getByRole('tab', { name: /30 Tage|30 days/i });
    fireEvent.click(tab30);
    expect(tab30).toHaveAttribute('aria-selected', 'true');

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={false}
          onClose={vi.fn()}
          onSave={onSave}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    const tab90AfterCreate = screen.getByRole('tab', { name: /90 Tage|90 days/i });
    expect(tab90AfterCreate).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(screen.getByRole('tab', { name: /30 Tage|30 days/i }));

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={false}
          onClose={vi.fn()}
          onSave={onSave}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={existingAd}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    expect(screen.queryByRole('tab', { name: /90 Tage|90 days/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('ad-submit-button'));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      id: 'ad-reset-duration',
      durationDays: 90,
    }));
  });

  it('preserves unsaved title and text edits when parent re-renders with new editingAd reference for same id', () => {
    const existingAd: FrontendMyAd = {
      id: 'ad-keep-edits',
      kind: 'offer',
      title: 'Alter Titel',
      text: 'Alter Text',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 90,
      allowInquiries: true,
      status: 'active',
      daysLeft: 90,
      published: true,
    };
    const onSave = vi.fn();

    const { rerender } = render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={existingAd}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Bearbeiteter Titel' } });
    fireEvent.change(screen.getByTestId('ad-text-input'), { target: { value: 'Bearbeiteter Text' } });

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={{ ...existingAd }}
          activeAdsCount={1}
        />
      </UiModeProvider>
    );

    expect(screen.getByTestId('ad-title-input')).toHaveValue('Bearbeiteter Titel');
    expect(screen.getByTestId('ad-text-input')).toHaveValue('Bearbeiteter Text');

    fireEvent.click(screen.getByTestId('ad-submit-button'));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      id: 'ad-keep-edits',
      title: 'Bearbeiteter Titel',
      text: 'Bearbeiteter Text',
    }));
  });

  it('ignores stale getAdImage result after user replaces image with a new upload', async () => {
    let resolveStaleImage!: (val: string) => void;
    vi.spyOn(marketService, 'getAdImage').mockImplementation(
      () => new Promise<string>((resolve) => { resolveStaleImage = resolve; })
    );
    vi.spyOn(imageCompression, 'compressAdImageToWebpDataUri').mockResolvedValue(
      'data:image/webp;base64,FRESH_UPLOAD'
    );

    const existingAdWithImage: FrontendMyAd = {
      id: 'ad-stale-img',
      kind: 'offer',
      title: 'Stale Image Test',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      published: true,
      imageToken: 'tok-stale',
    };

    render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={vi.fn()}
          editingAd={existingAdWithImage}
          ownerDid="did:key:z6MkuSeller"
        />
      </UiModeProvider>
    );

    const file = new File(['img'], 'fresh.png', { type: 'image/png' });
    fireEvent.change(screen.getByTestId('ad-image-file-input'), { target: { files: [file] } });

    await waitFor(() => {
      expect(screen.getByAltText('Ad preview')).toHaveAttribute('src', 'data:image/webp;base64,FRESH_UPLOAD');
    });

    resolveStaleImage('data:image/webp;base64,STALE_FETCHED');
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.getByAltText('Ad preview')).toHaveAttribute('src', 'data:image/webp;base64,FRESH_UPLOAD');
  });

  it('ignores stale compressAdImageToWebpDataUri result after modal closes and opens for another ad', async () => {
    let resolveStaleCompress!: (val: string) => void;
    vi.spyOn(imageCompression, 'compressAdImageToWebpDataUri').mockImplementation(
      () => new Promise<string>((resolve) => { resolveStaleCompress = resolve; })
    );

    const ad1: FrontendMyAd = {
      id: 'ad-1',
      kind: 'offer',
      title: 'Inserat Eins',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      published: true,
    };
    const ad2: FrontendMyAd = {
      id: 'ad-2',
      kind: 'offer',
      title: 'Inserat Zwei',
      publishedAt: nowSecs - 86400,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      published: true,
    };
    const onSave = vi.fn();

    const { rerender } = render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={ad1}
        />
      </UiModeProvider>
    );

    const file = new File(['img'], 'slow.png', { type: 'image/png' });
    fireEvent.change(screen.getByTestId('ad-image-file-input'), { target: { files: [file] } });
    expect(screen.getByTestId('ad-submit-button')).toBeDisabled();

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={ad2}
        />
      </UiModeProvider>
    );

    expect(screen.getByTestId('ad-title-input')).toHaveValue('Inserat Zwei');

    resolveStaleCompress('data:image/webp;base64,STALE_COMPRESSED');
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.queryByAltText('Ad preview')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('ad-submit-button'));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      id: 'ad-2',
    }));
    expect(onSave.mock.calls[0][0]).not.toHaveProperty('imageWebpDataUri');
  });

  it('supports creating an Event ad with date, optional time, and 500-char text', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const futureDate = new Date();
    futureDate.setDate(futureDate.getDate() + 10);
    const yyyy = futureDate.getFullYear();
    const mm = String(futureDate.getMonth() + 1).padStart(2, '0');
    const dd = String(futureDate.getDate()).padStart(2, '0');
    const dateStr = `${yyyy}-${mm}-${dd}`;

    render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    fireEvent.click(screen.getByTestId('ad-kind-event-btn'));
    expect(screen.getByTestId('ad-event-date-input')).toBeInTheDocument();
    expect(screen.getByTestId('ad-event-time-input')).toBeInTheDocument();
    // Duration segmented tabs must be hidden for events
    expect(screen.queryByText(/30 Tage|30 days/i)).not.toBeInTheDocument();

    fireEvent.change(screen.getByTestId('ad-event-date-input'), { target: { value: dateStr } });
    fireEvent.change(screen.getByTestId('ad-event-time-input'), { target: { value: '18:30' } });
    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Minuto-Stammtisch' } });
    const longText = 'A'.repeat(420);
    fireEvent.change(screen.getByTestId('ad-text-input'), { target: { value: longText } });
    expect(screen.getByText('420 / 500')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('ad-submit-button'));
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        kind: 'event',
        title: 'Minuto-Stammtisch',
        text: longText,
        eventAllDay: false,
        eventAt: expect.any(Number),
      }));
    });
  });
});
