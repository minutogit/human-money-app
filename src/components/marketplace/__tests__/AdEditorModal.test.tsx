import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AdEditorModal } from '../AdEditorModal';
import { UiModeProvider } from '../../../context/UiModeContext';
import { FrontendMyAd } from '../../../types';
import { marketService } from '../../../services/marketService';
import * as imageCompression from '../../../utils/imageCompression';
import {
  offsetDaysDateInputValue,
  toEventDateInputValue,
  toEventTimeInputValue,
} from '../../../utils/adEventFormat';

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

  it('enforces event date guards for missing, >180-day future, and past dates', () => {
    const onSave = vi.fn();
    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
      />
    );

    fireEvent.click(screen.getByTestId('ad-kind-event-btn'));
    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Termin-Check' } });

    const form = screen.getByTestId('ad-title-input').closest('form')!;
    fireEvent.submit(form);
    expect(screen.getByText(/Bitte wähle ein gültiges Termindatum\.|Please select a valid event date\./i)).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();

    fireEvent.change(screen.getByTestId('ad-event-date-input'), {
      target: { value: offsetDaysDateInputValue(181) },
    });
    fireEvent.submit(form);
    expect(screen.getByText(/maximal 180 Tage in der Zukunft|more than 180 days in the future/i)).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();

    fireEvent.change(screen.getByTestId('ad-event-date-input'), {
      target: { value: offsetDaysDateInputValue(-2) },
    });
    fireEvent.submit(form);
    expect(screen.getByText(/Der Termin muss in der Zukunft liegen\.|The event date must be in the future\./i)).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();
  });

  it('supports all-day events and applies 6-hour edit grace only to unchanged event timestamps', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true, now: new Date(2026, 5, 15, 12, 0, 0) });
    try {
      const onSave = vi.fn().mockResolvedValue(undefined);
      const twoHoursAgoSecs = Math.floor(new Date(2026, 5, 15, 10, 0, 0).getTime() / 1000);
      const unchangedPastEvent: FrontendMyAd = {
        id: 'ad-ev-grace',
        kind: 'event',
        title: 'Laufender Markt',
        eventAt: twoHoursAgoSecs,
        eventAllDay: false,
        publishedAt: twoHoursAgoSecs - 86400,
        expiresAt: twoHoursAgoSecs + 86400,
        allowInquiries: true,
        status: 'active',
        daysLeft: 1,
        published: true,
      };

      const { rerender } = render(
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
          editingAd={unchangedPastEvent}
        />
      );

      expect(screen.getByTestId('ad-event-date-input')).toHaveValue(toEventDateInputValue(twoHoursAgoSecs, false));
      expect(screen.getByTestId('ad-event-time-input')).toHaveValue(toEventTimeInputValue(twoHoursAgoSecs, false));

      fireEvent.click(screen.getByTestId('ad-submit-button'));
      await waitFor(() => {
        expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
          id: 'ad-ev-grace',
          eventAt: twoHoursAgoSecs,
          eventAllDay: false,
        }));
      });

      // Changing time to another past hour breaks unchanged-timestamp grace and fails
      fireEvent.change(screen.getByTestId('ad-event-time-input'), { target: { value: '11:00' } });
      fireEvent.click(screen.getByTestId('ad-submit-button'));
      expect(screen.getByText(/Der Termin muss in der Zukunft liegen\.|The event date must be in the future\./i)).toBeInTheDocument();
      expect(onSave).toHaveBeenCalledTimes(1);

      // Switching to new all-day event (empty time) sets eventAllDay: true
      rerender(
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
        />
      );
      fireEvent.click(screen.getByTestId('ad-kind-event-btn'));
      fireEvent.change(screen.getByTestId('ad-event-date-input'), {
        target: { value: offsetDaysDateInputValue(5) },
      });
      fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Ganztages-Flohmarkt' } });
      fireEvent.click(screen.getByTestId('ad-submit-button'));

      await waitFor(() => {
        expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({
          kind: 'event',
          title: 'Ganztages-Flohmarkt',
          eventAllDay: true,
        }));
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('rejects blank title and over-limit title, text, and pricing on form submit', () => {
    const onSave = vi.fn();
    render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={vi.fn()}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    const titleInput = screen.getByTestId('ad-title-input');
    const textInput = screen.getByTestId('ad-text-input');
    const pricingInput = screen.getByTestId('ad-pricing-input');
    const form = titleInput.closest('form')!;

    fireEvent.change(titleInput, { target: { value: '   ' } });
    fireEvent.submit(form);
    expect(screen.getByText(/Titel ist erforderlich|Title is required/i)).toBeInTheDocument();

    fireEvent.change(titleInput, { target: { value: 'T'.repeat(81) } });
    fireEvent.submit(form);
    expect(screen.getByText(/Maximal 80 Zeichen erlaubt|Maximum 80 characters allowed/i)).toBeInTheDocument();

    fireEvent.change(titleInput, { target: { value: 'Gültiger Titel' } });
    fireEvent.change(textInput, { target: { value: 'X'.repeat(501) } });
    fireEvent.submit(form);
    expect(screen.getByText(/Maximal 500 Zeichen erlaubt|Maximum 500 characters allowed/i)).toBeInTheDocument();

    fireEvent.change(textInput, { target: { value: 'Kurztext' } });
    fireEvent.change(pricingInput, { target: { value: 'P'.repeat(41) } });
    fireEvent.submit(form);
    expect(screen.getByText(/Maximal 40 Zeichen erlaubt|Maximum 40 characters allowed/i)).toBeInTheDocument();

    expect(onSave).not.toHaveBeenCalled();
  });

  it('handles pricing chip toggles, preset parsing for FreeTaler, and custom pricing reset', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const adWithFt: FrontendMyAd = {
      id: 'ad-ft',
      kind: 'offer',
      title: 'Gartenhilfe',
      pricing: '12.5 FreeTaler',
      publishedAt: nowSecs - 100,
      expiresAt: nowSecs + 86400 * 30,
      allowInquiries: true,
      status: 'active',
      daysLeft: 30,
      published: true,
    };

    const { rerender } = render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        editingAd={adWithFt}
      />
    );

    expect(screen.getByTestId('ad-price-amount-input')).toHaveValue('12.5');
    fireEvent.change(screen.getByTestId('ad-price-amount-input'), { target: { value: '15' } });
    fireEvent.click(screen.getByTestId('price-chip-minuto'));
    fireEvent.click(screen.getByTestId('price-chip-free'));
    fireEvent.click(screen.getByTestId('price-chip-free'));
    fireEvent.click(screen.getByTestId('price-chip-negotiable'));
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        pricing: expect.stringMatching(/^VB$|^Negotiable$/i),
      }));
    });

    // Clear custom preset via clear button
    const customAd: FrontendMyAd = { ...adWithFt, id: 'ad-custom-clear', pricing: 'Tausch nach Absprache' };
    rerender(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        editingAd={customAd}
      />
    );

    fireEvent.click(screen.getByTestId('price-chip-custom-clear'));
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    await waitFor(() => {
      expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({
        id: 'ad-custom-clear',
        pricing: undefined,
      }));
    });
  });

  it('disables publish-now toggle when own business card is unpublished', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <AdEditorModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={onSave}
        cardPublished={false}
      />
    );

    const publishCheckbox = screen.getByTestId('ad-publish-now-checkbox');
    expect(publishCheckbox).toBeDisabled();
    expect(publishCheckbox).not.toBeChecked();
    expect(screen.getByText(/Wird zunächst lokal als Entwurf gespeichert|Will be saved locally as a draft first/i)).toBeInTheDocument();

    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Lokaler Entwurf' } });
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        title: 'Lokaler Entwurf',
        published: false,
      }));
    });
  });

  it('surfaces save/compression errors without closing and ignores late save completion after close', async () => {
    vi.spyOn(imageCompression, 'compressAdImageToWebpDataUri').mockRejectedValueOnce(new Error('Image too large'));

    let resolveLateSave!: () => void;
    const onSave = vi
      .fn()
      .mockRejectedValueOnce(new Error('Backend write failed'))
      .mockImplementationOnce(() => new Promise<void>((resolve) => { resolveLateSave = resolve; }));
    const onClose = vi.fn();

    const { rerender } = render(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={true}
          onClose={onClose}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    // Image compression error surfaces and re-enables submit
    const badFile = new File(['bad'], 'bad.png', { type: 'image/png' });
    fireEvent.change(screen.getByTestId('ad-image-file-input'), { target: { files: [badFile] } });
    expect(await screen.findByText('Image too large')).toBeInTheDocument();

    fireEvent.change(screen.getByTestId('ad-title-input'), { target: { value: 'Fehler-Test' } });
    fireEvent.click(screen.getByTestId('ad-submit-button'));

    expect(await screen.findByText('Backend write failed')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('ad-submit-button')).toBeEnabled();

    // Start slow save then close modal before resolution -> onClose must not fire
    fireEvent.click(screen.getByTestId('ad-submit-button'));
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledTimes(2);
    });

    rerender(
      <UiModeProvider initialMode="expert">
        <AdEditorModal
          isOpen={false}
          onClose={onClose}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    resolveLateSave();
    await Promise.resolve();
    await Promise.resolve();
    expect(onClose).not.toHaveBeenCalled();
  });
});
