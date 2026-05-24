import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { WalletRecovery } from '../WalletRecovery';
import { invoke } from '@tauri-apps/api/core';
import { ProfileInfo } from '../../types';

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

describe('WalletRecovery Component', () => {
  const mockOnRecoverySuccess = vi.fn();
  const mockOnSwitchToLogin = vi.fn();

  const mockSeed12 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon';
  const mockSeed24 = 'apple banana cherry date elderberry fig grape honey iris jasmine kiwi lemon mango nut olive peach quince raspberry strawberry tangerine umbrella violet watermelon xylem yellow zebra';
  const mockWordlist = ['apple', 'banana', 'cherry', 'date', 'elderberry', 'fig', 'grape', 'honey', 'iris', 'jasmine', 'kiwi', 'lemon', 'mango', 'nut', 'olive', 'peach', 'quince', 'raspberry', 'strawberry', 'tangerine', 'umbrella', 'violet', 'watermelon', 'xylem', 'yellow', 'zebra'];
  const mockProfiles: ProfileInfo[] = [
    { profileName: 'Test Profile 1', folderName: 'profile1' },
    { profileName: 'Test Profile 2', folderName: 'profile2' },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(mockProfiles);
      }
      if (cmd === 'get_bip39_wordlist') {
        return Promise.resolve(mockWordlist);
      }
      if (cmd === 'validate_mnemonic') {
        const mnemonic = args?.mnemonic || '';
        if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
          return Promise.resolve(true);
        }
        return Promise.reject(new Error('Invalid mnemonic'));
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'recover_wallet_and_set_new_password') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
  });

  describe('Initialization', () => {
    it('renders the recovery form', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Recover Wallet')).toBeInTheDocument();
      });
    });

    it('calls list_profiles on component mount', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('list_profiles');
      });
    });

    it('populates profile dropdown with available profiles', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Test Profile 1')).toBeInTheDocument();
        expect(screen.getByText('Test Profile 2')).toBeInTheDocument();
      });
    });

    it('selects first profile by default', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const select = screen.getByTestId('profile-select');
        expect(select).toHaveValue('profile1');
      });
    });

    it('shows error message when no profiles are available', async () => {
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_profiles') {
          return Promise.resolve([]);
        }
        if (cmd === 'get_bip39_wordlist') {
          return Promise.resolve(mockWordlist);
        }
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') {
          return Promise.resolve('test-instance-id');
        }
        if (cmd === 'recover_wallet_and_set_new_password') {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      });

      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText(/Error: No profiles found to recover/i)).toBeInTheDocument();
      });
    });

    it('calls get_bip39_wordlist on component mount', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
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
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
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

  describe('Seed Input (Words Mode)', () => {
    it('shows 12 word input fields by default', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        // First input is profile dropdown, then 12 word inputs
        expect(wordInputs.length).toBeGreaterThanOrEqual(12);
      });
    });

    it('adjusts to 24 word inputs when word count changes', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
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
        expect(wordInputs.length).toBeGreaterThanOrEqual(24);
      });
    });

    it('validates mnemonic live and shows success message', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBeGreaterThanOrEqual(12);
      });

      // Fill with valid seed (skip first input which is profile dropdown)
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs; // Get the 12 seed word inputs
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('validates mnemonic live and shows error message', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBeGreaterThanOrEqual(12);
      });

      // Fill with invalid seed
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input) => {
        fireEvent.change(input, { target: { value: 'wrongword' } });
      });

      await waitFor(() => {
        expect(screen.getByText(/Error: Invalid mnemonic/i)).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('enables recover button only when mnemonic is valid', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const recoverButton = screen.getByText('Recover Wallet & Login');
        expect(recoverButton).toBeDisabled();
      });

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        const recoverButton = screen.getByText('Recover Wallet & Login');
        expect(recoverButton).not.toBeDisabled();
      }, { timeout: 1000 });
    });

    it('handles paste of multiple words in single field', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBeGreaterThanOrEqual(12);
      });

      // Paste multiple words into first seed field
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const firstSeedInput = wordInputs[1]; // First seed input (index 0 is profile dropdown)
      fireEvent.change(firstSeedInput, { target: { value: 'apple banana cherry date' } });

      await waitFor(() => {
        const seedInputs = wordInputs.slice(1, 13);
        expect(seedInputs[0]).toHaveValue('apple');
        expect(seedInputs[1]).toHaveValue('banana');
        expect(seedInputs[2]).toHaveValue('cherry');
        expect(seedInputs[3]).toHaveValue('date');
      });
    });

    it('cleans numbered prefixes from pasted text', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        const wordInputs = screen.getAllByTestId(/word-input-/);
        expect(wordInputs.length).toBeGreaterThanOrEqual(12);
      });

      // Paste with numbered prefixes
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const firstSeedInput = wordInputs[1];
      fireEvent.change(firstSeedInput, { target: { value: '1. apple 2. banana 3. cherry' } });

      await waitFor(() => {
        const seedInputs = wordInputs.slice(1, 13);
        expect(seedInputs[0]).toHaveValue('apple');
        expect(seedInputs[1]).toHaveValue('banana');
        expect(seedInputs[2]).toHaveValue('cherry');
      });
    });
  });

  describe('Seed Input (Phrase Mode)', () => {
    it('switches to phrase mode when clicking toggle', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Enter full phrase')).toBeInTheDocument();
      });

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });
    });

    it('auto-cleans phrase input with numbers and punctuation', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i);
      fireEvent.change(textarea, { 
        target: { value: '1. apple 2. banana 3. cherry 4. date 5. elderberry 6. fig 7. grape 8. honey 9. iris 10. jasmine 11. kiwi 12. lemon' } 
      });

      await waitFor(() => {
        expect(textarea).toHaveValue(mockSeed12);
      });
    });

    it('handles newlines and tabs in phrase input', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i);
      fireEvent.change(textarea, { 
        target: { value: 'apple\tbanana\ncherry\r\ndate\telderberry' } 
      });

      await waitFor(() => {
        expect(textarea).toHaveValue('apple banana cherry date elderberry');
      });
    });

    it('auto-completes word when there is a single suggestion', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i);
      
      // Type partial word that has single completion
      fireEvent.change(textarea, { target: { value: 'app' } });

      await waitFor(() => {
        // Should auto-complete to 'apple'
        expect(textarea).toHaveValue('apple');
      }, { timeout: 1000 });
    });

    it('validates phrase input live', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i);
      fireEvent.change(textarea, { target: { value: mockSeed12 } });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });
    });

    it('switches back to words mode preserving input', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      const phraseToggle = screen.getByText('Enter full phrase');
      fireEvent.click(phraseToggle);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i)).toBeInTheDocument();
      });

      const textarea = screen.getByPlaceholderText(/Enter your 12 or 24 word seed phrase here/i);
      fireEvent.change(textarea, { target: { value: mockSeed12 } });

      const wordsToggle = screen.getByText('Use single fields');
      fireEvent.click(wordsToggle);

      await waitFor(() => {
        const words = mockSeed12.split(' ');
        // Check if words are preserved in order
        for (let i = 0; i < 12; i++) {
          const input = screen.getByTestId(`word-input-${i}`);
          expect(input).toHaveValue(words[i]);
        }
      });
    });
  });

  describe('Password Validation', () => {
    it('shows error when passwords do not match', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Fill password fields with mismatched values
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password456' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passwords do not match/i)).toBeInTheDocument();
      });
    });

    it('shows error when password is too short', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'short' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'short' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: Password must be at least 8 characters/i)).toBeInTheDocument();
      });
    });

    it('clears error message when focusing on password field', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password456' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: The passwords do not match/i)).toBeInTheDocument();
      });

      // Focus on password field
      fireEvent.focus(newPasswordInput);

      await waitFor(() => {
        expect(screen.queryByText(/Error: The passwords do not match/i)).not.toBeInTheDocument();
      });
    });

    it('shows error when no profile is selected', async () => {
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_profiles') {
          return Promise.resolve(mockProfiles);
        }
        if (cmd === 'get_bip39_wordlist') {
          return Promise.resolve(mockWordlist);
        }
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') {
          return Promise.resolve('test-instance-id');
        }
        if (cmd === 'recover_wallet_and_set_new_password') {
          return Promise.resolve(undefined);
        }
        return Promise.resolve(undefined);
      });

      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Clear profile selection
      const profileSelect = screen.getByTestId('profile-select');
      fireEvent.change(profileSelect, { target: { value: '' } });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Error: Please select a profile to recover/i)).toBeInTheDocument();
      });
    });

    it('shows error when seed is not valid', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with invalid seed
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input) => {
        fireEvent.change(input, { target: { value: 'wrongword' } });
      });

      await waitFor(() => {
        expect(screen.getByText(/Error: Invalid mnemonic/i)).toBeInTheDocument();
      }, { timeout: 1000 });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      await waitFor(() => {
        const recoverButton = screen.getByText('Recover Wallet & Login');
        expect(recoverButton).toBeDisabled();
      });
    });
  });

  describe('Successful Recovery', () => {
    it('successfully recovers wallet with valid seed and new password', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('recover_wallet_and_set_new_password', {
          folderName: 'profile1',
          mnemonic: mockSeed12,
          passphrase: undefined,
          newPassword: 'password123',
          localInstanceId: 'test-instance-id',
          language: 'english',
        });
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(mockOnRecoverySuccess).toHaveBeenCalled();
      }, { timeout: 3000 });
    });

    it('successfully recovers wallet with passphrase', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Fill passphrase
      const passphraseInput = screen.getByPlaceholderText('Enter extra word if you used one during creation');
      fireEvent.change(passphraseInput, { target: { value: 'mysecret' } });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('recover_wallet_and_set_new_password', {
          folderName: 'profile1',
          mnemonic: mockSeed12,
          passphrase: 'mysecret',
          newPassword: 'password123',
          localInstanceId: 'test-instance-id',
          language: 'english',
        });
      }, { timeout: 3000 });

      await waitFor(() => {
        expect(mockOnRecoverySuccess).toHaveBeenCalled();
      }, { timeout: 3000 });
    });

    it('shows loading state during recovery', async () => {
      let resolveRecovery: (value: unknown) => void = () => {};
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_profiles') {
          return Promise.resolve(mockProfiles);
        }
        if (cmd === 'get_bip39_wordlist') {
          return Promise.resolve(mockWordlist);
        }
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') {
          return Promise.resolve('test-instance-id');
        }
        if (cmd === 'recover_wallet_and_set_new_password') {
          return new Promise(resolve => { resolveRecovery = resolve; });
        }
        return Promise.resolve(undefined);
      });

      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByText('Seed phrase is valid.')).toBeInTheDocument();
      }, { timeout: 1000 });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Recovering wallet, please wait/i)).toBeInTheDocument();
        expect(recoverButton).toBeDisabled();
      });

      // Resolve the promise
      if (resolveRecovery) resolveRecovery(undefined);

      await waitFor(() => {
        expect(mockOnRecoverySuccess).toHaveBeenCalled();
      }, { timeout: 3000 });
    });

    it('handles backend error during recovery', async () => {
      (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_profiles') {
          return Promise.resolve(mockProfiles);
        }
        if (cmd === 'get_bip39_wordlist') {
          return Promise.resolve(mockWordlist);
        }
        if (cmd === 'validate_mnemonic') {
          const mnemonic = args?.mnemonic || '';
          if (mnemonic === mockSeed12 || mnemonic === mockSeed24) {
            return Promise.resolve(true);
          }
          return Promise.reject(new Error('Invalid mnemonic'));
        }
        if (cmd === 'get_local_instance_id') {
          return Promise.resolve('test-instance-id');
        }
        if (cmd === 'recover_wallet_and_set_new_password') {
          return Promise.reject(new Error('Backend error'));
        }
        return Promise.resolve(undefined);
      });

      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
          onSwitchToLogin={mockOnSwitchToLogin}
        />
      );

      // Fill with valid seed
      const words = mockSeed12.split(' ');
      const wordInputs = screen.getAllByTestId(/word-input-/);
      const seedInputs = wordInputs;
      seedInputs.forEach((input, index) => {
        fireEvent.change(input, { target: { value: words[index] } });
      });

      await waitFor(() => {
        expect(screen.getByTestId('feedback-message')).toHaveTextContent('Seed phrase is valid.');
      }, { timeout: 4000 });

      // Fill password
      const newPasswordInput = screen.getByPlaceholderText('Minimum 8 characters');
      fireEvent.change(newPasswordInput, { target: { value: 'password123' } });

      const confirmPasswordInput = screen.getByPlaceholderText('Repeat your new password');
      fireEvent.change(confirmPasswordInput, { target: { value: 'password123' } });

      const recoverButton = screen.getByText('Recover Wallet & Login');
      fireEvent.click(recoverButton);

      await waitFor(() => {
        expect(screen.getByText(/Error recovering wallet/i)).toBeInTheDocument();
      }, { timeout: 3000 });

      expect(mockOnRecoverySuccess).not.toHaveBeenCalled();
    });
  });

  describe('Navigation', () => {
    it('calls onSwitchToLogin when clicking Back to Login', async () => {
      render(
        <WalletRecovery
          onRecoverySuccess={mockOnRecoverySuccess}
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
  });
});
