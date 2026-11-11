import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AdEditorModal } from '../AdEditorModal';
import { FrontendMyAd } from '../../../types';

describe('AdEditorModal Component', () => {
  const nowSecs = Math.floor(Date.now() / 1000);

  it('renders creation form with Offer/Request options and saves new ad', () => {
    const onSave = vi.fn();
    const onClose = vi.fn();

    render(
      <AdEditorModal
        isOpen={true}
        onClose={onClose}
        onSave={onSave}
        activeAdsCount={1}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Neues Inserat|New ad/i })).toBeInTheDocument();

    // Select "Gesuch"
    const requestRadio = screen.getByTestId('ad-kind-request-btn');
    fireEvent.click(requestRadio);

    // Title & text
    const titleInput = screen.getByPlaceholderText(/z\. B\. Suche Leiter|e\.g\. Looking for a ladder/i);
    fireEvent.change(titleInput, { target: { value: 'Suche Werkzeugkoffer' } });

    const textInput = screen.getByTestId('ad-text-input');
    fireEvent.change(textInput, { target: { value: 'Brauche Ratschenkasten für Samstag' } });

    // Duration: 90 days
    const duration90Tab = screen.getByRole('tab', { name: /90 Tage|90 days/i });
    fireEvent.click(duration90Tab);

    // Toggle isRemote
    const remoteCheckbox = screen.getByTestId('ad-is-remote-checkbox');
    fireEvent.click(remoteCheckbox);

    // Submit
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith({
      kind: 'request',
      title: 'Suche Werkzeugkoffer',
      text: 'Brauche Ratschenkasten für Samstag',
      durationDays: 90,
      allowInquiries: true,
      isRemote: true,
    });
  });

  it('renders editing form prefilled with existing ad data', () => {
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
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Inserat bearbeiten|Edit ad/i })).toBeInTheDocument();

    const titleInput = screen.getByDisplayValue('Brennholz Buche');
    expect(titleInput).toBeInTheDocument();

    const textInput = screen.getByDisplayValue('Trockenes Buchenholz abzugeben');
    expect(textInput).toBeInTheDocument();

    // Toggle inquiries
    const inquiryToggle = screen.getAllByRole('checkbox')[0];
    fireEvent.click(inquiryToggle);

    // Save
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
});
