import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ProfileCardEditor } from '../ProfileCardEditor';
import { FrontendMyMarketProfile } from '../../../types';

vi.mock('../../../utils/geoUtils', async () => {
  const actual = await vi.importActual<any>('../../../utils/geoUtils');
  return {
    ...actual,
    getCurrentLocation: vi.fn().mockResolvedValue('52.520008, 13.404954'),
    geocodeAddress: vi.fn().mockResolvedValue('48.137154, 11.576124'),
  };
});

vi.mock('../../../utils/imageCompression', async () => {
  return {
    compressAvatarToWebpDataUri: vi.fn().mockResolvedValue('data:image/webp;base64,mockavataruri'),
    compressAdImageToWebpDataUri: vi.fn().mockResolvedValue('data:image/webp;base64,mockaduri'),
  };
});

describe('ProfileCardEditor Component', () => {
  const mockMyProfile: FrontendMyMarketProfile = {
    profile: {
      id: 'did:key:z6MkuSelf',
      firstName: 'Max',
      lastName: 'Mustermann',
      organization: 'Mustermann Holz',
      community: 'Zimmerer e.V.',
      address: {
        street: 'Waldweg 1',
        zipCode: '12345',
        city: 'Holzhausen',
        country: 'DE',
      },
      email: 'max@holz.de',
      phone: '+49170123456',
      url: 'https://holz-mustermann.de',
    },
    visibility: {
      name: 'everyone',
      organization: 'everyone',
      community: 'everyone',
      city: 'everyone',
      url: 'everyone',
      picture: 'everyone',
      street: 'contacts',
      email: 'contacts',
      phone: 'contacts',
      coordinates: 'contacts',
      gender: 'contacts',
    },
    skills: [
      {
        title: 'Möbelbau',
        text: 'Tische und Stühle nach Maß',
        pricing: '50 Minuto / Std',
      },
    ],
    ads: [],
    maxHops: 2,
    allowInquiries: true,
    published: true,
    qrIsPartial: false,
  };

  it('renders editor form with all profile fields and visibility badges', () => {
    const onSave = vi.fn();
    const onClose = vi.fn();

    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={onClose}
        myProfile={mockMyProfile}
        onSave={onSave}
      />
    );

    expect(screen.getByRole('heading', { level: 2, name: /Visitenkarte bearbeiten|Edit business card/i })).toBeInTheDocument();

    // Check prefilled inputs
    expect(screen.getByDisplayValue('Max')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Mustermann')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Mustermann Holz')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Möbelbau')).toBeInTheDocument();

    // Check live preview has Max Mustermann
    expect(screen.getByText('Max Mustermann')).toBeInTheDocument();

    // Submit form
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      profile: expect.objectContaining({
        firstName: 'Max',
        lastName: 'Mustermann',
        organization: 'Mustermann Holz',
      }),
      visibility: expect.objectContaining({
        name: 'everyone',
        organization: 'everyone',
      }),
      skills: expect.arrayContaining([
        expect.objectContaining({ title: 'Möbelbau', is_remote: false }),
      ]),
      validityYears: 2,
    }));
  });

  it('applies business preset when clicked', () => {
    const onSave = vi.fn();

    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          visibility: {
            name: 'contacts',
            organization: 'nobody',
            community: 'nobody',
            city: 'nobody',
            url: 'nobody',
            picture: 'nobody',
            street: 'nobody',
            email: 'nobody',
            phone: 'nobody',
            coordinates: 'nobody',
            gender: 'nobody',
          },
        }}
        onSave={onSave}
      />
    );

    const presetCheckbox = screen.getByTestId('business-preset-checkbox');
    fireEvent.click(presetCheckbox);

    // Save
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      visibility: expect.objectContaining({
        organization: 'everyone',
        email: 'everyone',
        phone: 'everyone',
        url: 'everyone',
      }),
    }));
  });

  it('adds and removes skills up to 8 max', () => {
    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={mockMyProfile}
        onSave={vi.fn()}
      />
    );

    // Add skill
    const addSkillBtn = screen.getByRole('button', { name: /Fähigkeit hinzufügen|Add skill/i });
    fireEvent.click(addSkillBtn);

    const inputs = screen.getAllByPlaceholderText(/Fähigkeit \(z\. B\. Holzarbeiten|Skill \(e\.g\./i);
    expect(inputs.length).toBe(2);

    // Remove first skill
    const deleteBtns = screen.getAllByRole('button', { name: /Fähigkeit entfernen|Remove skill/i });
    fireEvent.click(deleteBtns[0]);

    const remainingInputs = screen.getAllByPlaceholderText(/Fähigkeit \(z\. B\. Holzarbeiten|Skill \(e\.g\./i);
    expect(remainingInputs.length).toBe(1);
  });

  it('supports GPS location lookup and switches coordinate visibility to everyone', async () => {
    const onSave = vi.fn();
    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          profile: { ...mockMyProfile.profile, coordinates: undefined },
          visibility: { ...mockMyProfile.visibility, coordinates: 'nobody' },
        }}
        onSave={onSave}
      />
    );

    // Recommended hint should be shown when coordinates are empty
    expect(screen.getByTestId('coordinates-recommended-hint')).toBeInTheDocument();

    // Click GPS button
    const gpsBtn = screen.getByTestId('fetch-gps-btn');
    fireEvent.click(gpsBtn);

    // Coordinate input should be populated and rounded to 2 decimals (~1 km)
    await screen.findByDisplayValue('52.52, 13.40');

    // Change precision to 3 (~100 m)
    const precisionSelect = screen.getByTestId('coord-precision-select');
    fireEvent.change(precisionSelect, { target: { value: '3' } });

    // Should re-round existing coordinates to 3 decimals
    expect(screen.getByDisplayValue('52.520, 13.400')).toBeInTheDocument();

    // Recommended hint disappears when coordinates are present and visibility is everyone
    expect(screen.queryByTestId('coordinates-recommended-hint')).not.toBeInTheDocument();
  });

  it('supports toggling is_remote on skills', () => {
    const onSave = vi.fn();
    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={mockMyProfile}
        onSave={onSave}
      />
    );

    const remoteCheckbox = screen.getByTestId('skill-remote-checkbox-0');
    expect(remoteCheckbox).not.toBeChecked();
    fireEvent.click(remoteCheckbox);
    expect(remoteCheckbox).toBeChecked();

    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      skills: [
        expect.objectContaining({
          title: 'Möbelbau',
          is_remote: true,
        }),
      ],
    }));
  });

  it('supports avatar upload via avatar-file-input and removal via avatar-remove-btn', async () => {
    const onSave = vi.fn();
    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={mockMyProfile}
        onSave={onSave}
      />
    );

    // Initial avatar input is present
    const fileInput = screen.getByTestId('avatar-file-input');
    expect(fileInput).toBeInTheDocument();

    // Trigger file change
    const dummyFile = new File(['dummy content'], 'avatar.jpg', { type: 'image/jpeg' });
    fireEvent.change(fileInput, { target: { files: [dummyFile] } });

    // Wait for remove button to appear when pictureUrl is set
    const removeBtn = await screen.findByTestId('avatar-remove-btn');
    expect(removeBtn).toBeInTheDocument();

    // Click remove button
    fireEvent.click(removeBtn);
    expect(screen.queryByTestId('avatar-remove-btn')).not.toBeInTheDocument();

    // Save and verify pictureUrl is undefined
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      profile: expect.objectContaining({
        pictureUrl: undefined,
      }),
    }));
  });
});
