import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SupportView } from '../SupportView';
import { openUrl } from '@tauri-apps/plugin-opener';

// Mock the Tauri opener plugin
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('SupportView Component', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders support title and description', () => {
    render(<SupportView />);

    expect(screen.getAllByText('Menschlich Miteinander').length).toBeGreaterThan(0);
    expect(screen.getByText(/This app is supported by the association/i)).toBeInTheDocument();
  });

  it('renders the CTA button and redirects on click', async () => {
    render(<SupportView />);

    const ctaButton = screen.getByRole('button', { name: /To the Association Website/i });
    expect(ctaButton).toBeInTheDocument();

    fireEvent.click(ctaButton);

    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith('https://menschlich-miteinander.org');
    });
  });

  it('renders donation, membership and participation sections', () => {
    render(<SupportView />);

    expect(screen.getByText('Donations')).toBeInTheDocument();
    expect(screen.getByText('Membership')).toBeInTheDocument();
    expect(screen.getByText('Participation')).toBeInTheDocument();
  });
});
