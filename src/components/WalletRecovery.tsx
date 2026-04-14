// src/components/WalletRecovery.tsx
import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";
import { ProfileInfo } from "../types";

interface WalletRecoveryProps {
    onRecoverySuccess: () => void;
    onSwitchToLogin: () => void;
}
type InputMode = "words" | "phrase";

export function WalletRecovery({ onRecoverySuccess, onSwitchToLogin }: WalletRecoveryProps) {
    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [selectedProfile, setSelectedProfile] = useState<string>("");

    // Core state
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [mnemonicWords, setMnemonicWords] = useState<string[]>(Array(12).fill(""));
    const [inputMode, setInputMode] = useState<InputMode>("words");

    // Form state
    const [passphrase, setPassphrase] = useState("");
    const [newPassword, setNewPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");

    // UI state
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [isValidMnemonic, setIsValidMnemonic] = useState(false);

    // State and refs for the 'phrase' input mode
    const [rawPhrase, setRawPhrase] = useState("");
    const [bip39Wordlist, setBip39Wordlist] = useState<string[]>([]);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const prevRawPhraseRef = useRef("");

    // Log when component is displayed
    useEffect(() => {
        logger.info("WalletRecovery component displayed");

        async function fetchProfiles() {
            setIsLoading(true);
            try {
                const availableProfiles = await invoke<ProfileInfo[]>("list_profiles");
                setProfiles(availableProfiles);
                if (availableProfiles.length > 0) {
                    setSelectedProfile(availableProfiles[0].folder_name);
                } else {
                    setFeedbackMsg("Error: No profiles found to recover.");
                }
                info(`Frontend: Found ${availableProfiles.length} profiles for recovery selection.`);
            } catch (e) {
                const errorMsg = `Failed to fetch profiles: ${e}`;
                setFeedbackMsg(`Error: ${errorMsg}`);
                error(`Frontend: ${errorMsg}`);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProfiles();
    }, []);

    // Effect 1: Adjust mnemonicWords array size when wordCount changes
    useEffect(() => {
        const currentWords = mnemonicWords.join(" ").split(" ").filter(Boolean);
        const newMnemonicArray = Array(wordCount).fill("").map((_, i) => currentWords[i] || "");
        setMnemonicWords(newMnemonicArray);
    }, [wordCount]);

    // Effect 2: Fetch BIP-39 wordlist on component mount
    useEffect(() => {
        async function fetchWordlist() {
            try {
                const list = await invoke<string[]>("get_bip39_wordlist");
                setBip39Wordlist(list);
                info("Successfully fetched BIP-39 wordlist from backend.");
            } catch (e) {
                error(`Failed to fetch BIP-39 wordlist: ${e}`);
            }
        }
        fetchWordlist();
    }, []);

    // Effect 3: Process the raw text from the textarea ('phrase' mode)
    useEffect(() => {
        if (inputMode !== 'phrase') return;

        const prevPhrase = prevRawPhraseRef.current;

        // Step 1: Check if the raw input contains "dirty" characters that need aggressive cleaning.
        const needsCleaning = /[0-9]+\.|\n|\r/.test(rawPhrase);

        if (needsCleaning) {
            const singleLine = rawPhrase.replace(/(\r\n|\n|\r)/gm, " ");
            const cleaned = singleLine.replace(/[0-9]+\./g, '').trim().replace(/\s+/g, ' ');
            // If cleaning is needed and changes the string, update the rawPhrase state and stop.
            // The effect will re-run with the clean text.
            if (cleaned !== rawPhrase) {
                setRawPhrase(cleaned);
                return;
            }
        }

        // Step 2: Derive words from the (now clean or naturally typed) phrase for state updates.
        const words = rawPhrase.trim().replace(/\s+/g, ' ').split(' ').filter(Boolean);
        setMnemonicWords(Array(wordCount).fill("").map((_, i) => words[i] || ""));
        if (words.length === 12 || words.length === 24) {
            setWordCount(words.length);
        }

        // Step 3: Handle auto-completion, which now runs on clean text.
        const lastWord = words[words.length - 1];
        if (rawPhrase.length > prevPhrase.length && lastWord && !rawPhrase.endsWith(' ') && bip39Wordlist.length > 0) {
            const filteredSuggestions = bip39Wordlist.filter(w => w.startsWith(lastWord.toLowerCase()));

            if (filteredSuggestions.length === 1 && filteredSuggestions[0] !== lastWord) {
                const suggestion = filteredSuggestions[0];
                const newWords = [...words];
                newWords[words.length - 1] = suggestion;
                const newPhrase = newWords.join(' ');

                setRawPhrase(newPhrase);
                setTimeout(() => {
                    const el = textareaRef.current;
                    if (el) {
                        const selectionStart = newPhrase.length - (suggestion.length - lastWord.length);
                        const selectionEnd = newPhrase.length;
                        el.setSelectionRange(selectionStart, selectionEnd);
                    }
                }, 0);
            }
        }
        prevRawPhraseRef.current = rawPhrase;
    }, [rawPhrase, inputMode, bip39Wordlist, wordCount]);

    // Effect 4: Validate the mnemonic against the backend whenever it changes
    useEffect(() => {
        const validate = async () => {
            // Only validate if there's a reasonable number of non-empty words
            const nonEmptyWords = mnemonicWords.filter(Boolean);
            if (nonEmptyWords.length > 0 && mnemonicWords.every(word => word && word.length > 1)) {
                const fullMnemonic = mnemonicWords.join(" ");
                try {
                    await invoke("validate_mnemonic", { mnemonic: fullMnemonic });
                    setIsValidMnemonic(true);
                    setFeedbackMsg("Seed phrase is valid.");
                } catch (e) {
                    setIsValidMnemonic(false);
                    setFeedbackMsg("Error: Seed phrase is not valid.");
                }
            } else {
                setIsValidMnemonic(false);
                if (nonEmptyWords.length > 0) {
                    setFeedbackMsg("Awaiting valid seed phrase...");
                } else {
                    setFeedbackMsg("");
                }
            }
        };
        validate();
    }, [mnemonicWords]);

    // Handler for the individual input fields ('words' mode)
    const handleWordChange = (index: number, value: string) => {
        const cleanedText = value.replace(/[0-9]+\.\s*/g, '');
        const pastedWords = cleanedText.trim().replace(/\s+/g, ' ').split(' ');

        const newWords = [...mnemonicWords];
        for (let i = 0; i < pastedWords.length; i++) {
            const targetIndex = index + i;
            if (targetIndex < wordCount) {
                newWords[targetIndex] = pastedWords[i].toLowerCase().trim();
            }
        }
        setMnemonicWords(newWords);

        if (pastedWords.length > 1) {
            const nextIndex = index + pastedWords.length;
            if (nextIndex < wordCount) {
                document.getElementById(`word-${nextIndex}`)?.focus();
            }
        }
    };

    // Handler to toggle between input modes
    const handleInputModeToggle = () => {
        const newMode = inputMode === "words" ? "phrase" : "words";
        if (newMode === 'phrase') {
            setRawPhrase(mnemonicWords.join(" ").trim());
        }
        setInputMode(newMode);
    };

    // Handler for the final form submission
    async function handleRecovery() {
        const el = textareaRef.current;
        if (el) el.setSelectionRange(el.value.length, el.value.length);

        if (!selectedProfile) {
            setFeedbackMsg("Error: Please select a profile to recover.");
            return;
        }
        if (!isValidMnemonic) {
            setFeedbackMsg("Error: Please enter a valid seed phrase.");
            return;
        }
        if (newPassword !== confirmPassword) {
            setFeedbackMsg("Error: The passwords do not match.");
            return;
        }
        if (newPassword.length < 8) {
            setFeedbackMsg("Error: Password must be at least 8 characters long.");
            return;
        }

        setIsLoading(true);
        setFeedbackMsg("Recovering wallet...");
        try {
            await invoke("recover_wallet_and_set_new_password", {
                folderName: selectedProfile,
                mnemonic: mnemonicWords.join(" "),
                passphrase: passphrase || undefined,
                newPassword
            });
            info("Frontend: Wallet successfully recovered. Logging in.");
            onRecoverySuccess();
        } catch (e) {
            setFeedbackMsg(`Error recovering wallet: ${e}`);
            error(`Frontend: Wallet recovery failed: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : (isValidMnemonic ? "text-theme-success" : "text-theme-light");

    return (
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="w-full max-w-4xl bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border border-theme-subtle">
                <div className="text-center">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Human Money App</h1>
                    <p className="text-lg text-theme-light mt-1">Recover Wallet</p>
                </div>

                <form className="space-y-5" onSubmit={(e) => { e.preventDefault(); handleRecovery(); }}>
                    {profiles.length > 0 && (
                         <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">1. Profile to Recover</label>
                            <div className="max-w-md">
                                <select
                                    value={selectedProfile}
                                    onChange={(e) => setSelectedProfile(e.target.value)}
                                    className="w-full px-3 py-2 border rounded-md bg-bg-input border-theme-subtle text-theme-light focus:ring-2 focus:ring-theme-primary"
                                >
                                    {profiles.map((profile) => (
                                        <option key={profile.folder_name} value={profile.folder_name}>
                                            {profile.profile_name}
                                        </option>
                                    ))}
                                </select>
                            </div>
                        </div>
                    )}

                    <div>
                        <div className="flex justify-between items-center mb-2">
                            <div className="flex items-center gap-4">
                                <label className="text-sm font-semibold text-theme-secondary">2. Your Seed Phrase</label>
                                <button type="button" onClick={handleInputModeToggle} className="text-xs text-theme-primary hover:underline">
                                    {inputMode === "words" ? "Enter full phrase" : "Use single fields"}
                                </button>
                            </div>
                            <select value={wordCount} onChange={(e) => setWordCount(Number(e.target.value) as 12 | 24)} className="px-2 py-1 text-xs border border-theme-subtle rounded-md bg-bg-input text-theme-light focus:ring-2 focus:ring-theme-primary">
                                <option value={12}>12 Words</option>
                                <option value={24}>24 Words</option>
                            </select>
                        </div>

                        {inputMode === 'words' ? (
                            <div className="grid grid-cols-3 sm:grid-cols-4 gap-3">
                                {mnemonicWords.map((word, index) => (
                                    <div key={index} className="relative">
                                        <span className="absolute left-2 top-1/2 -translate-y-1/2 text-xs text-theme-light">{index + 1}.</span>
                                        <Input
                                            id={`word-${index}`}
                                            type="text"
                                            value={word}
                                            onChange={(e) => handleWordChange(index, e.target.value)}
                                            className="pl-6 font-mono"
                                            autoComplete="off"
                                        />
                                    </div>
                                ))}
                            </div>
                        ) : (
                            <div className="relative">
                                <Textarea
                                    ref={textareaRef}
                                    id="phrase-input"
                                    value={rawPhrase}
                                    onChange={(e) => { setRawPhrase(e.target.value); }}
                                    placeholder="Enter your 12 or 24 word seed phrase here..."
                                    rows={4}
                                    className="font-mono"
                                />
                            </div>
                        )}
                    </div>

                    <div className="border-t border-theme-light-border pt-5 space-y-5">
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">3. Optional Passphrase</label>
                            <Input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} placeholder="Enter if you used one during profile creation" />
                        </div>

                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">4. New Password</label>
                            <Input type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} placeholder="Minimum 8 characters" required />
                        </div>

                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">5. Confirm New Password</label>
                            <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder="Repeat your new password" required />
                        </div>
                    </div>

                    <div className="pt-3 text-center">
                        {feedbackMsg && <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>{feedbackMsg}</p>}
                        <div className="flex flex-col items-center gap-4">
                            <Button type="submit" disabled={isLoading || !isValidMnemonic || profiles.length === 0}>
                                {isLoading ? "Recovering..." : "Recover Wallet & Login"}
                            </Button>
                            <button type="button" onClick={onSwitchToLogin} className="text-sm text-theme-primary hover:underline">
                                Back to Login
                            </button>
                        </div>
                    </div>
                </form>
            </div>
        </div>
    );
}