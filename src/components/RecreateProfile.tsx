// src/components/RecreateProfile.tsx
import { useState, useEffect, useRef, FormEvent } from "react";
import logo from "../assets/logo.png";
import { profileService } from "../services/profileService";
import { authService } from "../services/authService";
import { AuthLayout } from "./AuthLayout";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { MnemonicLanguage } from "../types";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";
import { ArrowLeft, ArrowRight, RefreshCw, CheckCircle2, HelpCircle, AlertTriangle } from "lucide-react";
import { PrefixInfoModal } from "./ui/PrefixInfoModal";

type WizardStep = "import_seed" | "set_details";
type InputMode = "words" | "phrase";

const cleanPhrase = (phrase: string) => {
    return phrase.replace(/\s+/g, ' ').trim().toLowerCase();
};

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
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>("english");
    const [mnemonicWords, setMnemonicWords] = useState<string[]>(Array(12).fill(""));
    const [inputMode, setInputMode] = useState<InputMode>("words");
    const [isValidMnemonic, setIsValidMnemonic] = useState(false);
    const [rawPhrase, setRawPhrase] = useState("");
    const [bip39Wordlist, setBip39Wordlist] = useState<string[]>([]);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const prevRawPhraseRef = useRef("");
    const passwordInputRef = useRef<HTMLInputElement>(null);

    // --- State for Profile Details Step ---
    const [passphrase, setPassphrase] = useState<string>("");
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>("");
    const [profileName, setProfileName] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");
    const [showPrefixInfo, setShowPrefixInfo] = useState(false);


    // --- Effects ---

    // Log when component is displayed
    useEffect(() => {
        logger.info("RecreateProfile component displayed");

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

    // Effect 1: Adjust mnemonicWords array size when wordCount changes
    useEffect(() => {
        setMnemonicWords(prev => {
            const currentWords = prev.join(" ").split(" ").filter(Boolean);
            return Array(wordCount).fill("").map((_, i) => currentWords[i] || "");
        });
    }, [wordCount]);

    // Effect 2: Fetch BIP-39 wordlist on component mount
    useEffect(() => {
        async function fetchWordlist() {
            try {
                const list = await profileService.getWordlist(selectedLanguage);
                setBip39Wordlist(list);
                info(`Successfully fetched BIP-39 wordlist for ${selectedLanguage} for import.`);
            } catch (e) {
                error(`Failed to fetch BIP-39 wordlist: ${e}`);
            }
        }
        fetchWordlist();
    }, [selectedLanguage]);

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
            if (nonEmptyWords.length === wordCount) {
                const fullMnemonic = mnemonicWords.join(" ");
                try {
                    await profileService.validateMnemonic(fullMnemonic, selectedLanguage);
                    setIsValidMnemonic(true);
                    setFeedbackMsg("Seed phrase is valid.");
                } catch {
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
    }, [mnemonicWords, selectedLanguage, wordCount]);


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
        if (!isValidMnemonic) {
            setFeedbackMsg("Error: Please enter a valid seed phrase to continue.");
            return;
        }
        if (passphrase !== confirmPassphrase) {
            setFeedbackMsg("Error: The passphrases do not match.");
            return;
        }
        setFeedbackMsg("");
        setWizardStep("set_details");
    };

    // Handler for the final profile creation
    // (Adapted from CreateProfile.tsx)
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
        // Prefix is now optional for Root-Accounts

        setIsLoading(true);
        setFeedbackMsg(""); // Clear any previous errors
        
        // Use a small delay to ensure React has finished the render cycle
        // and the browser has had a chance to paint the loading state
        setTimeout(async () => {
            try {
                const localInstanceId = await authService.getLocalInstanceId();
                await profileService.createProfile({
                    profileName,
                    mnemonic: mnemonicWords.join(' '), // Use the imported mnemonic
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
                error(`Frontend: Profile recreation failed: ${e}`);
            } finally {
                setIsLoading(false);
            }
        }, 150);
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
                                <div className="flex flex-col gap-3 mb-2">
                                    <div className="flex justify-between items-center">
                                        <label className="text-sm font-semibold text-theme-secondary">Seed Phrase</label>
                                        <select 
                                            value={selectedLanguage} 
                                            onChange={(e) => setSelectedLanguage(e.target.value as MnemonicLanguage)} 
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
                                        <div className="flex items-center gap-4">
                                            <button type="button" onClick={handleInputModeToggle} className="text-xs text-theme-primary hover:underline">
                                                {inputMode === "words" ? "Enter full phrase" : "Use single fields"}
                                            </button>
                                        </div>
                                        <select id="wordCount" value={wordCount} onChange={(e) => setWordCount(Number(e.target.value) as 12 | 24)} className="px-2 py-1 text-xs border border-theme-subtle rounded-md bg-bg-input text-theme-light focus:ring-2 focus:ring-theme-primary">
                                            <option value={12}>12 Words</option>
                                            <option value={24}>24 Words</option>
                                        </select>
                                    </div>
                                </div>

                                {inputMode === 'words' ? (
                                    <div className="grid grid-cols-3 sm:grid-cols-4 gap-3">
                                        {mnemonicWords.map((word, index) => (
                                            <div key={index} className="relative">
                                                <span className="absolute left-2 top-1/2 -translate-y-1/2 text-xs text-theme-light">{index + 1}.</span>
                                                <Input
                                                    id={`word-${index}`}
                                                    data-testid={`word-input-${index}`}
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
                                            data-testid="phrase-textarea"
                                            value={rawPhrase}
                                            onChange={(e) => { setRawPhrase(cleanPhrase(e.target.value)); }}
                                            placeholder="Paste full seed phrase here... Numbers and punctuation are cleaned automatically."
                                            rows={6}
                                            className="font-mono text-sm"
                                        />
                                        <p className="text-[10px] text-theme-light mt-1">Automatic cleaning handles numbers (e.g. 1. 2.), dots, newlines and extra spaces.</p>
                                    </div>
                                )}
                            </div>

                            <div className="border-t border-theme-light-border pt-5 space-y-5">
                                <div>
                                    <label htmlFor="passphrase" className="block text-sm font-semibold text-theme-secondary mb-1">Optional Passphrase (13th Word)</label>
                                    <Input 
                                        id="passphrase"
                                        type="password" 
                                        value={passphrase} 
                                        onChange={(e) => setPassphrase(e.target.value)} 
                                        placeholder="Enter extra security word if used" 
                                    />
                                </div>
                                {passphrase && (
                                    <div>
                                        <label htmlFor="confirmPassphrase" className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Optional Passphrase</label>
                                        <Input 
                                            id="confirmPassphrase"
                                            type="password" 
                                            value={confirmPassphrase} 
                                            onChange={(e) => setConfirmPassphrase(e.target.value)} 
                                            placeholder="Verify your extra word" 
                                        />
                                    </div>
                                )}
                            </div>
                        </div>

                        <div className="flex gap-4 w-full pt-4">
                            <Button type="button" variant="secondary" onClick={onSwitchToLogin} className="flex-1 py-4 rounded-2xl gap-2">
                                <ArrowLeft size={18} /> Back to Login
                            </Button>
                            <Button type="submit" disabled={!isValidMnemonic} className="flex-1 py-4 rounded-2xl gap-2">
                                Next <ArrowRight size={18} />
                            </Button>
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
                            <label htmlFor="profileName" className="block text-sm font-semibold text-theme-secondary mb-1">Profile Name</label>
                            <Input id="profileName" data-testid="profile-name-input" type="text" value={profileName} onChange={(e) => setProfileName(e.target.value)} placeholder="e.g., 'My Main Wallet'" required />
                        </div>
                        <div>
                            <label htmlFor="password" className="block text-sm font-semibold text-theme-secondary mb-1">Password</label>
                            <Input 
                                id="password"
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
                            <label htmlFor="confirmPassword" className="block text-sm font-semibold text-theme-secondary mb-1">Confirm Password</label>
                            <Input 
                                id="confirmPassword"
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
                                <div className="flex items-center justify-between mb-1">
                                    <label htmlFor="userPrefix" className="block text-sm font-semibold text-theme-secondary">Device Prefix (Optional)</label>
                                    <button 
                                        type="button" 
                                        onClick={() => setShowPrefixInfo(true)}
                                        className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20"
                                    >
                                        <HelpCircle size={12} />
                                        <span>Read Instructions</span>
                                    </button>
                                </div>
                                <Input id="userPrefix" data-testid="user-prefix-input" type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} placeholder="e.g. 'my_laptop', '0', or 'pc'" />
                                <p className="text-[10px] font-black text-rose-600 flex items-start gap-1.5 leading-tight italic mt-1.5">
                                    <AlertTriangle size={12} className="shrink-0 mt-0.5" />
                                    Critical: Every device MUST have a unique prefix to prevent irreversible reputation loss.
                                </p>
                            </div>
                        </div>

                        <PrefixInfoModal 
                            isOpen={showPrefixInfo} 
                            onClose={() => setShowPrefixInfo(false)} 
                        />

                        <div className="flex flex-col items-center gap-4 pt-3">
                            {isLoading && (
                                <p className="text-center text-sm font-medium text-theme-secondary animate-pulse">
                                    Creating profile, please wait... This may take a moment.
                                </p>
                            )}
                            <div className="flex gap-4 w-full pt-4">
                                <Button type="button" variant="secondary" onClick={() => setWizardStep("import_seed")} className="flex-1 py-4 rounded-2xl gap-2">
                                    <ArrowLeft size={18} /> Back
                                </Button>
                                <Button type="submit" disabled={isLoading} className="flex-[2] py-4 rounded-2xl gap-2">
                                    {isLoading ? <RefreshCw className="animate-spin" size={18} /> : <CheckCircle2 size={18} />}
                                    {isLoading ? "Creating Profile..." : "Create & Encrypt Profile"}
                                </Button>
                            </div>
                        </div>
                    </form>
                );
        }
    };

    return (
        <AuthLayout maxWidth="max-w-2xl">
            <div className="flex items-center justify-center gap-3 sm:gap-6">
                <img
                    src={logo}
                    alt="Human Money Logo"
                    className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm"
                />
                <div className="text-left space-y-0 sm:space-y-0.5">
                    <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">HUMAN MONEY</h1>
                    <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">Recreate Profile from Seed</p>
                </div>
            </div>

            {renderContent()}

            {feedbackMsg && !isLoading && <p className={`text-center text-sm font-medium mt-4 ${feedbackClass}`}>{feedbackMsg}</p>}
        </AuthLayout>
    );
}
