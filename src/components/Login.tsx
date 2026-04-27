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
    const [showHandoverUI, setShowHandoverUI] = useState(false);
    const [showPostHandoverWarning, setShowPostHandoverWarning] = useState(false);
    const [handoverUserId, setHandoverUserId] = useState("");
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
                const localInstanceId = await invoke<string>("get_local_instance_id");
                await invoke("login", {
                    folderName: selectedProfile,
                    password,
                    cleanupOnLogin: true,
                    localInstanceId,
                });
                
                info("Frontend: Login successful.");
                const loggedInProfile = profiles.find(p => p.folder_name === selectedProfile);
                if (loggedInProfile) {
                    onLoginSuccess(loggedInProfile.profile_name);
                }
                // We keep isLoggingIn true while transitioning to the dashboard

            } catch (e: any) {
                const msg = String(e);
                // The core returns "Device Mismatch" with a space.
                if (msg.includes("Device Mismatch") || msg.includes("DeviceMismatch") || msg.includes("different device")) {
                    setFeedbackMsg(msg);
                    setShowHandoverUI(true);
                } else {
                    setFeedbackMsg(`Error: ${msg}`);
                }
                error(`Frontend: Login failed: ${msg}`);
                setIsLoggingIn(false);
                setPassword("");
                passwordInputRef.current?.focus();
            }
        }, 150);
    }

    async function handleHandover() {
        setIsLoggingIn(true);
        setFeedbackMsg("Binding wallet to this device (Handover)...");
        try {
            const localInstanceId = await invoke<string>("get_local_instance_id");
            await invoke("handover_to_this_device", {
                folderName: selectedProfile,
                password,
                localInstanceId,
            });
            
            info("Frontend: Handover successful. Showing security warning.");
            const userId = await invoke<string>("get_user_id");
            setHandoverUserId(userId);
            setShowPostHandoverWarning(true);
            setIsLoggingIn(false);
        } catch (e) {
            setFeedbackMsg(`Error: Handover failed: ${e}`);
            setIsLoggingIn(false);
        }
    }

    const feedbackClass = isLoggingIn 
        ? "text-theme-secondary animate-pulse font-bold" 
        : (feedbackMsg.includes("Error") || feedbackMsg.includes("Mismatch") ? "text-theme-error" : "text-theme-success");

    return (
        <>
        {showPostHandoverWarning ? (
            <div className="w-full h-full flex flex-col items-center justify-center">
                <div className="w-full max-w-xl bg-bg-card shadow-2xl rounded-2xl p-8 space-y-6 border-2 border-theme-error animate-in fade-in zoom-in duration-300">
                    <div className="text-center space-y-2">
                        <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-red-100 dark:bg-red-900/30 text-theme-error mb-2">
                            <svg className="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                            </svg>
                        </div>
                        <h2 className="text-2xl font-bold text-theme-error">Handover Successful!</h2>
                        <p className="text-theme-light">The wallet is now bound to this device.</p>
                    </div>

                    <div className="p-4 bg-bg-input rounded-lg border border-theme-subtle space-y-2">
                        <p className="text-xs font-semibold text-theme-secondary uppercase tracking-wider">Your Unique User ID (DID):</p>
                        <div className="relative group">
                            <p className="text-sm font-mono break-all text-theme-primary bg-black/5 dark:bg-white/5 p-3 rounded border border-theme-subtle/50">
                                {handoverUserId}
                            </p>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="p-4 bg-red-50 dark:bg-red-900/10 rounded-lg border border-red-200 dark:border-red-800 text-sm text-theme-light">
                            <p className="font-bold text-theme-error mb-2">CRITICAL SECURITY WARNING:</p>
                            <p>This identity is defined by your seed phrase AND the <span className="font-bold underline">User Prefix</span> you chose.</p>
                            
                            <div className="mt-3 space-y-2">
                                <p>1. You <span className="font-bold underline text-theme-error">MUST NOT</span> use this exact profile (same Prefix) on your <span className="font-bold underline">OLD device</span> anymore. Delete the wallet folder there immediately!</p>
                                <p>2. If you want to use the same seed phrase on multiple devices simultaneously, you <span className="font-bold italic">must</span> create a new profile with a <span className="font-bold text-theme-success">DIFFERENT User Prefix</span> for each device.</p>
                            </div>

                            <p className="mt-3 text-xs italic">Reusing the same identity (Prefix) concurrently leads to double-spending conflicts and permanent reputation loss.</p>
                        </div>

                        <Button 
                            type="button" 
                            onClick={() => {
                                const loggedInProfile = profiles.find(p => p.folder_name === selectedProfile);
                                if (loggedInProfile) {
                                    onLoginSuccess(loggedInProfile.profile_name);
                                }
                            }}
                            className="w-full !bg-theme-primary py-4 font-bold shadow-lg hover:shadow-xl transition-all"
                        >
                            I Understand - Proceed to Dashboard
                        </Button>
                    </div>
                </div>
            </div>
        ) : (
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
                                    onChange={(e) => {
                                        setSelectedProfile(e.target.value);
                                        setShowHandoverUI(false);
                                        setFeedbackMsg("");
                                    }}
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
                                {feedbackMsg || "Signing in... Decrypting your wallet data. Please wait."}
                             </p>
                        ) : (
                            feedbackMsg && (
                                <div className={`text-left text-xs font-medium mb-4 whitespace-pre-wrap p-4 rounded-lg border shadow-sm ${
                                    feedbackMsg.includes("Mismatch") 
                                        ? "bg-amber-50 dark:bg-amber-900/20 border-amber-200 dark:border-amber-800 text-theme-light" 
                                        : "bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800 text-theme-error"
                                }`}>
                                    {feedbackMsg}
                                </div>
                            )
                        )}
                        <div className="flex flex-col items-center gap-4">
                            {showHandoverUI ? (
                                <div className="w-full space-y-4">
                                    <Button 
                                        type="button" 
                                        onClick={handleHandover} 
                                        disabled={isLoggingIn} 
                                        variant="primary" 
                                        className="w-full !bg-amber-600 hover:!bg-amber-700 text-white font-bold py-3 shadow-lg transform transition active:scale-95"
                                    >
                                        Authorize & Perform Device Handover
                                    </Button>
                                    <Button
                                        type="button"
                                        onClick={() => {
                                            setShowHandoverUI(false);
                                            setFeedbackMsg("");
                                        }}
                                        disabled={isLoggingIn}
                                        variant="secondary"
                                        className="w-full"
                                    >
                                        Cancel - Keep using on old device
                                    </Button>
                                    <p className="text-[10px] text-theme-light italic">
                                        Note: This binds the wallet profile to THIS device. 
                                        Use 'Option B' below if you want to use the seed concurrently on multiple devices.
                                    </p>
                                </div>
                            ) : (
                                <Button type="submit" disabled={isLoading || isLoggingIn || profiles.length === 0} className="w-full">
                                    {isLoggingIn ? "Signing in..." : "Login"}
                                </Button>
                            )}
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
        )}
        </>
    );
}