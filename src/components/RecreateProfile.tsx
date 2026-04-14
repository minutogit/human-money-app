// src/components/RecreateProfile.tsx
import { useState, useEffect, useRef, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";

type WizardStep = "import_seed" | "set_details";
type InputMode = "words" | "phrase";

interface RecreateProfileProps {
    onProfileCreated: () => void;
    onSwitchToLogin: () => void;
}

export function RecreateProfile({ onProfileCreated, onSwitchToLogin }: RecreateProfileProps) {
    const [wizardStep, setWizardStep] = useState<WizardStep>("import_seed");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    // --- State for Seed Import Step ---
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [mnemonicWords, setMnemonicWords] = useState<string[]>(Array(12).fill(""));
    const [inputMode, setInputMode] = useState<InputMode>("words");
    const [isValidMnemonic, setIsValidMnemonic] = useState(false);
    const [rawPhrase, setRawPhrase] = useState("");
    const [bip39Wordlist, setBip39Wordlist] = useState<string[]>([]);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const prevRawPhraseRef = useRef("");

    // --- State for Profile Details Step ---
    const [passphrase, setPassphrase] = useState<string>("");
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>("");
    const [profileName, setProfileName] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");


    // --- Effects ---

    // Log when component is displayed
    useEffect(() => {
        logger.info("RecreateProfile component displayed");
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
                info("Successfully fetched BIP-39 wordlist for import.");
            } catch (e) {
                error(`Failed to fetch BIP-39 wordlist: ${e}`);
            }
        }
        fetchWordlist();
    }, []);

    // Effect 3: Process the raw text from the textarea ('phrase' mode)
    // (Copied from WalletRecovery.tsx)
    useEffect(() => {
        if (inputMode !== 'phrase') return;
        const prevPhrase = prevRawPhraseRef.current;
        
        // --- PRO-MODE CLEANING (Same as CreateNewProfile) ---
        const cleanSeedText = (text: string) => {
            return text
                .toLowerCase()
                .replace(/[0-9.,\-:]/g, ' ') // Remove digits and punctuation
                .replace(/[\r\n\t]/g, ' ')      // Replace tabs and newlines with space
                .replace(/\s+/g, ' ')          // Collapse multiple spaces
                .trim();
        };

        const cleaned = cleanSeedText(rawPhrase);
        if (cleaned !== rawPhrase && rawPhrase.length > 0) {
            // Check if we should auto-apply cleaning. 
            // For recreate, we apply it if there are numbers or multiple spaces.
            if (/[0-9.,\-:]/.test(rawPhrase) || /\s\s/.test(rawPhrase)) {
                setRawPhrase(cleaned);
                return;
            }
        }

        const words = rawPhrase.trim().replace(/\s+/g, ' ').split(' ').filter(Boolean);
        setMnemonicWords(Array(wordCount).fill("").map((_, i) => words[i] || ""));
        if (words.length === 12 || words.length === 24) {
            setWordCount(words.length);
        }

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

    // Effect 4: Validate the mnemonic against the backend
    // (Copied from WalletRecovery.tsx)
    useEffect(() => {
        const validate = async () => {
            const nonEmptyWords = mnemonicWords.filter(Boolean);
            if (nonEmptyWords.length > 0 && mnemonicWords.every(word => word && word.length > 1) && (mnemonicWords.length === 12 || mnemonicWords.length === 24)) {
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
        const timer = setTimeout(validate, 300); // Debounce validation
        return () => clearTimeout(timer);
    }, [mnemonicWords]);


    // --- Event Handlers ---

    // Handler for the individual input fields ('words' mode)
    // (Copied from WalletRecovery.tsx)
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
    // (Copied from WalletRecovery.tsx)
    const handleInputModeToggle = () => {
        const newMode = inputMode === "words" ? "phrase" : "words";
        if (newMode === 'phrase') {
            setRawPhrase(mnemonicWords.join(" ").trim());
        }
        setInputMode(newMode);
    };

    // Handler for proceeding to step 2
    const handleGoToDetails = () => {
        if (isValidMnemonic) {
            setFeedbackMsg("");
            setWizardStep("set_details");
        } else {
            setFeedbackMsg("Error: Please enter a valid seed phrase to continue.");
        }
    };

    // Handler for the final profile creation
    // (Adapted from CreateProfile.tsx)
    async function handleCreateProfileSubmit(e: FormEvent) {
        e.preventDefault();
        if (password !== confirmPassword) {
            setFeedbackMsg("Error: The passwords do not match.");
            return;
        }
        if (password.length < 8) {
            setFeedbackMsg("Error: Password must be at least 8 characters long.");
            return;
        }
        if (passphrase !== confirmPassphrase) {
            setFeedbackMsg("Error: The passphrases do not match.");
            return;
        }
        if (!userPrefix || userPrefix.trim().length === 0) {
            setFeedbackMsg("Error: A unique User Prefix is required for this profile.");
            return;
        }

        setIsLoading(true);
        setFeedbackMsg("Creating profile, please wait...");
        try {
            await invoke("create_profile", {
                profileName,
                mnemonic: mnemonicWords.join(' '), // Use the imported mnemonic
                passphrase: passphrase || undefined,
                userPrefix: userPrefix,
                password,
            });
            setFeedbackMsg("Profile successfully created!");
            onProfileCreated();
        } catch (e) {
            setFeedbackMsg(`Error creating profile: ${e}`);
            error(`Frontend: Profile recreation failed: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }


    // --- Render Logic ---

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : (isValidMnemonic ? "text-theme-success" : "text-theme-light");

    const renderContent = () => {
        switch (wizardStep) {
            // STEP 1: Import Seed Phrase
            case "import_seed":
                return (
                    <form onSubmit={(e) => { e.preventDefault(); handleGoToDetails(); }}>
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 1: Your Seed Phrase</h2>
                            <p className="text-theme-light mt-1">Enter your existing 12 or 24-word secret seed phrase.</p>
                        </div>

                        <div className="space-y-5 my-8">
                            <div>
                                <div className="flex justify-between items-center mb-2">
                                    <div className="flex items-center gap-4">
                                        <label className="text-sm font-semibold text-theme-secondary">Seed Phrase</label>
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
                                            placeholder="Paste full seed phrase here... Numbers and punctuation are cleaned automatically."
                                            rows={6}
                                            className="font-mono text-sm"
                                        />
                                        <p className="text-[10px] text-theme-light mt-1">Automatic cleaning handles numbers (e.g. 1. 2.), dots, newlines and extra spaces.</p>
                                    </div>
                                )}
                            </div>
                        </div>

                        <div className="flex justify-between items-center">
                            <Button type="button" variant="secondary" onClick={onSwitchToLogin}>Back to Login</Button>
                            <Button type="submit" disabled={!isValidMnemonic}>Next</Button>
                        </div>
                    </form>
                );

            // STEP 2: Set Profile Details
            case "set_details":
                return (
                    <form onSubmit={handleCreateProfileSubmit} className="space-y-5">
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 2: Secure Your Profile</h2>
                            <p className="text-theme-light mt-1">Create a name and password for this new profile.</p>
                        </div>

                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Profile Name</label>
                            <Input type="text" value={profileName} onChange={(e) => setProfileName(e.target.value)} placeholder="e.g., 'My Main Wallet'" required />
                        </div>
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Password</label>
                            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Minimum 8 characters" required />
                        </div>
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Password</label>
                            <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder="Repeat your password" required />
                        </div>

                        <div className="border-t border-theme-light-border pt-5 space-y-5">
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Optional Passphrase (Advanced)</label>
                                <Input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} placeholder="Adds extra security to your seed" />
                            </div>
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Optional Passphrase</label>
                                <Input type="password" value={confirmPassphrase} onChange={(e) => setConfirmPassphrase(e.target.value)} placeholder="Repeat your passphrase" />
                                <p className="text-xs text-theme-light mt-1">Warning: If you use this, you must enter the *exact same* passphrase for this seed on all devices.</p>
                            </div>
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">User Prefix (CRITICAL)</label>
                                <Input type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} placeholder="e.g., 'my_laptop' (must be unique)" required />
                                <p className="text-xs text-theme-error font-semibold mt-1">WARNING: This prefix MUST be *unique* for each device (e.g., laptop, phone). Reusing the same prefix on multiple devices will lead to critical errors and unresolvable conflicts.</p>
                            </div>
                        </div>

                        <div className="flex justify-between items-center pt-3">
                            <Button type="button" variant="secondary" onClick={() => setWizardStep("import_seed")}>Back</Button>
                            <Button type="submit" disabled={isLoading}>
                                {isLoading ? "Creating Profile..." : "Create & Encrypt Profile"}
                            </Button>
                        </div>
                    </form>
                );
        }
    };

    return (
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="w-full max-w-4xl min-w-[420px] bg-bg-card shadow-2xl rounded-2xl p-8 border border-theme-subtle">
                <div className="text-center mb-6">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Human Money App</h1>
                    <p className="text-lg text-theme-light mt-1">Recreate Profile from Seed</p>
                </div>

                {renderContent()}

                {feedbackMsg && <p className={`text-center text-sm font-medium mt-4 ${feedbackClass}`}>{feedbackMsg}</p>}
            </div>
        </div>
    );
}