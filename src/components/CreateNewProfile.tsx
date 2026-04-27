// src/components/CreateNewProfile.tsx
import { useState, useEffect, useRef, ChangeEvent, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { MnemonicLanguage } from "../types";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";

// Define the steps for the wizard
type WizardStep = "display_seed" | "confirm_seed" | "set_password";

// Define the structure for the confirmation words
interface ConfirmationWord {
    index: number;
    value: string;
}

interface CreateNewProfileProps {
    onProfileCreated: () => void;
    onSwitchToRecreate: () => void;
    onSwitchToLogin?: () => void;
}

export function CreateNewProfile({ onProfileCreated, onSwitchToRecreate, onSwitchToLogin }: CreateNewProfileProps) {
    const [wizardStep, setWizardStep] = useState<WizardStep>("display_seed");
    const [generatedSeed, setGeneratedSeed] = useState<string[]>([]);
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>("english");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    // State for seed confirmation step
    const [confirmationWords, setConfirmationWords] = useState<ConfirmationWord[]>([]);
    const [userConfirmationInput, setUserConfirmationInput] = useState<{ [key: number]: string }>({});
    const [isBulkMode, setIsBulkMode] = useState(false);
    const [bulkSeedInput, setBulkSeedInput] = useState("");

    // State for password step
    const [passphrase, setPassphrase] = useState<string>("");
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>("");
    const [profileName, setProfileName] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");
    const passwordInputRef = useRef<HTMLInputElement>(null);

    // --- Effects ---

    useEffect(() => {
        // Log when component is displayed
        logger.info("CreateNewProfile component displayed");

        // Detect system language for smart default
        const systemLang = navigator.language || "en";
        let detectedLanguage: MnemonicLanguage = "english";
        if (systemLang.startsWith("de")) {
            detectedLanguage = "german";
        } else if (systemLang.startsWith("es")) {
            detectedLanguage = "spanish";
        } else if (systemLang.startsWith("fr")) {
            detectedLanguage = "french";
        } else if (systemLang.startsWith("it")) {
            detectedLanguage = "italian";
        } else if (systemLang.startsWith("ja")) {
            detectedLanguage = "japanese";
        } else if (systemLang.startsWith("ko")) {
            detectedLanguage = "korean";
        } else if (systemLang.startsWith("pt")) {
            detectedLanguage = "portuguese";
        } else if (systemLang.startsWith("cs")) {
            detectedLanguage = "czech";
        } else if (systemLang.startsWith("zh-CN")) {
            detectedLanguage = "chineseSimplified";
        } else if (systemLang.startsWith("zh-TW")) {
            detectedLanguage = "chineseTraditional";
        }
        setSelectedLanguage(detectedLanguage);
    }, []);

    useEffect(() => {
        // Generate a new seed phrase when the component mounts or word count or language changes.
        async function generateNewSeed() {
            setIsLoading(true);
            setFeedbackMsg("Generating new secure seed phrase...");
            try {
                const newSeed: string = await invoke("generate_mnemonic", { wordCount, language: selectedLanguage });
                setGeneratedSeed(newSeed.split(' '));
                setFeedbackMsg("");
                info("Frontend: A new seed phrase was generated.");
            } catch (e) {
                const errorMsg = `Failed to generate seed phrase: ${e}`;
                setFeedbackMsg(`Error: ${errorMsg}`);
                error(errorMsg);
            } finally {
                setIsLoading(false);
            }
        }
        generateNewSeed();
    }, [wordCount, selectedLanguage]);

    // Auto-clean bulk seed input (same logic as RecreateProfile)
    useEffect(() => {
        if (!isBulkMode) return;

        const cleanSeedText = (text: string) => {
            return text
                .toLowerCase()
                .replace(/[0-9.,\-:]/g, ' ') // Remove digits and punctuation
                .replace(/[\r\n\t]/g, ' ')      // Replace tabs and newlines with space
                .replace(/\s+/g, ' ')          // Collapse multiple spaces
                .trim();
        };

        const cleaned = cleanSeedText(bulkSeedInput);
        if (cleaned !== bulkSeedInput && bulkSeedInput.length > 0) {
            // Auto-apply cleaning if there are numbers or multiple spaces
            if (/[0-9.,\-:]/.test(bulkSeedInput) || /\s\s/.test(bulkSeedInput)) {
                setBulkSeedInput(cleaned);
            }
        }
    }, [bulkSeedInput, isBulkMode]);


    // --- Helper Functions ---

    // Prepares the confirmation step by selecting random words
    const prepareConfirmationStep = () => {
        const shuffled = [...generatedSeed].map((word, index) => ({ word, index }));
        shuffled.sort(() => 0.5 - Math.random()); // Shuffle indices
        const selected = shuffled.slice(0, 3).map(item => ({ index: item.index, value: "" }));
        selected.sort((a, b) => a.index - b.index); // Sort by index for user-friendly display
        setConfirmationWords(selected);
        setUserConfirmationInput({}); // Reset input
        setBulkSeedInput("");
        setIsBulkMode(false);
        setFeedbackMsg("");
        setWizardStep("confirm_seed");
    };

    const cleanSeedText = (text: string) => {
        return text
            .toLowerCase()
            .replace(/[0-9.,\-:]/g, ' ') // Remove digits and punctuation
            .replace(/[\r\n\t]/g, ' ')      // Replace tabs and newlines with space
            .replace(/\s+/g, ' ')          // Collapse multiple spaces
            .trim();
    };

    // --- Event Handlers ---

    async function handleWordCountChange(event: ChangeEvent<HTMLSelectElement>) {
        const newWordCount = parseInt(event.target.value, 10) as 12 | 24;
        setWordCount(newWordCount);
    }

    async function handleLanguageChange(event: ChangeEvent<HTMLSelectElement>) {
        const newLanguage = event.target.value as MnemonicLanguage;
        setSelectedLanguage(newLanguage);
    }

    const handleConfirmationInputChange = (index: number, value: string) => {
        setUserConfirmationInput(prev => ({ ...prev, [index]: value.trim().toLowerCase() }));
    };

    const handleConfirmSeedSubmit = (e: FormEvent) => {
        e.preventDefault();
        
        if (isBulkMode) {
            const cleanedInput = cleanSeedText(bulkSeedInput);
            const targetSeed = generatedSeed.join(' ');
            if (cleanedInput !== targetSeed) {
                setFeedbackMsg("Error: The phrase does not match your generated seed. Please check for typos or missing words.");
                return;
            }
        } else {
            for (const word of confirmationWords) {
                if (userConfirmationInput[word.index] !== generatedSeed[word.index]) {
                    setFeedbackMsg("Error: One or more words are incorrect. Please check your saved seed phrase.");
                    return;
                }
            }
        }

        setFeedbackMsg("Seed phrase confirmed successfully!");
        setTimeout(() => {
            setFeedbackMsg("");
            setWizardStep("set_password");
        }, 1500);
    };

    async function handleCreateProfileSubmit(e: FormEvent) {
        e.preventDefault();
        if (password !== confirmPassword) {
            setFeedbackMsg("Error: The passwords do not match.");
            passwordInputRef.current?.focus();
            return;
        }
        if (password.length < 8) {
            setFeedbackMsg("Error: Password must be at least 8 characters long.");
            passwordInputRef.current?.focus();
            return;
        }
        if (passphrase !== confirmPassphrase) {
            setFeedbackMsg("Error: The passphrases do not match.");
            passwordInputRef.current?.focus();
            return;
        }

        setIsLoading(true);
        setFeedbackMsg(""); // Clear any previous errors
        
        // Use a small delay to ensure React has finished the render cycle
        // and the browser has had a chance to paint the loading state
        setTimeout(async () => {
            try {
                const localInstanceId = await invoke<string>("get_local_instance_id");
                await invoke("create_profile", {
                    profileName,
                    mnemonic: generatedSeed.join(' '),
                    passphrase: passphrase || undefined,
                    userPrefix: userPrefix || undefined,
                    password,
                    localInstanceId,
                    language: selectedLanguage,
                });
                // Keep loading state true during transition, like Login
                onProfileCreated();
            } catch (e) {
                setFeedbackMsg(`Error creating profile: ${e}`);
                error(`Frontend: Profile creation failed: ${e}`);
            } finally {
                setIsLoading(false);
            }
        }, 150);
    }


    // --- Render Logic ---

    const feedbackClass = "text-theme-error";

    const renderContent = () => {
        switch (wizardStep) {
            // STEP 1: Display Seed Phrase
            case "display_seed":
                return (
                    <>
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 1: Your Secret Seed Phrase</h2>
                            <p className="text-theme-light mt-1">Write these words down in order and store them in a secure, offline location. This is the only way to recover your wallet.</p>
                            <p className="text-theme-light mt-2 text-sm">
                                Already have a seed phrase?
                                <button type="button" onClick={onSwitchToRecreate} className="ml-1 text-theme-primary hover:underline font-semibold">
                                    Recreate profile here
                                </button>
                            </p>
                        </div>
                        <div className="my-4 p-4 border border-theme-error rounded-lg bg-red-500/10">
                            <p className="font-bold text-center text-theme-error">WARNING: Never share this phrase with anyone. Anyone with this phrase can take your funds.</p>
                        </div>
                        <div className="grid grid-cols-3 gap-x-6 gap-y-3 my-6">
                            {generatedSeed.map((word, index) => (
                                <div key={index} className="text-theme-secondary font-mono">
                                    <span className="text-sm text-theme-light mr-2">{index + 1}.</span>{word}
                                </div>
                            ))}
                        </div>
                        <div className="flex flex-col gap-3 mt-4">
                            <div className="flex justify-between items-center">
                                <label className="text-sm font-semibold text-theme-secondary">Mnemonic Language</label>
                                <select 
                                    value={selectedLanguage} 
                                    onChange={handleLanguageChange} 
                                    className="px-2 py-1 text-xs border border-theme-subtle rounded-md bg-bg-input text-theme-light focus:ring-2 focus:ring-theme-primary"
                                >
                                    <option value="english">English</option>
                                    <option value="german">Deutsch</option>
                                    <option value="spanish">Español</option>
                                    <option value="french">Français</option>
                                    <option value="italian">Italiano</option>
                                    <option value="japanese">日本語</option>
                                    <option value="korean">한국어</option>
                                    <option value="portuguese">Português</option>
                                    <option value="czech">Čeština</option>
                                    <option value="chineseSimplified">简体中文</option>
                                    <option value="chineseTraditional">繁體中文</option>
                                </select>
                            </div>
                            <div className="flex justify-between items-center">
                                <div className="flex gap-2">
                                    <select value={wordCount} onChange={handleWordCountChange} className="px-2 py-1 text-xs border border-theme-subtle rounded-md bg-bg-input text-theme-light focus:ring-2 focus:ring-theme-primary">
                                        <option value={12}>12 Words</option>
                                        <option value={24}>24 Words</option>
                                    </select>
                                    {onSwitchToLogin && (
                                        <Button type="button" variant="secondary" onClick={onSwitchToLogin} className="!px-3 !py-1 text-xs">
                                            Back to Login
                                        </Button>
                                    )}
                                </div>
                                <Button type="button" onClick={prepareConfirmationStep} disabled={isLoading || generatedSeed.length === 0}>
                                    I've Saved My Seed Phrase
                                </Button>
                            </div>
                        </div>
                    </>
                );

            // STEP 2: Confirm Seed Phrase
            case "confirm_seed":
                return (
                    <form onSubmit={handleConfirmSeedSubmit}>
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 2: Confirm Your Seed Phrase</h2>
                            <p className="text-theme-light mt-1">
                                {isBulkMode 
                                    ? "Paste your entire seed phrase below. Numbers and punctuation will be cleaned automatically." 
                                    : "To ensure you saved it correctly, please enter the following words from your seed phrase."}
                            </p>
                            <button 
                                type="button" 
                                onClick={() => {
                                    setIsBulkMode(!isBulkMode);
                                    setFeedbackMsg("");
                                }} 
                                className="mt-2 text-xs text-theme-primary hover:underline font-semibold"
                            >
                                {isBulkMode ? "Switch to Individual Words (Standard)" : "Switch to Bulk Paste (Pro Mode)"}
                            </button>
                        </div>

                        {isBulkMode ? (
                            <div className="my-8">
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Paste Full Seed Phrase</label>
                                <textarea
                                    value={bulkSeedInput}
                                    onChange={(e) => setBulkSeedInput(e.target.value)}
                                    className="w-full h-32 px-3 py-2 border border-theme-subtle rounded-md bg-bg-input text-theme-primary font-mono text-sm focus:outline-none focus:ring-2 focus:ring-theme-primary"
                                    placeholder="Paste here... e.g. 1. apple 2. banana..."
                                    required
                                />
                                <p className="text-[10px] text-theme-light mt-1">Automatic cleaning will handle numbers, dots, newlines and extra spaces.</p>
                            </div>
                        ) : (
                            <div className="space-y-4 my-8">
                                {confirmationWords.map(({ index }) => (
                                    <div key={index}>
                                        <label className="block text-sm font-semibold text-theme-secondary mb-1">Word #{index + 1}</label>
                                        <Input
                                            type="text"
                                            value={userConfirmationInput[index] || ""}
                                            onChange={(e) => handleConfirmationInputChange(index, e.target.value)}
                                            required
                                            className="font-mono"
                                            autoComplete="off"
                                        />
                                    </div>
                                ))}
                            </div>
                        )}
                        
                        <div className="flex justify-between items-center">
                            <Button type="button" variant="secondary" onClick={() => setWizardStep("display_seed")}>Back</Button>
                            <Button type="submit">Confirm Seed</Button>
                        </div>
                    </form>
                );

            // STEP 3: Set Password
            case "set_password":
                return (
                    <form onSubmit={handleCreateProfileSubmit} className="space-y-5">
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 3: Secure Your Wallet</h2>
                            <p className="text-theme-light mt-1">Create a strong password to encrypt your wallet on this device.</p>
                        </div>

                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Profile Name</label>
                            <Input type="text" value={profileName} onChange={(e) => setProfileName(e.target.value)} placeholder="e.g., 'My Main Wallet'" required />
                        </div>
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Password</label>
                            <Input 
                                type="password" 
                                value={password} 
                                onChange={(e) => setPassword(e.target.value)} 
                                onFocus={() => {
                                    if (feedbackMsg.includes("Error")) {
                                        setFeedbackMsg("");
                                    }
                                    setPassword("");
                                    setConfirmPassword("");
                                }}
                                placeholder="Minimum 8 characters" 
                                required 
                                ref={passwordInputRef}
                            />
                        </div>
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Password</label>
                            <Input 
                                type="password" 
                                value={confirmPassword} 
                                onChange={(e) => setConfirmPassword(e.target.value)} 
                                onFocus={() => {
                                    if (feedbackMsg.includes("Error")) {
                                        setFeedbackMsg("");
                                    }
                                }}
                                placeholder="Repeat your password" 
                                required 
                            />
                        </div>

                        <div className="border-t border-theme-light-border pt-5 space-y-5">
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Optional Passphrase (Advanced)</label>
                                <Input 
                                    type="password" 
                                    value={passphrase} 
                                    onChange={(e) => setPassphrase(e.target.value)} 
                                    onFocus={() => {
                                        if (feedbackMsg.includes("Error")) {
                                            setFeedbackMsg("");
                                        }
                                    }}
                                    placeholder="Adds extra security to your seed" 
                                />
                            </div>
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Optional Passphrase</label>
                                <Input 
                                    type="password" 
                                    value={confirmPassphrase} 
                                    onChange={(e) => setConfirmPassphrase(e.target.value)} 
                                    onFocus={() => {
                                        if (feedbackMsg.includes("Error")) {
                                            setFeedbackMsg("");
                                        }
                                    }}
                                    placeholder="Repeat your passphrase" 
                                />
                                <p className="text-xs text-theme-light mt-1">Warning: If you forget this passphrase, your seed phrase alone will not be enough to recover your wallet.</p>
                            </div>
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Optional User Prefix</label>
                                <Input type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} placeholder="e.g., 'my_wallet' (can be left blank)" />
                            </div>
                        </div>

                        <div className="flex flex-col items-center gap-4 pt-3">
                            {isLoading && (
                                <p className="text-center text-sm font-medium text-theme-secondary animate-pulse">
                                    Creating profile, please wait... This may take a moment.
                                </p>
                            )}
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
            <div className="w-full max-w-xl min-w-[420px] bg-bg-card shadow-2xl rounded-2xl p-8 border border-theme-subtle">
                <div className="text-center mb-6">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Human Money App</h1>
                    <p className="text-lg text-theme-light mt-1">Create a New Profile</p>
                </div>

                {renderContent()}

                {feedbackMsg && !isLoading && <p className={`text-center text-sm font-medium mt-4 ${feedbackClass}`}>{feedbackMsg}</p>}
            </div>
        </div>
    );
}