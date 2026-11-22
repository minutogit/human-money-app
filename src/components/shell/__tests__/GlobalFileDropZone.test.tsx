import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { GlobalFileDropZone } from '../GlobalFileDropZone';

vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    appState: { view: 'wallet' },
    navigate: vi.fn(),
    setPendingDroppedFile: vi.fn(),
  }),
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    isAuthenticated: true,
    protectAction: vi.fn(),
  }),
}));

describe('GlobalFileDropZone', () => {
  it('shows localized common.fileTooLarge message when dropped file exceeds 64 MiB', async () => {
    render(
      <GlobalFileDropZone>
        <div data-testid="drop-child">content</div>
      </GlobalFileDropZone>
    );

    const dropRoot = screen.getByTestId('drop-child').parentElement!;
    const oversizedFile = {
      name: 'huge.hmcbackup',
      size: 64 * 1024 * 1024 + 1,
      arrayBuffer: vi.fn(),
    } as unknown as File;

    fireEvent.drop(dropRoot, {
      dataTransfer: {
        types: ['Files'],
        getData: () => '',
        files: [oversizedFile],
      },
    });

    await waitFor(() => {
      expect(
        screen.getByText(/File exceeds the maximum supported size \(64 MiB\)/i)
      ).toBeInTheDocument();
    });
    expect(screen.queryByText(/^Error$/)).not.toBeInTheDocument();
  });
});
