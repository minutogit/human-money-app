// src/components/CreateProfile.tsx
import { useState, useEffect, ChangeEvent, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";

// Define the steps for the wizard
type WizardStep = "display_seed" | "confirm_seed" | "set_password";

// Define the structure for the confirmation words
interface ConfirmationWord {
    index: number;
    value: string;
}

interface CreateProfileProps {
    onProfileCreated: () => void;
}

export function CreateProfile({ onProfileCreated }: CreateProfileProps) {
    const [wizardStep, setWizardStep] = useState<WizardStep>("display_seed");
    const [generatedSeed, setGeneratedSeed] = useState<string[]>([]);
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    // State for seed confirmation step
    const [confirmationWords, setConfirmationWords] = useState<ConfirmationWord[]>([]);
    const [userConfirmationInput, setUserConfirmationInput] = useState<{ [key: number]: string }>({});

    // State for password step
    const [passphrase, setPassphrase] = useState<string>("");
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>("");
    const [profileName, setProfileName] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");

    // --- Effects ---

    useEffect(() => {
        // Log when component is displayed
        logger.info("CreateProfile component displayed");
        
        // Generate a new seed phrase when the component mounts or word count changes.
        async function generateNewSeed() {
            setIsLoading(true);
            setFeedbackMsg("Generating new secure seed phrase...");
            try {
                const newSeed: string = await invoke("generate_mnemonic", { wordCount });
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
    }, [wordCount]);


    // --- Helper Functions ---

    // Prepares the confirmation step by selecting random words
    const prepareConfirmationStep = () => {
        const shuffled = [...generatedSeed].map((word, index) => ({ word, index }));
        shuffled.sort(() => 0.5 - Math.random()); // Shuffle indices
        const selected = shuffled.slice(0, 3).map(item => ({ index: item.index, value: "" }));
        selected.sort((a, b) => a.index - b.index); // Sort by index for user-friendly display
        setConfirmationWords(selected);
        setUserConfirmationInput({}); // Reset input
        setFeedbackMsg("");
        setWizardStep("confirm_seed");
    };

    // --- Event Handlers ---

    async function handleWordCountChange(event: ChangeEvent<HTMLSelectElement>) {
        const newWordCount = parseInt(event.target.value, 10) as 12 | 24;
        setWordCount(newWordCount);
    }

    const handleConfirmationInputChange = (index: number, value: string) => {
        setUserConfirmationInput(prev => ({ ...prev, [index]: value.trim().toLowerCase() }));
    };

    const handleConfirmSeedSubmit = (e: FormEvent) => {
        e.preventDefault();
        for (const word of confirmationWords) {
            if (userConfirmationInput[word.index] !== generatedSeed[word.index]) {
                setFeedbackMsg("Error: One or more words are incorrect. Please check your saved seed phrase.");
                return;
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

        setIsLoading(true);
        setFeedbackMsg("Creating profile, please wait...");
        try {
            await invoke("create_profile", {
                profileName,
                mnemonic: generatedSeed.join(' '),
                passphrase: passphrase || undefined,
                userPrefix: userPrefix || undefined,
                password,
            });
            setFeedbackMsg("Profile successfully created!");
            onProfileCreated();
        } catch (e) {
            setFeedbackMsg(`Error creating profile: ${e}`);
            error(`Frontend: Profile creation failed: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }


    // --- Render Logic ---

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : "text-theme-success";

    const renderContent = () => {
        switch (wizardStep) {
            // STEP 1: Display Seed Phrase
            case "display_seed":
                return (
                    <>
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 1: Your Secret Seed Phrase</h2>
                            <p className="text-theme-light mt-1">Write these words down in order and store them in a secure, offline location. This is the only way to recover your wallet.</p>
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
                        <div className="flex justify-between items-center mt-4">
                            <select value={wordCount} onChange={handleWordCountChange} className="px-2 py-1 text-xs border border-theme-subtle rounded-md bg-bg-input text-theme-light focus:ring-2 focus:ring-theme-primary">
                                <option value={12}>12 Words</option>
                                <option value={24}>24 Words</option>
                            </select>
                            <Button type="button" onClick={prepareConfirmationStep} disabled={isLoading || generatedSeed.length === 0}>
                                I've Saved My Seed Phrase
                            </Button>
                        </div>
                    </>
                );

            // STEP 2: Confirm Seed Phrase
            case "confirm_seed":
                return (
                    <form onSubmit={handleConfirmSeedSubmit}>
                        <div className="text-center">
                            <h2 className="text-2xl font-bold text-theme-primary">Step 2: Confirm Your Seed Phrase</h2>
                            <p className="text-theme-light mt-1">To ensure you saved it correctly, please enter the following words from your seed phrase.</p>
                        </div>
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
                                <p className="text-xs text-theme-light mt-1">Warning: If you forget this passphrase, your seed phrase alone will not be enough to recover your wallet.</p>
                            </div>
                            <div>
                                <label className="block text-sm font-semibold text-theme-secondary mb-1">Optional User Prefix</label>
                                <Input type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} placeholder="e.g., 'my_wallet' (can be left blank)" />
                            </div>
                        </div>

                        <div className="flex justify-center pt-3">
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
                    <h1 className="text-4xl font-extrabold text-theme-primary">Voucher Wallet</h1>
                    <p className="text-lg text-theme-light mt-1">Create a New Profile</p>
                </div>

                {renderContent()}

                {feedbackMsg && <p className={`text-center text-sm font-medium mt-4 ${feedbackClass}`}>{feedbackMsg}</p>}
            </div>
        </div>
    );
}