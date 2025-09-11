// src/components/CreateProfile.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";


export function CreateProfile() {
    const [generatedSeed, setGeneratedSeed] = useState("");
    const [confirmationSeed, setConfirmationSeed] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    useEffect(() => {
        async function initialGenerateSeed() {
            const newSeed: string = await invoke("generate_mnemonic", { wordCount: 12 });
            setGeneratedSeed(newSeed);
        }
        if (!generatedSeed) {
            initialGenerateSeed().catch(e => error(`Failed to generate initial seed: ${e}`));
        }
    }, [generatedSeed]);

    async function createProfile() {
        if (generatedSeed !== confirmationSeed) {
            setFeedbackMsg("Error: The seed phrases do not match.");
            return;
        }
        if (password !== confirmPassword) {
            setFeedbackMsg("Error: The passwords do not match.");
            return;
        }
        if (password.length < 8) {
            setFeedbackMsg("Error: Password must be at least 8 characters long.");
            return;
        }

        setIsLoading(true);
        setFeedbackMsg("Creating profile, please wait...");
        try {
            await invoke("create_profile", {
                mnemonic: generatedSeed,
                userPrefix: userPrefix || null,
                password,
            });
            setFeedbackMsg("Profile successfully created!");
            info("Frontend: Profile creation successful.");
            setGeneratedSeed("");
            setConfirmationSeed("");
            setUserPrefix("");
            setPassword("");
            setConfirmPassword("");
        } catch (e) {
            setFeedbackMsg(`Error creating profile: ${e}`);
            error(`Frontend: Profile creation failed: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }

    // Button is only disabled when loading or when there are validation errors
    const isCreationDisabled = isLoading || 
        (confirmationSeed !== "" && generatedSeed !== confirmationSeed) ||
        (password !== "" && confirmPassword !== "" && password !== confirmPassword) ||
        (password !== "" && password.length < 8);

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : "text-theme-success";

    return (
        <div className="flex flex-col items-center justify-center min-h-screen bg-bg-app p-4 font-sans">
            <div className="w-full max-w-xl bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border border-theme-subtle">
                <div className="text-center">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Voucher Wallet</h1>
                    <p className="text-lg text-theme-light mt-1">Create New Profile</p>
                </div>

                <form className="space-y-5" onSubmit={(e) => { e.preventDefault(); createProfile(); }}>
                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">1. Your Seed Phrase (Save this securely!)</label>
                        <Textarea value={generatedSeed} readOnly rows={3} className="bg-bg-input-readonly" />
                    </div>

                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">2. Confirm Seed Phrase</label>
                        <Textarea value={confirmationSeed} onChange={(e) => setConfirmationSeed(e.target.value)} placeholder="Type your 12 words again here." required rows={3} />
                    </div>

                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">3. Optional User Prefix</label>
                        <div className="max-w-md mx-auto">
                            <Input type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} placeholder="e.g., 'my_wallet' (can be left blank)" />
                        </div>
                    </div>

                    <div className="border-t border-theme-light-border pt-5 space-y-5">
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">4. Password</label>
                            <div className="max-w-md mx-auto">
                                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Minimum 8 characters" required />
                            </div>
                        </div>
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">4. Confirm Password</label>
                            <div className="max-w-md mx-auto">
                                <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder="Repeat your password" required />
                            </div>
                        </div>
                    </div>

                    <div className="pt-3 text-center">
                        {feedbackMsg && <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>{feedbackMsg}</p>}
                        <div className="flex justify-center">
                            <Button type="submit" disabled={isCreationDisabled}>
                                {isLoading ? "Creating Profile..." : "Create Profile"}
                            </Button>
                        </div>
                    </div>
                </form>
            </div>
        </div>
    );
}