import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ProfileSettings } from '../ProfileSettings';
import { profileService } from '../../services/profileService';
import { getCurrentLocation, geocodeAddress } from '../../utils/geoUtils';

// Mock dependecies
vi.mock('../../services/profileService', () => ({
  profileService: {
    getProfile: vi.fn(),
    saveProfile: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: vi.fn(() => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  })),
}));

vi.mock('../../utils/geoUtils', () => ({
  normalizeCoordinates: vi.fn((coords) => coords),
  geocodeAddress: vi.fn(),
  getCurrentLocation: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('ProfileSettings Component', () => {
  const mockProfile = {
    protocol_version: 1,
    id: 'test-id',
    firstName: 'Jane',
    lastName: 'Doe',
    gender: '2',
    organization: 'Jane Org',
    community: 'Jane Community',
    email: 'jane@example.com',
    phone: '987654321',
    url: 'https://jane.com',
    coordinates: '52.52, 13.40',
    serviceOffer: 'Offer help',
    needs: 'Need help',
    address: {
      street: 'Jane Street',
      houseNumber: '42',
      zipCode: '10115',
      city: 'Berlin',
      country: 'Germany',
      fullAddress: 'Jane Street, 42, 10115, Berlin, Germany',
    },
    pictureUrl: 'https://jane.com/pic.jpg',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (profileService.getProfile as Mock).mockResolvedValue({ ...mockProfile });
    (profileService.saveProfile as Mock).mockResolvedValue(undefined);
  });

  it('renders loading state initially', () => {
    render(<ProfileSettings />);
    expect(screen.getByText(/Loading Profile\.\.\./i)).toBeInTheDocument();
  });

  it('loads and populates profile fields', async () => {
    render(<ProfileSettings />);

    await waitFor(() => {
      expect(screen.queryByText(/Loading Profile\.\.\./i)).not.toBeInTheDocument();
    });

    expect(screen.getByPlaceholderText('e.g. Alice')).toHaveValue('Jane');
    expect(screen.getByPlaceholderText('e.g. Smith')).toHaveValue('Doe');
    expect(screen.getByPlaceholderText('Organization Name')).toHaveValue('Jane Org');
    expect(screen.getByPlaceholderText('Community Name')).toHaveValue('Jane Community');
    expect(screen.getByPlaceholderText('Street Address')).toHaveValue('Jane Street');
    expect(screen.getByPlaceholderText('123')).toHaveValue('42');
    expect(screen.getByPlaceholderText('12345')).toHaveValue('10115');
    expect(screen.getByPlaceholderText('City Name')).toHaveValue('Berlin');
    expect(screen.getByPlaceholderText('Country Name')).toHaveValue('Germany');
    expect(screen.getByPlaceholderText('51.16, 10.45 or Maps Link')).toHaveValue('52.52, 13.40');
    expect(screen.getByPlaceholderText('identity@domain.com')).toHaveValue('jane@example.com');
    expect(screen.getByPlaceholderText('+49 000 000000')).toHaveValue('987654321');
    expect(screen.getByPlaceholderText('https://profile.com')).toHaveValue('https://jane.com');
  });

  it('allows editing fields and calls saveProfile on submit', async () => {
    const user = userEvent.setup();
    render(<ProfileSettings />);

    const firstNameInput = await screen.findByPlaceholderText('e.g. Alice');
    fireEvent.change(firstNameInput, { target: { value: 'Janet' } });
    expect(firstNameInput).toHaveValue('Janet');

    const updateButton = screen.getByRole('button', { name: /Update Profile/i });
    await user.click(updateButton);

    await waitFor(() => {
      expect(profileService.saveProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          firstName: 'Janet',
          lastName: 'Doe',
        }),
        'test-password'
      );
    });

    expect(screen.getByText(/Profile updated!/i)).toBeInTheDocument();
  });

  it('supports GPS location auto-fill', async () => {
    const user = userEvent.setup();
    (getCurrentLocation as Mock).mockResolvedValue('51.16, 10.45');
    render(<ProfileSettings />);

    const gpsButton = await screen.findByRole('button', { name: /GPS/i });
    await user.click(gpsButton);

    await waitFor(() => {
      expect(getCurrentLocation).toHaveBeenCalled();
      expect(screen.getByPlaceholderText('51.16, 10.45 or Maps Link')).toHaveValue('51.16, 10.45');
    });
  });

  it('supports address geocoding auto-fill', async () => {
    const user = userEvent.setup();
    (geocodeAddress as Mock).mockResolvedValue('52.52, 13.40');
    render(<ProfileSettings />);

    const geocodeButton = await screen.findByRole('button', { name: /Auto-Address/i });
    await user.click(geocodeButton);

    await waitFor(() => {
      expect(geocodeAddress).toHaveBeenCalledWith(expect.objectContaining({
        city: 'Berlin',
      }));
      expect(screen.getByPlaceholderText('51.16, 10.45 or Maps Link')).toHaveValue('52.52, 13.40');
    });
  });

  it('displays error message on failed save', async () => {
    const user = userEvent.setup();
    (profileService.saveProfile as Mock).mockRejectedValue(new Error('Backend offline'));
    render(<ProfileSettings />);

    await screen.findByPlaceholderText('e.g. Alice');
    const updateButton = screen.getByRole('button', { name: /Update Profile/i });
    await user.click(updateButton);

    expect(await screen.findByText(/Failed to update profile: Backend offline/i)).toBeInTheDocument();
  });
});
