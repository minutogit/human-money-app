// src/components/CreateProfile.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";

// Import unserer neuen UI-Komponenten
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
        } catch (e) {
            setFeedbackMsg(`Error creating profile: ${e}`);
            error(`Frontend: Profile creation failed: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }

    const isCreationDisabled =
        !generatedSeed ||
        generatedSeed !== confirmationSeed ||
        !password ||
        password !== confirmPassword ||
        isLoading;

    const feedbackClass = feedbackMsg.includes("Error") ? "text-danger" : "text-accent";

    return (
        <div className="flex flex-col items-center justify-center min-h-screen bg-background p-4">
            <div className="w-full max-w-xl bg-surface shadow-xl rounded-xl p-8 space-y-6">
                <div className="text-center">
                    <h1 className="text-3xl font-bold text-gray-900">Voucher Wallet</h1>
                    <p className="text-md text-secondary mt-1">Create New Profile</p>
                </div>

                <form className="space-y-4" onSubmit={(e) => { e.preventDefault(); createProfile(); }}>
                    {/* Beachten Sie, wie sauber das jetzt ist. Keine langen classNames mehr! */}
                    <div>
                        <label className="block text-sm font-medium text-secondary mb-1">1. Your Seed Phrase</label>
                        <Textarea value={generatedSeed} readOnly rows={3} className="bg-gray-100" />
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-secondary mb-1">2. Confirm Seed Phrase</label>
                        <Textarea value={confirmationSeed} onChange={(e) => setConfirmationSeed(e.target.value)} required rows={3} />
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-secondary mb-1">3. Optional User Prefix</label>
                        <Input type="text" value={userPrefix} onChange={(e) => setUserPrefix(e.target.value)} />
                    </div>

                    {/* Visuelle Gruppierung der Passwort-Felder */}
                    <div className="border-t border-gray-200 pt-4 space-y-4">
                        <div>
                            <label className="block text-sm font-medium text-secondary mb-1">4. Password</label>
                            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
                        </div>
                        <div>
                            <label className="block text-sm font-medium text-secondary mb-1">5. Confirm Password</label>
                            <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} required />
                        </div>
                    </div>

                    <div className="pt-2">
                        {feedbackMsg && <p className={`text-center text-sm font-medium mb-3 ${feedbackClass}`}>{feedbackMsg}</p>}
                        <Button type="submit" disabled={isCreationDisabled}>
                            {isLoading ? "Creating..." : "Create Profile"}
                        </Button>
                    </div>
                </form>
            </div>
        </div>
    );
}