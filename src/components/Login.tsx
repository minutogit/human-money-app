// src/components/Login.tsx
import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";

import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { ProfileInfo } from "../types";

interface LoginProps {
    onLoginSuccess: (profileName: string) => void;
    onSwitchToCreate: () => void;
    onSwitchToRecreate: () => void;
    onSwitchToReset: () => void;
}

export function Login({ onLoginSuccess, onSwitchToCreate, onSwitchToRecreate, onSwitchToReset }: LoginProps) {
    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [selectedProfile, setSelectedProfile] = useState<string>("");
    const [password, setPassword] = useState("");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [isLoggingIn, setIsLoggingIn] = useState(false);
    const passwordInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        logger.info("Login component displayed");
        async function fetchProfiles() {
            setIsLoading(true);
            try {
                const availableProfiles = await invoke<ProfileInfo[]>("list_profiles");
                setProfiles(availableProfiles);
                if (availableProfiles.length > 0) {
                    setSelectedProfile(availableProfiles[0].folder_name);
                }
                info(`Frontend: Found ${availableProfiles.length} profiles.`);
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

    async function handleLogin() {
        if (!selectedProfile || !password) {
            setFeedbackMsg("Error: Please select a profile and enter your password.");
            return;
        }

        // Set loading states
        setIsLoggingIn(true);
        setFeedbackMsg(""); // Clear old errors

        // We use a slightly longer timeout to ensure React has finished the render cycle
        // and the browser has had a chance to paint the loading state.
        setTimeout(async () => {
            try {
                await invoke("login", {
                    folderName: selectedProfile,
                    password,
                    cleanupOnLogin: true,
                });
                
                info("Frontend: Login successful.");
                const loggedInProfile = profiles.find(p => p.folder_name === selectedProfile);
                if (loggedInProfile) {
                    onLoginSuccess(loggedInProfile.profile_name);
                }
                // We keep isLoggingIn true while transitioning to the dashboard

            } catch (e) {
                const msg = `Login failed: ${e}`;
                setFeedbackMsg(`Error: ${msg}`);
                error(`Frontend: ${msg}`);
                setIsLoggingIn(false);
                setPassword("");
                passwordInputRef.current?.focus();
            }
        }, 150);
    }

    const feedbackClass = isLoggingIn 
        ? "text-theme-secondary animate-pulse font-bold" 
        : (feedbackMsg.includes("Error") ? "text-theme-error" : "text-theme-success");

    return (
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="w-full max-w-xl min-w-[380px] bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border border-theme-subtle">
                <div className="text-center">
                    <h1 className="text-4xl font-extrabold text-theme-primary">Human Money App</h1>
                    <p className="text-lg text-theme-light">Login to Your Wallet</p>
                </div>

                <form className="space-y-5" onSubmit={(e) => { e.preventDefault(); handleLogin(); }}>
                    {profiles.length > 0 && (
                        <div>
                            <label className="block text-sm font-semibold text-theme-secondary mb-1">Select Profile</label>
                            <div className="max-w-md mx-auto">
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
                        <label className="block text-sm font-semibold text-theme-secondary mb-1">Password</label>
                        <div className="max-w-md mx-auto">
                            <Input 
                                type="password" 
                                value={password} 
                                onChange={(e) => {
                                    setPassword(e.target.value);
                                    if (feedbackMsg.includes("Error")) setFeedbackMsg("");
                                }} 
                                onFocus={() => {
                                    if (feedbackMsg.includes("Error")) setFeedbackMsg("");
                                }}
                                placeholder="Enter your password"
                                ref={passwordInputRef}
                            />
                        </div>
                    </div>

                    <div className="pt-3 text-center">
                        {isLoggingIn ? (
                             <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>
                                Signing in... Decrypting your wallet data. Please wait.
                             </p>
                        ) : (
                            feedbackMsg && <p className={`text-center text-sm font-medium mb-4 ${feedbackClass}`}>{feedbackMsg}</p>
                        )}
                        <div className="flex flex-col items-center gap-4">
                             <Button type="submit" disabled={isLoading || isLoggingIn || profiles.length === 0}>
                                {isLoggingIn ? "Signing in..." : "Login"}
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
                                onClick={onSwitchToRecreate}
                                className="text-sm text-theme-primary hover:underline"
                            >
                                Recreate profile from seed
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
        </div>
    );
}