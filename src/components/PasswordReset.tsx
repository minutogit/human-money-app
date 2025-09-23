// src/components/PasswordReset.tsx
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Textarea } from "./ui/Textarea";

interface PasswordResetProps {
    onResetSuccess: () => void;
    onSwitchToLogin: () => void;
}

export function PasswordReset({ onResetSuccess, onSwitchToLogin }: PasswordResetProps) {
    const [mnemonic, setMnemonic] = useState("");
    const [newPassword, setNewPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    async function handleReset() {
        if (!mnemonic) {
            setFeedbackMsg("Error: Seed phrase is required.");
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
        setFeedbackMsg("Resetting password...");
        try {
            await invoke("recover_wallet_and_set_new_password", { mnemonic, newPassword });
            info("Frontend: Password successfully reset. Logging in.");
            onResetSuccess();
        } catch (e) {
            const msg = `Password reset failed: ${e}`;
            setFeedbackMsg(`Error: ${msg}`);
            error(`Frontend: ${msg}`);
        } finally {
            setIsLoading(false);
        }
    }

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : "text-theme-success";

    return (
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="w-full max-w-xl min-w-[380px] bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border border-theme-subtle">
                <div className="text-center">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Voucher Wallet</h1>
                    <p className="text-lg text-theme-light mt-1">Reset Your Password</p>
                </div>

                <form className="space-y-5" onSubmit={(e) => { e.preventDefault(); handleReset(); }}>
                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">1. Your Seed Phrase</label>
                        <Textarea value={mnemonic} onChange={(e) => setMnemonic(e.target.value)} placeholder="Enter your 12 or 24 word seed phrase to recover your wallet." required rows={3} />
                    </div>

                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">2. New Password</label>
                        <div className="max-w-md mx-auto">
                            <Input type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} placeholder="Enter your new password (min. 8 characters)" required />
                        </div>
                    </div>

                    <div>
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">3. Confirm New Password</label>
                        <div className="max-w-md mx-auto">
                            <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder="Repeat your new password" required />
                        </div>
                    </div>

                    <div className="pt-3 text-center">
                        {feedbackMsg && <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>{feedbackMsg}</p>}
                        <div className="flex flex-col items-center gap-4">
                            <Button type="submit" disabled={isLoading}>
                                {isLoading ? "Resetting..." : "Reset Password & Login"}
                            </Button>
                            <button
                                type="button"
                                onClick={onSwitchToLogin}
                                className="text-sm text-theme-primary hover:underline"
                            >
                                Back to Login
                            </button>
                        </div>
                    </div>
                </form>
            </div>
        </div>
    );
}