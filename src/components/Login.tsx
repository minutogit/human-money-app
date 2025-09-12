// src/components/Login.tsx
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";

interface LoginProps {
    onLoginSuccess: () => void;
    onSwitchToCreate: () => void;
    onSwitchToReset: () => void;
}

export function Login({ onLoginSuccess, onSwitchToCreate, onSwitchToReset }: LoginProps) {
    const [password, setPassword] = useState("");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    async function handleLogin() {
        if (!password) {
            setFeedbackMsg("Error: Password is required.");
            return;
        }

        setIsLoading(true);
        setFeedbackMsg("Logging in...");
        try {
            await invoke("login", { password });
            info("Frontend: Login successful.");
            onLoginSuccess();
        } catch (e) {
            const msg = `Login failed: ${e}`;
            setFeedbackMsg(`Error: ${msg}`);
            error(`Frontend: ${msg}`);
            setIsLoading(false);
        }
    }

    const feedbackClass = feedbackMsg.includes("Error") ? "text-theme-error" : "text-theme-success";

    return (
        <div className="w-full max-w-xl bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border border-theme-subtle">
            <div className="text-center">
                <h1 className="text-4xl font-extrabold text-theme-primary">Voucher Wallet</h1>
                <p className="text-lg text-theme-light mt-1">Login to Your Wallet</p>
            </div>

            <form className="space-y-5" onSubmit={(e) => { e.preventDefault(); handleLogin(); }}>
                 <div>
                    <label className="block text-sm font-semibold text-theme-secondary mb-1">Password</label>
                    <div className="max-w-md mx-auto">
                        <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Enter your password" required />
                    </div>
                </div>

                <div className="pt-3 text-center">
                    {feedbackMsg && <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>{feedbackMsg}</p>}
                    <div className="flex flex-col items-center gap-4">
                        <Button type="submit" disabled={isLoading}>
                            {isLoading ? "Logging In..." : "Login"}
                        </Button>
                        <button
                            type="button"
                            onClick={onSwitchToCreate}
                            className="text-sm text-theme-primary hover:underline"
                        >
                            Don't have a wallet? Create one
                        </button>
                        <button
                            type="button"
                            onClick={onSwitchToReset}
                            className="text-sm text-theme-light hover:underline"
                        >
                            Forgot password?
                        </button>
                    </div>
                </div>
            </form>
        </div>
    );
}
