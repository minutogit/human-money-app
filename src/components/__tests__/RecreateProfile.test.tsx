import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { RecreateProfile } from '../RecreateProfile';
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

describe('RecreateProfile Component', () => {
  const mockOnProfileCreated = vi.fn();
  const mockOnSwitchToLogin = vi.fn();

  const mockSeed12 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon';
  const mockSeed24 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem yellow zebra';
  const mockWordlist = ['apple', 'banana', 'cherry', 'date', 'elderberry', 'fig', 'grape', 'honey', 'iris', 'jasmine', 'kiwi', 'lemon', 'mango', 'nut', 'olive', 'peach', 'quince', 'raspberry', 'strawberry', 'tangerine', 'umbrella', 'violet', 'watermelon', 'xylem', 'yellow', 'zebra'];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_bip39_wordlist') {
        return Promise.resolve(mockWordlist);
      }
      if (cmd === 'validate_mnemonic') {
        const mnemonic = args?.mnemonic || '';
        // const words = mnemonic.split(' ').filter(Boolean);
        // Simple validation: check if it's the mock seed
        if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
          return Promise.resolve(true);
        }
        return Promise.reject(new Error('Invalid mnemonic'));
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

  describe('Initialization', () => {
    it('renders the seed import step', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Step 1: Your Seed Phrase')).toBeInTheDocument();
      });
    });

    it('calls get_bip39_wordlist on component mount', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('get_bip39_wordlist', {
          language: 'english',
        });
      });
    });

    it('refetches wordlist when language changes', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByDisplayValue('English')).toBeInTheDocument();
      });

      const languageSelect = screen.getByDisplayValue('English');
      fireEvent.change(languageSelect, { target: { value: 'german' } });

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('get_bip39_wordlist', {
          language: 'german',
        });
      });
    });
  });

  describe('Step 1: Seed Input (Words Mode)', () => {
    it('shows 12 word input fields by default', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(12);
      });
    });

    it('adjusts to 24 word inputs when word count changes', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByDisplayValue('12 Words')).toBeInTheDocument();
      });

      const wordCountSelect = screen.getByDisplayValue('12 Words');
      fireEvent.change(wordCountSelect, { target: { value: '24' } });

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(24);
      });
    });

    it('validates mnemonic live and shows success message', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(12);
      });

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('validates mnemonic live and shows error message', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(12);
      });

      // Fill with invalid seed
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input) => {
        fireEvent.change(input, { target: { value: 'wrongword' } });
      });

      await waitFor(() => {
        expect(screen.getByText(/Error: Seed phrase is not valid/i)).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('enables Next button only when mnemonic is valid', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).toBeDisabled();
      });

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
      }, { timeout: 1000 });
    });

    it('handles paste of multiple words in single field', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(12);
      });

      // Paste multiple words into first field
      const firstInput = screen.getAllByRole('textbox')[0];
      fireEvent.change(firstInput, { target: { value: 'apple banana cherry date' } });

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs[0]).toHaveValue('apple');
        expect(wordInputs[1]).toHaveValue('banana');
        expect(wordInputs[2]).toHaveValue('cherry');
        expect(wordInputs[3]).toHaveValue('date');
      });
    });

    it('cleans numbered prefixes from pasted text', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBe(12);
      });

      // Paste with numbered prefixes
      const firstInput = screen.getAllByRole('textbox')[0];
      fireEvent.change(firstInput, { target: { value: '1. apple 2. banana 3. cherry' } });

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs[0]).toHaveValue('apple');
        expect(wordInputs[1]).toHaveValue('banana');
        expect(wordInputs[2]).toHaveValue('cherry');
      });
    });
  });

  describe('Step 1: Seed Input (Phrase Mode)', () => {
    it('switches to phrase mode when clicking toggle', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Enter full phrase')).toBeInTheDocument();
      });

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });
    });

    it('auto-cleans phrase input with numbers and punctuation', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste full seed phrase here/i);
      fireEvent.change(textarea, { 
        target: { value: '1. apple 2. banana 3. cherry 4. date 5. elderberry 6. fig 7. grape 8. honey 9. iris 10. jasmine 11. kiwi 12. lemon' } 
      });

      await waitFor(() => {
        expect(textarea).toHaveValue(mockSeed12);
      });
    });

    it('handles newlines and tabs in phrase input', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste full seed phrase here/i);
      fireEvent.change(textarea, { 
        target: { value: 'apple\tbanana\ncherry\r\ndate\telderberry' } 
      });

      await waitFor(() => {
        expect(textarea).toHaveValue('apple banana cherry date elderberry');
      });
    });

    it('auto-completes word when there is a single suggestion', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste full seed phrase here/i);
      
      // Type partial word that has single completion
      fireEvent.change(textarea, { target: { value: 'app' } });

      await waitFor(() => {
        // Should auto-complete to 'apple'
        expect(textarea).toHaveValue('apple');
      }, { timeout: 1000 });
    });

    it('validates phrase input live', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste full seed phrase here/i);
      fireEvent.change(textarea, { target: { value: mockSeed12 } });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('switches back to words mode preserving input', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Paste full seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Paste full seed phrase here/i);
      fireEvent.change(textarea, { target: { value: mockSeed12 } });

      const wordsToggle = screen.getByText('Use single fields');
      fireEvent.click(wordsToggle);

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        const words = mockSeed12.split(' ');
        wordInputs.forEach((input, index) => {
          expect(input).toHaveValue(words[index]);
        });
      });
    });
  });

  describe('Step 1 -> 2 Navigation', () => {
    it('shows error when trying to proceed with invalid mnemonic', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).toBeDisabled();
      });

      // Try to click disabled button (should not navigate)
      const nextButton = screen.getByText('Next');
      fireEvent.click(nextButton);

      expect(screen.getByText('Step 1: Your Seed Phrase')).toBeInTheDocument();
    });

    it('navigates to step 2 when mnemonic is valid and Next is clicked', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      const nextButton = screen.getByText('Next');
      fireEvent.click(nextButton);

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });
    });
  });

  describe('Step 2: Profile Details', () => {
    beforeEach(() => {
      vi.clearAllMocks();
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'get_bip39_wordlist') return Promise.resolve(mockWordlist);
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
        if (cmd === 'create_profile') return Promise.resolve(undefined);
        return Promise.resolve(undefined);
      });
    });

    it('shows profile name, password, and user prefix fields', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill seed and navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      const nextButton = screen.getByText('Next');
      fireEvent.click(nextButton);

      await waitFor(() => {
        expect(screen.getByLabelText(/Profile Name/i)).toBeInTheDocument();
        expect(screen.getByPlaceholderText('Minimum 8 characters')).toBeInTheDocument();
        expect(screen.getByLabelText(/Device Prefix/i)).toBeInTheDocument();
      });
    });

    it('shows error when passwords do not match', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      // Fill form with mismatched passwords
      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password456' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passwords do not match/i)).toBeInTheDocument();
      });
    });

    it('shows error when password is too short', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'short' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'short' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: Password must be at least 8 characters/i)).toBeInTheDocument();
      });
    });

    it('shows error when passphrases do not match in Step 1', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Enter mismatched passphrases in Step 1
      const passphraseInput = screen.getByPlaceholderText(/Enter extra security word/i);
      fireEvent.change(passphraseInput, { target: { value: 'pass1' } });
      
      const confirmPassphraseInput = screen.getByPlaceholderText(/Verify your extra word/i);
      fireEvent.change(confirmPassphraseInput, { target: { value: 'pass2' } });

      const nextButton = screen.getByText('Next');
      fireEvent.click(nextButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passphrases do not match/i)).toBeInTheDocument();
      });
    });



    it('focuses password input on validation error', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password456' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passwords do not match/i)).toBeInTheDocument();
        expect(passwordInput).toHaveFocus();
      });
    });

    it('clears error message when focusing on password field', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password456' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passwords do not match/i)).toBeInTheDocument();
      });

      // Focus on password field
      fireEvent.focus(passwordInput);

      await waitFor(() => {
        expect(screen.queryByText(/Error: The passwords do not match/i)).not.toBeInTheDocument();
      });
    });

    it('successfully creates profile with all fields and calls callback', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      // Fill form correctly
      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('create_profile', {
          profileName: 'Test Profile',
          mnemonic: mockSeed12,
          passphrase: undefined,
          userPrefix: 'my_device',
          password: 'password123',
          localInstanceId: 'test-instance-id',
          language: 'english',
        });
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(mockOnProfileCreated).toHaveBeenCalled();
      }, { timeout: 3000 });
    });



    it('handles backend error during profile creation', async () => {
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'get_bip39_wordlist') return Promise.resolve(mockWordlist);
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
        if (cmd === 'create_profile') return Promise.reject(new Error('Backend error'));
        return Promise.resolve(undefined);
      });

      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      // Fill form correctly
      const profileNameInput = screen.getByPlaceholderText(/e.g., 'My Main Wallet'/i);
      fireEvent.change(profileNameInput, { target: { value: 'Test Profile' } });

      const userPrefixInput = screen.getByPlaceholderText(/e.g. 'my_laptop', '0', or 'pc'/i);
      fireEvent.change(userPrefixInput, { target: { value: 'my_device' } });

      const passwordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(passwordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const createButton = screen.getByText('Create & Encrypt Profile');
      fireEvent.click(createButton);

      await waitFor(() => {
        expect(screen.getByText(/Error creating profile/i)).toBeInTheDocument();
      }, { timeout: 3000 });

      expect(mockOnProfileCreated).not.toHaveBeenCalled();
    });
  });

  describe('Navigation', () => {
    it('calls onSwitchToLogin when clicking Back to Login', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
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

    it('shows back button in step 2 and returns to step 1', async () => {
      render(
        <RecreateProfile
          onProfileCreated={mockOnProfileCreated}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Navigate to step 2
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      wordInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const nextButton = screen.getByText('Next');
        expect(nextButton).not.toBeDisabled();
        fireEvent.click(nextButton);
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(screen.getByText('Step 2: Secure Your Profile')).toBeInTheDocument();
      }, { timeout: 3000 });

      const backButton = screen.getByText('Back');
      fireEvent.click(backButton);

      await waitFor(() => {
        expect(screen.getByText('Step 1: Your Seed Phrase')).toBeInTheDocument();
      });
    });
  });
});
