// src/components/Login.tsx
import { useState, useEffect, useRef } from "react";
import logo from "../assets/logo.png";

import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Card } from "./ui/Card";
import { ProfileInfo } from "../types";
import { 
    UserCircle, 
    Lock, 
    LogIn, 
    Trash2, 
    ShieldAlert, 
    RefreshCw, 
    AlertTriangle, 
    CheckCircle2, 
    Key, 
    ArrowRight,
    PlusCircle,
    UserX,
    ShieldCheck
} from "lucide-react";

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
    
    const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
    const [showDeletePasswordPrompt, setShowDeletePasswordPrompt] = useState(false);
    const [deletePassword, setDeletePassword] = useState("");
    const [deleteUserId, setDeleteUserId] = useState("");
    const [isDeleting, setIsDeleting] = useState(false);
    const [isVerifyingDelete, setIsVerifyingDelete] = useState(false);
    
    const passwordInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        logger.info("Login component displayed");
        refreshProfiles();
    }, []);

    async function refreshProfiles() {
        setIsLoading(true);
        try {
            const availableProfiles = await invoke<ProfileInfo[]>("list_profiles");
            setProfiles(availableProfiles);
            if (availableProfiles.length > 0 && !selectedProfile) {
                setSelectedProfile(availableProfiles[0].folderName);
            }
        } catch (e) {
            setFeedbackMsg(`Profile Access Error: ${e}`);
        } finally {
            setIsLoading(false);
        }
    }

    async function handleLogin() {
        if (!selectedProfile || !password) {
            setFeedbackMsg("Please select a profile and enter your password.");
            return;
        }

        setIsLoggingIn(true);
        setFeedbackMsg("");

        setTimeout(async () => {
            try {
                const localInstanceId = await invoke<string>("get_local_instance_id");
                await invoke("login", {
                    folderName: selectedProfile,
                    password,
                    cleanupOnLogin: true,
                    localInstanceId,
                });
                
                const loggedInProfile = profiles.find(p => p.folderName === selectedProfile);
                if (loggedInProfile) onLoginSuccess(loggedInProfile.profileName);
            } catch (e: any) {
                const msg = String(e);
                if (msg.includes("Device Mismatch") || msg.includes("different device")) {
                    setFeedbackMsg("Security Protocol: Device Mismatch Detected.");
                    setShowHandoverUI(true);
                } else {
                    setFeedbackMsg(`Verification Failure: ${msg}`);
                }
                setIsLoggingIn(false);
                setPassword("");
                passwordInputRef.current?.focus();
            }
        }, 150);
    }

    async function handleHandover() {
        setIsLoggingIn(true);
        setFeedbackMsg("Linking profile to this device...");
        try {
            const localInstanceId = await invoke<string>("get_local_instance_id");
            await invoke("handover_to_this_device", {
                folderName: selectedProfile,
                password,
                localInstanceId,
            });
            
            const userId = await invoke<string>("get_user_id");
            setHandoverUserId(userId);
            setShowPostHandoverWarning(true);
            setIsLoggingIn(false);
        } catch (e) {
            setFeedbackMsg(`Handover Failure: ${e}`);
            setIsLoggingIn(false);
        }
    }

    async function handleVerifyDeletePassword() {
        setIsVerifyingDelete(true);
        setFeedbackMsg("");
        try {
            const userId = await invoke<string>("verify_profile_password", {
                folderName: selectedProfile,
                password: deletePassword,
            });
            setDeleteUserId(userId);
            setShowDeletePasswordPrompt(false);
            setShowDeleteConfirm(true);
        } catch (e) {
            setFeedbackMsg(`Authentication Error: ${e}`);
        } finally {
            setIsVerifyingDelete(false);
        }
    }

    async function handleDeleteProfile() {
        setIsDeleting(true);
        setFeedbackMsg("Deleting profile...");
        try {
            await invoke("delete_profile", {
                folderName: selectedProfile,
                password: deletePassword,
            });
            
            setFeedbackMsg(`Profile deleted successfully.`);
            setShowDeleteConfirm(false);
            setDeletePassword("");
            await refreshProfiles();
            const availableProfiles = await invoke<ProfileInfo[]>("list_profiles");
            if (availableProfiles.length > 0) setSelectedProfile(availableProfiles[0].folderName);
            else onSwitchToCreate();
        } catch (e) {
            setFeedbackMsg(`Purge failure: ${e}`);
        } finally {
            setIsDeleting(false);
        }
    }

    const activeProfile = profiles.find(p => p.folderName === selectedProfile);
    const activeProfileName = activeProfile?.profileName || "Unknown Profile";

    return (
        <div className="w-full min-h-screen flex items-start sm:items-center justify-center py-4 sm:py-20 px-4 relative overflow-hidden bg-white">
            {/* Background elements */}
            <div className="absolute top-0 left-1/4 w-96 h-96 bg-theme-primary/5 rounded-full blur-3xl -translate-y-1/2"></div>
            <div className="absolute bottom-0 right-1/4 w-96 h-96 bg-theme-primary/5 rounded-full blur-3xl translate-y-1/2"></div>

            {/* Overlays */}
            {showDeleteConfirm && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
                    <Card className="w-full max-w-xl border-rose-200 shadow-2xl p-10 space-y-8 animate-in zoom-in duration-300">
                        <div className="text-center space-y-4">
                            <div className="mx-auto w-20 h-20 bg-rose-500 text-white rounded-[32px] flex items-center justify-center shadow-premium-lg">
                                <UserX size={40} />
                            </div>
                            <div className="space-y-1">
                                <h2 className="text-3xl font-black text-rose-900 tracking-tighter uppercase">Delete Profile</h2>
                                <p className="text-[10px] font-black uppercase tracking-[0.2em] text-rose-500">Identity: {activeProfileName}</p>
                            </div>
                        </div>

                        <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                            <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">User ID (DID)</p>
                            <p className="text-xs font-mono break-all text-slate-800 bg-white p-3 rounded-2xl border border-slate-100">{deleteUserId}</p>
                        </div>

                        <div className="p-6 bg-rose-50 border border-rose-100 rounded-[32px] space-y-4">
                            <div className="flex items-center gap-2 text-rose-600">
                                <ShieldAlert size={18} />
                                <h3 className="text-xs font-black uppercase tracking-widest">Restoration Advisory</h3>
                            </div>
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                If you haven't moved this vault (Handover), <span className="font-black text-rose-500">EVERYTHING WILL BE LOST PERMANENTLY</span>. Only proceed if you have a verified backup on another device or your master key sequence.
                            </p>
                        </div>

                        <div className="flex gap-4">
                            <Button type="button" variant="secondary" onClick={() => setShowDeleteConfirm(false)} className="flex-1 rounded-2xl">Abort</Button>
                            <Button type="button" onClick={handleDeleteProfile} disabled={isDeleting} className="flex-1 rounded-3xl !bg-rose-600 hover:!bg-rose-700 shadow-premium-lg">
                                {isDeleting ? "Deleting..." : "Delete Profile"}
                            </Button>
                        </div>
                    </Card>
                </div>
            )}

            {showDeletePasswordPrompt && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
                    <Card className="w-full max-w-md shadow-2xl p-10 space-y-8 animate-in zoom-in duration-300">
                        <div className="text-center space-y-4">
                            <div className="mx-auto w-16 h-16 bg-slate-100 text-slate-400 rounded-3xl flex items-center justify-center">
                                <Lock size={32} />
                            </div>
                            <div className="space-y-1">
                                <h2 className="text-2xl font-black text-slate-900 tracking-tight uppercase">Delete Profile</h2>
                                <p className="text-[10px] font-black uppercase tracking-widest text-slate-400">Authorize deletion for {activeProfileName}</p>
                            </div>
                        </div>
                        
                        <form className="space-y-6" onSubmit={(e) => { e.preventDefault(); handleVerifyDeletePassword(); }}>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-slate-400 uppercase tracking-widest">Vault Password</label>
                                <Input type="password" value={deletePassword} onChange={(e) => setDeletePassword(e.target.value)} placeholder="Required" autoFocus />
                            </div>

                            <div className="flex gap-4">
                                <Button type="button" variant="secondary" onClick={() => { setShowDeletePasswordPrompt(false); setDeletePassword(""); setFeedbackMsg(""); }} disabled={isVerifyingDelete} className="flex-1 rounded-2xl">Cancel</Button>
                                <Button type="submit" disabled={isVerifyingDelete || !deletePassword} className="flex-1 rounded-3xl shadow-md">
                                    {isVerifyingDelete ? "Verifying..." : "Verify"}
                                </Button>
                            </div>
                        </form>
                    </Card>
                </div>
            )}

            {showPostHandoverWarning ? (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
                    <Card className="w-full max-w-xl border-theme-primary shadow-2xl p-10 space-y-8 animate-in zoom-in duration-300">
                        <div className="text-center space-y-4">
                            <div className="mx-auto w-20 h-20 bg-theme-primary text-white rounded-[32px] flex items-center justify-center shadow-premium-lg">
                                <ShieldCheck size={40} />
                            </div>
                            <div className="space-y-1">
                                <h2 className="text-3xl font-black text-theme-primary tracking-tighter uppercase">Handover Complete</h2>
                                <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">Handover Complete</p>
                            </div>
                        </div>

                        <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                            <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">Bound Identity (DID)</p>
                            <p className="text-xs font-mono break-all text-theme-primary bg-white p-3 rounded-2xl border border-slate-100">{handoverUserId}</p>
                        </div>

                        <div className="p-6 bg-amber-50 border border-amber-100 rounded-[32px] space-y-4">
                            <div className="flex items-center gap-2 text-amber-600">
                                <AlertTriangle size={18} />
                                <h3 className="text-xs font-black uppercase tracking-widest">Double-Spend Prevention</h3>
                            </div>
                            <p className="text-sm font-medium text-amber-900 leading-relaxed">
                                This vault is now bound to this instance. <span className="font-black">DO NOT</span> use this profile on the old device. Simultaneous use will cause permanent reputation damage and asset locking.
                            </p>
                        </div>

                        <Button onClick={() => {
                            const loggedInProfile = profiles.find(p => p.folderName === selectedProfile);
                            if (loggedInProfile) onLoginSuccess(loggedInProfile.profileName);
                        }} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            Acknowledge & Access Dashboard <ArrowRight size={20} />
                        </Button>
                    </Card>
                </div>
            ) : (
                <div className="w-full max-w-2xl bg-white/80 backdrop-blur-xl border border-theme-subtle rounded-[48px] p-6 sm:p-10 sm:pb-12 shadow-premium-lg space-y-6 sm:space-y-8 relative animate-in fade-in zoom-in duration-700">
                    <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-theme-primary/20 to-transparent"></div>
                    
                    <div className="flex items-center justify-center gap-3 sm:gap-6">
                        <img 
                            src={logo} 
                            alt="Human Money Logo" 
                            className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm" 
                        />
                        <div className="text-left space-y-0 sm:space-y-0.5">
                            <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">HUMAN MONEY</h1>
                            <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">Login to Your Wallet</p>
                        </div>
                    </div>

                    <form className="space-y-6" onSubmit={(e) => { e.preventDefault(); handleLogin(); }}>
                        {profiles.length > 0 && (
                            <div className="space-y-3">
                                <label htmlFor="profile-select" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><UserCircle size={12}/> Select Profile</label>
                                <div className="flex gap-3">
                                    <div className="relative flex-1 group">
                                        <select
                                            id="profile-select"
                                            value={selectedProfile}
                                            onChange={(e) => { setSelectedProfile(e.target.value); setShowHandoverUI(false); setFeedbackMsg(""); }}
                                            className="w-full bg-white border border-theme-subtle rounded-2xl px-5 py-4 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all group-hover:border-theme-primary/30"
                                        >
                                            {profiles.map((profile) => (
                                                <option key={profile.folderName} value={profile.folderName}>
                                                    {profile.profileName}
                                                </option>
                                            ))}
                                        </select>
                                        <div className="absolute right-5 top-1/2 -translate-y-1/2 pointer-events-none text-theme-light">
                                            <ArrowRight size={16} className="rotate-90" />
                                        </div>
                                    </div>
                                    <button 
                                        type="button"
                                        onClick={() => setShowDeletePasswordPrompt(true)}
                                        className="w-14 h-14 rounded-2xl bg-slate-50 border border-slate-200 text-slate-400 hover:text-rose-500 hover:bg-rose-50 hover:border-rose-200 transition-all flex items-center justify-center shadow-sm"
                                        title="Delete profile"
                                    >
                                        <Trash2 size={22} />
                                    </button>
                                </div>
                            </div>
                        )}

                        <div className="space-y-3">
                            <label htmlFor="password-input" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Lock size={12}/> Password</label>
                            <Input 
                                id="password-input"
                                type="password" 
                                value={password} 
                                onChange={(e) => { setPassword(e.target.value); if (feedbackMsg.includes("Verification")) setFeedbackMsg(""); }} 
                                placeholder="Enter Access Password"
                                ref={passwordInputRef}
                                className="py-5 px-6 text-lg font-bold tracking-widest"
                            />
                        </div>

                        <div className="space-y-6 pt-4">
                            {showHandoverUI ? (
                                <div className="space-y-4 animate-in slide-in-from-bottom-4">
                                    <Button 
                                        type="button" 
                                        onClick={handleHandover} 
                                        disabled={isLoggingIn} 
                                        className="w-full py-5 rounded-3xl !bg-amber-600 hover:!bg-amber-700 shadow-premium-lg text-lg gap-3"
                                    >
                                        {isLoggingIn ? <RefreshCw className="animate-spin" /> : <ShieldAlert size={24} />}
                                        Perform Device Handover
                                    </Button>
                                    <button 
                                        type="button" 
                                        onClick={() => { setShowHandoverUI(false); setFeedbackMsg(""); }} 
                                        disabled={isLoggingIn}
                                        className="w-full text-center text-[10px] font-black uppercase tracking-widest text-theme-light hover:text-theme-primary transition-colors"
                                    >
                                        Cancel Protocol
                                    </button>
                                </div>
                            ) : (
                                <Button type="submit" disabled={isLoading || isLoggingIn || profiles.length === 0 || !password || !selectedProfile} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                                    {isLoggingIn ? <RefreshCw className="animate-spin" size={24} /> : <LogIn size={24} />}
                                    {isLoggingIn ? "Authorizing..." : "Login"}
                                </Button>
                            )}

                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                <button type="button" onClick={onSwitchToCreate} className="p-4 bg-slate-50 border border-slate-200 rounded-[24px] text-center group hover:border-theme-primary/40 transition-all">
                                    <PlusCircle size={20} className="mx-auto mb-2 text-slate-400 group-hover:text-theme-primary" />
                                    <p className="text-[10px] font-black uppercase tracking-widest text-slate-500 group-hover:text-theme-primary">Don't have a wallet? Create one</p>
                                </button>
                                <button type="button" onClick={onSwitchToRecreate} className="p-4 bg-slate-50 border border-slate-200 rounded-[24px] text-center group hover:border-theme-primary/40 transition-all">
                                    <RefreshCw size={20} className="mx-auto mb-2 text-slate-400 group-hover:text-theme-primary" />
                                    <p className="text-[10px] font-black uppercase tracking-widest text-slate-500 group-hover:text-theme-primary">Recreate profile from seed</p>
                                </button>
                            </div>

                            <button type="button" onClick={onSwitchToReset} className="w-full text-center text-[10px] font-black uppercase tracking-[0.2em] text-theme-light hover:text-theme-primary transition-colors flex items-center justify-center gap-2">
                                <Key size={12} /> Forgot password?
                            </button>
                        </div>

                        {feedbackMsg && (
                            <div className={`p-5 rounded-[32px] border flex items-center gap-4 animate-in slide-in-from-bottom-4 ${feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') || feedbackMsg.includes('Mismatch') ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                                {feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') || feedbackMsg.includes('Mismatch') ? <ShieldAlert size={20} className="text-rose-500 shrink-0" /> : <CheckCircle2 size={20} className="text-emerald-500 shrink-0" />}
                                <div>
                                    <h4 className="text-[10px] font-black uppercase tracking-widest mb-1">{feedbackMsg.includes('Error') ? 'Verification System' : 'Protocol Alert'}</h4>
                                    <p className="text-sm font-bold leading-tight">{feedbackMsg}</p>
                                </div>
                            </div>
                        )}
                    </form>

                    <div className="pt-4 border-t border-theme-subtle/40 flex justify-center">
                        <p className="text-[9px] font-black text-slate-400 uppercase tracking-[0.3em] flex items-center gap-2">
                            <ShieldCheck size={12} /> Human Money Protocol v2.0
                        </p>
                    </div>
                </div>
            )}
        </div>
    );
}