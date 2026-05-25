import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { BugReportView } from '../BugReportView';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';

// Mock the Tauri opener plugin
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('BugReportView Component', () => {
  const mockOnBack = vi.fn();
  const mockLogs = '2026-05-25 INFO: Application started\n2026-05-25 WARN: Low memory';
  const writeTextMock = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockReset();
    
    // Setup clipboard mock
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });
    writeTextMock.mockClear();
  });

  it('renders loading state initially and then shows logs', async () => {
    (invoke as Mock).mockResolvedValue(mockLogs);

    render(<BugReportView onBack={mockOnBack} />);

    // Verify loading indicator is displayed first
    expect(screen.getByText(/Loading Logs.../i)).toBeInTheDocument();

    // Verify logs and elements are rendered after fetching
    await waitFor(() => {
      const preElement = screen.getByText((content, element) => {
        return element?.tagName.toLowerCase() === 'pre' && content.includes('Application started');
      });
      expect(preElement).toBeInTheDocument();
      expect(preElement.textContent).toBe(mockLogs);
    });

    expect(screen.getByText('Report a Bug')).toBeInTheDocument();
    expect(screen.getByText('Privacy Warning')).toBeInTheDocument();
  });

  it('invokes get_latest_logs on mount', async () => {
    (invoke as Mock).mockResolvedValue(mockLogs);

    render(<BugReportView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_latest_logs');
    });
  });

  it('handles errors when fetching logs', async () => {
    (invoke as Mock).mockRejectedValue(new Error('Failed to read logs'));

    render(<BugReportView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText('Failed to load logs')).toBeInTheDocument();
    });
  });

  it('copies logs to clipboard when copy button is clicked', async () => {
    (invoke as Mock).mockResolvedValue(mockLogs);

    render(<BugReportView onBack={mockOnBack} />);

    // Wait for logs to load
    const copyButton = await screen.findByRole('button', { name: /Copy Logs/i });

    fireEvent.click(copyButton);

    expect(writeTextMock).toHaveBeenCalledWith(mockLogs);
    
    // Check that button shows "Copied!" feedback
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Copied!/i })).toBeInTheDocument();
    });
  });

  it('opens github issues URL when open issue button is clicked', async () => {
    (invoke as Mock).mockResolvedValue(mockLogs);

    render(<BugReportView onBack={mockOnBack} />);

    const openIssueButton = await screen.findByRole('button', { name: /Open GitHub Issue/i });

    fireEvent.click(openIssueButton);

    expect(openUrl).toHaveBeenCalledWith('https://github.com/minutogit/human-money-app/issues/new');
  });

  it('calls onBack when the back button is clicked', async () => {
    (invoke as Mock).mockResolvedValue(mockLogs);

    render(<BugReportView onBack={mockOnBack} />);

    // PageLayout rendering header back button is a button with Title="Back"
    const backButton = await screen.findByTitle('Back');
    fireEvent.click(backButton);

    expect(mockOnBack).toHaveBeenCalled();
  });
});
