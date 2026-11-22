import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ProfileCardEditor } from '../ProfileCardEditor';
import { UiModeProvider } from '../../../context/UiModeContext';
import { FrontendMyMarketProfile } from '../../../types';

vi.mock('../../../utils/geoUtils', async () => {
  const actual = await vi.importActual<typeof import('../../../utils/geoUtils')>('../../../utils/geoUtils');
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

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('renders editor form with all profile fields and presets in Simple mode', async () => {
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

    // Simple mode shows PresetOrCustom options instead of 11 individual visibility buttons
    expect(screen.getByTestId('preset-option-private')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-local')).toBeInTheDocument();
    expect(screen.getByTestId('preset-option-business')).toBeInTheDocument();
    expect(screen.queryByTestId('visibility-btn-name')).not.toBeInTheDocument();

    // Check live preview has Max Mustermann
    expect(screen.getByText('Max Mustermann')).toBeInTheDocument();

    // Submit form
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        profile: expect.objectContaining({
          firstName: 'Max',
          lastName: 'Mustermann',
          organization: 'Mustermann Holz',
        }),
        visibility: expect.objectContaining({
          name: 'everyone',
          organization: 'everyone',
          street: 'contacts',
        }),
        skills: expect.arrayContaining([
          expect.objectContaining({ title: 'Möbelbau', isRemote: false }),
        ]),
        validityYears: 2,
      }));
    });
  });

  it('applies business preset via PresetOrCustom and keeps street off everyone', async () => {
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

    const presetBtn = screen.getByTestId('preset-option-business');
    fireEvent.click(presetBtn);

    // Save
    const saveBtn = screen.getByRole('button', { name: /^Speichern$|^Save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        visibility: expect.objectContaining({
          organization: 'everyone',
          email: 'everyone',
          phone: 'everyone',
          url: 'everyone',
          street: 'contacts',
          gender: 'nobody',
        }),
      }));
    });
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

  it('supports GPS location lookup, guards public precision >= ~1 km, and restricts street off everyone in Expert mode', async () => {
    const onSave = vi.fn();
    render(
      <UiModeProvider initialMode="expert">
        <ProfileCardEditor
          isOpen={true}
          onClose={vi.fn()}
          myProfile={{
            ...mockMyProfile,
            profile: { ...mockMyProfile.profile, coordinates: undefined },
            visibility: { ...mockMyProfile.visibility, coordinates: 'nobody', street: 'contacts' },
          }}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    // Recommended hint should be shown when coordinates are empty
    expect(screen.getByTestId('coordinates-recommended-hint')).toBeInTheDocument();

    // Click GPS button -> coordinates populated rounded to 2 decimals (~1 km), visibility becomes 'everyone'
    const gpsBtn = screen.getByTestId('fetch-gps-btn');
    fireEvent.click(gpsBtn);

    await screen.findByDisplayValue('52.52, 13.40');

    // Options 3 and 4 (~100 m / ~10 m) must be disabled while coordinates visibility is 'everyone'
    const precisionSelect = screen.getByTestId('coord-precision-select') as HTMLSelectElement;
    expect(precisionSelect.querySelector('option[value="3"]')).toBeDisabled();
    expect(precisionSelect.querySelector('option[value="4"]')).toBeDisabled();

    // Recommended hint disappears when coordinates are present and visibility is everyone
    expect(screen.queryByTestId('coordinates-recommended-hint')).not.toBeInTheDocument();

    // Switching coordinate visibility from 'everyone' to 'contacts' enables precision 3
    fireEvent.click(screen.getByTestId('visibility-btn-coordinates'));
    expect(precisionSelect.querySelector('option[value="3"]')).not.toBeDisabled();
    fireEvent.change(precisionSelect, { target: { value: '3' } });
    expect(screen.getByDisplayValue('52.520, 13.400')).toBeInTheDocument();

    // Street visibility button cycles only contacts <-> nobody (never everyone)
    const streetBtn = screen.getByTestId('visibility-btn-street');
    expect(streetBtn).toHaveTextContent(/Nur Kontakte|Contacts only/i);
    fireEvent.click(streetBtn);
    expect(streetBtn).toHaveTextContent(/Niemand|Nobody/i);
    fireEvent.click(streetBtn);
    expect(streetBtn).toHaveTextContent(/Nur Kontakte|Contacts only/i);
  });

  it('supports toggling isRemote on skills', async () => {
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

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        skills: [
          expect.objectContaining({
            title: 'Möbelbau',
            isRemote: true,
          }),
        ],
      }));
    });
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

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        profile: expect.objectContaining({
          pictureUrl: undefined,
        }),
      }));
    });
  });

  it('shows prefix card limit notice only when active profile ID has a prefix', () => {
    const { rerender } = render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={mockMyProfile}
        onSave={vi.fn()}
      />
    );

    expect(screen.queryByTestId('prefix-card-limit-notice')).not.toBeInTheDocument();

    rerender(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          profile: {
            ...mockMyProfile.profile,
            id: 'laden:abc@did:key:z6MkuSelf',
          },
        }}
        onSave={vi.fn()}
      />
    );

    const notice = screen.getByTestId('prefix-card-limit-notice');
    expect(notice).toBeInTheDocument();
    expect(notice).toHaveTextContent('laden');
  });

  it('distinguishes ~1 km coarse geocoding privacy, 1-click hide city/ZIP, and precise OSM notice', async () => {
    const geoModule = await import('../../../utils/geoUtils');
    const geocodeMock = vi.mocked(geoModule.geocodeAddress);
    geocodeMock.mockClear();

    const onSave = vi.fn();
    render(
      <UiModeProvider initialMode="expert">
        <ProfileCardEditor
          isOpen={true}
          onClose={vi.fn()}
          myProfile={mockMyProfile}
          onSave={onSave}
        />
      </UiModeProvider>
    );

    // ~1 km mode shows coarse inaccuracy hint and coarse OSM note
    expect(screen.getByTestId('coord-coarse-osm-note')).toBeInTheDocument();
    expect(screen.getByTestId('coord-coarse-hint')).toBeInTheDocument();
    expect(screen.queryByTestId('coord-osm-precise-notice')).not.toBeInTheDocument();

    // Clicking geocode in ~1 km mode must NOT send street or houseNumber
    fireEvent.click(screen.getByTestId('geocode-address-btn'));
    await waitFor(() => {
      expect(geocodeMock).toHaveBeenCalledWith(
        { zipCode: '12345', city: 'Holzhausen', country: 'DE' },
        2,
        { coarseOnly: true },
      );
    });

    // Once coordinates are set, 1-click button hides PLZ & Ort (visibility.city = 'nobody')
    const hideBtn = await screen.findByTestId('hide-city-zip-btn');
    fireEvent.click(hideBtn);

    // Switching to ~100 m precision shows OpenStreetMap street transfer notice and passes full address
    geocodeMock.mockClear();
    fireEvent.change(screen.getByTestId('coord-precision-select'), { target: { value: '3' } });
    expect(screen.getByTestId('coord-osm-precise-notice')).toBeInTheDocument();
    expect(screen.queryByTestId('coord-coarse-hint')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('geocode-address-btn'));
    await waitFor(() => {
      expect(geocodeMock).toHaveBeenCalledWith(
        mockMyProfile.profile.address,
        6,
        { coarseOnly: false },
      );
    });

    fireEvent.click(screen.getByRole('button', { name: /^Speichern$|^Save$/i }));
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(
        expect.objectContaining({
          visibility: expect.objectContaining({ city: 'nobody' }),
        }),
      );
    });
  });

  it('background auto-geocode on save never sends street and skips when coordinates visibility is nobody', async () => {
    const geoModule = await import('../../../utils/geoUtils');
    const geocodeMock = vi.mocked(geoModule.geocodeAddress);
    geocodeMock.mockClear();

    const onSave = vi.fn();
    const { unmount } = render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          visibility: { ...mockMyProfile.visibility, coordinates: 'nobody' },
        }}
        onSave={onSave}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /^Speichern$|^Save$/i }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(geocodeMock).not.toHaveBeenCalled();

    unmount();
    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={mockMyProfile}
        onSave={onSave}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /^Speichern$|^Save$/i }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(2));
    expect(geocodeMock).toHaveBeenCalledWith(
      { zipCode: '12345', city: 'Holzhausen', country: 'DE' },
      2,
      { coarseOnly: true },
    );
  });

  it('S11-X1: blocks unsafe avatar URIs in ProfileCardMediaSection and ProfileCardPreview while rendering valid base64 image data URIs', () => {
    const { unmount: unmountHttps } = render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          profile: {
            ...mockMyProfile.profile,
            pictureUrl: 'https://tracker.evil/avatar.png',
          },
        }}
        onSave={vi.fn()}
      />
    );
    expect(screen.queryAllByAltText('Avatar')).toHaveLength(0);
    unmountHttps();

    const { unmount: unmountSvg } = render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          profile: {
            ...mockMyProfile.profile,
            pictureUrl: 'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=',
          },
        }}
        onSave={vi.fn()}
      />
    );
    expect(screen.queryAllByAltText('Avatar')).toHaveLength(0);
    unmountSvg();

    render(
      <ProfileCardEditor
        isOpen={true}
        onClose={vi.fn()}
        myProfile={{
          ...mockMyProfile,
          profile: {
            ...mockMyProfile.profile,
            pictureUrl: 'data:image/webp;base64,UklGRkAAAABXRUJQ',
          },
        }}
        onSave={vi.fn()}
      />
    );
    expect(screen.getAllByAltText('Avatar')).toHaveLength(2);
  });

  it('S11-X2: profileService.saveProfile preserves validityYears and published state', async () => {
    const { marketService } = await import('../../../services/marketService');
    const { profileService } = await import('../../../services/profileService');

    const getSpy = vi.spyOn(marketService, 'getMyMarketProfile').mockResolvedValue({
      ...mockMyProfile,
      validityYears: 3,
      published: true,
    });
    const saveSpy = vi.spyOn(marketService, 'saveMyMarketProfile').mockResolvedValue({
      ...mockMyProfile,
      validityYears: 3,
      published: true,
    });

    try {
      await profileService.saveProfile({ id: 'did:key:z6MkuSelf', firstName: 'Moritz' }, 'pw123');
      expect(saveSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          validityYears: 3,
          published: true,
          profile: expect.objectContaining({ firstName: 'Moritz' }),
        }),
        'pw123',
      );
    } finally {
      getSpy.mockRestore();
      saveSpy.mockRestore();
    }
  });
});
