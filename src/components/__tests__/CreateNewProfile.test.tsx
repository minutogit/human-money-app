import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { CreateNewProfile } from '../CreateNewProfile';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('CreateNewProfile Component', () => {
  const mockOnProfileCreated = vi.fn();
  const mockOnSwitchToRecreate = vi.fn();
  const mockOnSwitchToLogin = vi.fn();

  const mockSeed12 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon';
  const mockSeed24 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem yellow zebra';

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'generate_mnemonic') {
        if (args?.wordCount === 24) {
          return Promise.resolve(mockSeed24);
        }
        return Promise.resolve(mockSeed12);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'create_profile') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
  });

  describe('Initialization & Seed Generation', () => {
    it('renders the initial seed display step', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Step 1: Your Secret Seed Phrase')).toBeInTheDocument();
      });
    });

    it('calls generate_mnemonic on component mount', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('generate_mnemonic', {
          wordCount: 12,
          language: 'english',
        });
      });
    });

    it('displays 12 seed words correctly', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const words = mockSeed12.split(' ');
        words.forEach((word, index) => {
          expect(screen.getByTestId(`word-display-${index}`)).toHaveTextContent(new RegExp(`${index + 1}\\s*${word}`));
        });
      });
    });

    it('regenerates seed when word count changes from 12 to 24', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByDisplayValue('12 Words (Standard Security)')).toBeInTheDocument();
      });

      const wordCountSelect = screen.getByDisplayValue('12 Words (Standard Security)');
      fireEvent.change(wordCountSelect, { target: { value: '24' } });

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('generate_mnemonic', {
          wordCount: 24,
          language: 'english',
        });
      });
    });

    it('regenerates seed when language changes', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByDisplayValue('English')).toBeInTheDocument();
      });

      const languageSelect = screen.getByDisplayValue('English');
      fireEvent.change(languageSelect, { target: { value: 'german' } });

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('generate_mnemonic', {
          wordCount: 12,
          language: 'german',
        });
      });
    });
  });

  describe('Step 1 -> 2 Navigation', () => {
    it('navigates to confirmation step when clicking "I\'ve Saved My Seed Phrase"', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText("I've Saved My Seed Phrase")).toBeInTheDocument();
      });

      const nextButton = screen.getByText("I've Saved My Seed Phrase");
      fireEvent.click(nextButton);

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });
    });

    it('disables button while loading', async () => {
      (invoke as Mock).mockImplementation(() => new Promise(resolve => setTimeout(() => resolve(mockSeed12), 100)));

      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).toBeDisabled();
      });
    });
  });

  describe('Step 2: Seed Confirmation (Standard Mode)', () => {
    beforeEach(() => {
      vi.clearAllMocks();
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'generate_mnemonic') return Promise.resolve(mockSeed12);
        if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
        if (cmd === 'create_profile') return Promise.resolve(undefined);
        return Promise.resolve(undefined);
      });
    });

    it('shows 3 random word inputs for confirmation', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
        // Should have 3 word input fields
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(3);
      });
    });

    it('shows error when confirmation words are incorrect', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });

      // Fill with wrong words
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input) => {
        fireEvent.change(input, { target: { value: 'wrongword' } });
      });

      await waitFor(() => {
        const confirmButton = screen.getByText('Confirm Seed');
        expect(confirmButton).not.toBeDisabled();
        fireEvent.click(confirmButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText(/Error: One or more words are incorrect/i)).toBeInTheDocument();
      }, { timeout: 3000 });
    });

    it('proceeds to step 3 when confirmation words are correct', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });

      // Get the word numbers being asked
      const wordLabels = screen.getAllByTestId(/word-input-/);
      const words = mockSeed12.split(' ');
      
      // Fill with correct words based on the labels
      wordLabels.forEach((input) => {
        const testId = input.getAttribute('data-testid') || '';
        const indexMatch = testId.match(/word-input-(\d+)/);
        if (indexMatch) {
          const wordIndex = parseInt(indexMatch[1]);
          fireEvent.change(input, { target: { value: words[wordIndex] } });
        }
      });

      await waitFor(() => {
        const confirmButton = screen.getByText('Confirm Seed');
        expect(confirmButton).not.toBeDisabled();
        fireEvent.click(confirmButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 3: Secure Your Wallet')).toBeInTheDocument();
      }, { timeout: 4000 });
    });
  });

  describe('Step 2: Seed Confirmation (Bulk Mode)', () => {
    beforeEach(() => {
      vi.clearAllMocks();
      (invoke as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'generate_mnemonic') return Promise.resolve(mockSeed12);
        if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
        if (cmd === 'create_profile') return Promise.resolve(undefined);
        return Promise.resolve(undefined);
      });
    });

    it('switches to bulk mode when clicking toggle', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });

      const bulkToggle = await screen.findByText(/Switch to Bulk Paste/i);
      fireEvent.click(bulkToggle);

      await waitFor(() => {
        expect(screen.getByLabelText(/Paste Full Seed Phrase/i)).toBeInTheDocument();
        expect(screen.getByPlaceholderText(/Paste here/i)).toBeInTheDocument();
      });
    });

    it('auto-cleans bulk input with numbers and punctuation', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });

      const bulkToggle = await screen.findByText(/Switch to Bulk Paste/i);
      fireEvent.click(bulkToggle);

      await waitFor(() => {
        expect(screen.getByLabelText(/Paste Full Seed Phrase/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste here/i);
      fireEvent.change(textarea, { 
        target: { value: '1. apple 2. banana 3. cherry 4. date 5. elderberry 6. fig 7. grape 8. honey 9. iris 10. jasmine 11. kiwi 12. lemon' } 
      });

      await waitFor(() => {
        // The textarea should be cleaned
        expect(textarea).toHaveValue(mockSeed12);
      });
    });

    it('shows error when bulk phrase does not match', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Confirm Your Seed Phrase')).toBeInTheDocument();
      });

      const bulkToggle = await screen.findByText(/Switch to Bulk Paste/i);
      fireEvent.click(bulkToggle);

      await waitFor(() => {
        expect(screen.getByLabelText(/Paste Full Seed Phrase/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste here/i);
      fireEvent.change(textarea, { target: { value: 'wrong phrase words here' } });

      await waitFor(() => {
        const confirmButton = screen.getByText('Confirm Seed');
        expect(confirmButton).not.toBeDisabled();
        fireEvent.click(confirmButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText(/Error: The phrase does not match/i)).toBeInTheDocument();
      });
    });
  });

  describe('Navigation Buttons', () => {
    it('calls onSwitchToRecreate when clicking recreate link', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText(/Create profile here/i)).toBeInTheDocument();
      });

      const recreateLink = screen.getByText(/Create profile here/i);
      fireEvent.click(recreateLink);

      expect(mockOnSwitchToRecreate).toHaveBeenCalled();
    });

    it('calls onSwitchToLogin when clicking back button', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Back to Login')).toBeInTheDocument();
      });

      const backButton = screen.getByText('Back to Login');
      fireEvent.click(backButton);

      expect(mockOnSwitchToLogin).toHaveBeenCalled();
    });

    it('shows back button in step 2', async () => {
      render(
        <CreateNewProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToRecreate={mockOnSwitchToRecreate}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const nextButton = screen.getByText("I've Saved My Seed Phrase");
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Back')).toBeInTheDocument();
      });

      const backButton = screen.getByText('Back');
      fireEvent.click(backButton);

      await waitFor(() => {
        expect(screen.getByText('Step 1: Your Secret Seed Phrase')).toBeInTheDocument();
      });
    });
  });
});
