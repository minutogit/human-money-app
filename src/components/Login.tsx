// src/components/Login.tsx
import { useState, useEffect, useRef, useCallback } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";

import { openUrl } from "@tauri-apps/plugin-opener";
import { authService } from "../services/authService";
import { AuthLayout } from "./AuthLayout";
import { profileService } from "../services/profileService";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Card } from "./ui/Card";
import { ProfileInfo } from "../types";
import { HelpIcon } from "./ui/HelpIcon";
import { useNavigation } from "../context/NavigationContext";
import { translateError, isBackendError } from "../utils/errorHelper";
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
    ShieldCheck,
    BookOpen
} from "lucide-react";

interface LoginProps {
    onLoginSuccess: (profileName: string) => void;
    onSwitchToCreate: () => void;
    onSwitchToRecreate: () => void;
    onSwitchToReset: () => void;
}

export function Login({ onLoginSuccess, onSwitchToCreate, onSwitchToRecreate, onSwitchToReset }: LoginProps) {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
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
    const [deleteError, setDeleteError] = useState("");
    
    const [localInstanceId, setLocalInstanceId] = useState("");
    
    const passwordInputRef = useRef<HTMLInputElement>(null);

    const refreshProfiles = useCallback(async () => {
        setIsLoading(true);
        try {
            const availableProfiles = await authService.listProfiles();
            setProfiles(availableProfiles);
            if (availableProfiles.length > 0 && !selectedProfile) {
                setSelectedProfile(availableProfiles[0].folderName);
            }
        } catch (e) {
            setFeedbackMsg(`${t('profile.accessError')}: ${translateError(e, t)}`);
        } finally {
            setIsLoading(false);
        }
    }, [selectedProfile, t]);

    useEffect(() => {
        logger.info("Login component displayed");
        refreshProfiles();
        authService.getLocalInstanceId()
            .then(setLocalInstanceId)
            .catch(err => logger.error(`Failed to load local instance ID: ${err}`));
    }, [refreshProfiles]);

    async function handleLogin() {
        if (!selectedProfile || !password) {
            setFeedbackMsg(t('profile.loginSelectProfile'));
            return;
        }

        setIsLoggingIn(true);
        setFeedbackMsg("");

        setTimeout(async () => {
            try {
                const localInstanceId = await authService.getLocalInstanceId();
                await authService.login({
                    folderName: selectedProfile,
                    password,
                    cleanupOnLogin: true,
                    localInstanceId,
                });
                
                const loggedInProfile = profiles.find(p => p.folderName === selectedProfile);
                if (loggedInProfile) onLoginSuccess(loggedInProfile.profileName);
            } catch (e) {
                const msg = isBackendError(e) ? e.message : String(e);
                if ((isBackendError(e) && e.code === 'error.auth.deviceMismatch') || msg.includes("Device Mismatch") || msg.includes("different device")) {
                    setFeedbackMsg(translateError(e, t));
                    setShowHandoverUI(true);
                } else {
                    setFeedbackMsg(`${t('profile.verificationFailure')}: ${translateError(e, t)}`);
                }
                setIsLoggingIn(false);
                setPassword("");
                passwordInputRef.current?.focus();
            }
        }, 150);
    }

    async function handleHandover() {
        setIsLoggingIn(true);
        setFeedbackMsg(t('profile.linkingDevice'));
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await authService.handoverToThisDevice({
                folderName: selectedProfile,
                password,
                targetInstanceId: localInstanceId,
            });
            
            const userId = await profileService.getUserId();
            setHandoverUserId(userId);
            setShowPostHandoverWarning(true);
            setIsLoggingIn(false);
        } catch (e) {
            setFeedbackMsg(`${t('profile.handoverFailure')}: ${translateError(e, t)}`);
            setIsLoggingIn(false);
        }
    }

    async function handleVerifyDeletePassword() {
        setIsVerifyingDelete(true);
        setDeleteError("");
        try {
            const userId = await authService.verifyProfilePassword({
                folderName: selectedProfile,
                password: deletePassword,
            });
            setDeleteUserId(userId);
            setShowDeletePasswordPrompt(false);
            setShowDeleteConfirm(true);
        } catch (e) {
            setDeleteError(`${t('profile.authenticationError')}: ${translateError(e, t)}`);
        } finally {
            setIsVerifyingDelete(false);
        }
    }

    async function handleDeleteProfile() {
        setIsDeleting(true);
        setDeleteError("");
        setFeedbackMsg(t('profile.deleting'));
        try {
            await authService.deleteProfile({
                folderName: selectedProfile,
                password: deletePassword,
            });
            
            setFeedbackMsg(t('profile.deletedSuccess'));
            setShowDeleteConfirm(false);
            setDeletePassword("");
            await refreshProfiles();
            const availableProfiles = await authService.listProfiles();
            if (availableProfiles.length > 0) setSelectedProfile(availableProfiles[0].folderName);
            else onSwitchToCreate();
        } catch (e) {
            const errorMsg = `${t('profile.purgeFailure')}: ${translateError(e, t)}`;
            setDeleteError(errorMsg);
            setFeedbackMsg(errorMsg);
        } finally {
            setIsDeleting(false);
        }
    }

    const activeProfile = profiles.find(p => p.folderName === selectedProfile);
    const activeProfileName = activeProfile?.profileName || t('profile.unknown');

    return (
        <AuthLayout maxWidth="max-w-2xl">
            {/* Overlays */}
            {showDeleteConfirm && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
                    <Card className="w-full max-w-xl border-rose-200 shadow-2xl p-10 space-y-8 animate-in zoom-in duration-300">
                        <div className="text-center space-y-4">
                            <div className="mx-auto w-20 h-20 bg-rose-500 text-white rounded-[32px] flex items-center justify-center shadow-premium-lg">
                                <UserX size={40} />
                            </div>
                            <div className="space-y-1">
                                <h2 className="text-3xl font-black text-rose-900 tracking-tighter uppercase flex items-center justify-center gap-2">
                                    <span>{t('profile.deleteProfile')}</span>
                                    <HelpIcon topic="profile" size={20} />
                                </h2>
                                <p className="text-[10px] font-black uppercase tracking-[0.2em] text-rose-500">{t('profile.identity')} {activeProfileName}</p>
                            </div>
                        </div>

                        <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                            <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('profile.userId')}</p>
                            <p className="text-xs font-mono break-all text-slate-800 bg-white p-3 rounded-2xl border border-slate-100">{deleteUserId}</p>
                        </div>

                        <div className="p-6 bg-rose-50 border border-rose-100 rounded-[32px] space-y-4">
                            <div className="flex items-center gap-2 text-rose-600">
                                <ShieldAlert size={18} />
                                <h3 className="text-xs font-black uppercase tracking-widest">{t('profile.restorationAdvisory')}</h3>
                            </div>
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                {t('profile.restoreWarning')}
                            </p>
                        </div>

                        {deleteError && (
                            <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                                <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                                <p className="text-xs font-bold text-rose-800 leading-tight">{deleteError}</p>
                            </div>
                        )}

                        <div className="flex gap-4">
                            <Button type="button" variant="secondary" onClick={() => { setShowDeleteConfirm(false); setDeleteError(""); }} className="flex-1 rounded-2xl">{t('common.abort')}</Button>
                            <Button type="button" onClick={handleDeleteProfile} disabled={isDeleting} className="flex-1 rounded-3xl !bg-rose-600 hover:!bg-rose-700 shadow-premium-lg">
                                {isDeleting ? t('profile.deleting') : t('profile.deleteProfile')}
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
                                <h2 className="text-2xl font-black text-slate-900 tracking-tight uppercase">{t('profile.deleteProfile')}</h2>
                                <p className="text-[10px] font-black uppercase tracking-widest text-slate-400">{t('profile.authorizeDeletionFor', { name: activeProfileName })}</p>
                            </div>
                        </div>
                        
                        <form className="space-y-6" onSubmit={(e) => { e.preventDefault(); handleVerifyDeletePassword(); }}>
                            <div className="space-y-2">
                                <label htmlFor="delete-password-input" className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('profile.walletPassword')}</label>
                                <Input 
                                    id="delete-password-input"
                                    type="password" 
                                    value={deletePassword} 
                                    onChange={(e) => {
                                        setDeletePassword(e.target.value);
                                        if (deleteError) setDeleteError("");
                                    }} 
                                    placeholder={t('common.required')} 
                                    autoFocus 
                                />
                            </div>

                            {deleteError && (
                                <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                                    <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                                    <p className="text-xs font-bold text-rose-800 leading-tight">{deleteError}</p>
                                </div>
                            )}

                            <div className="flex gap-4">
                                <Button type="button" variant="secondary" onClick={() => { setShowDeletePasswordPrompt(false); setDeletePassword(""); setDeleteError(""); }} disabled={isVerifyingDelete} className="flex-1 rounded-2xl">{t('common.cancel')}</Button>
                                <Button type="submit" disabled={isVerifyingDelete || !deletePassword} className="flex-1 rounded-3xl shadow-md">
                                    {isVerifyingDelete ? t('profile.verifying') : t('profile.verify')}
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
                                <h2 className="text-3xl font-black text-theme-primary tracking-tighter uppercase">{t('profile.handoverComplete')}</h2>
                                <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">{t('profile.handoverComplete')}</p>
                            </div>
                        </div>

                        <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                            <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">{t('profile.boundIdentity')}</p>
                            <p className="text-xs font-mono break-all text-theme-primary bg-white p-3 rounded-2xl border border-slate-100">{handoverUserId}</p>
                        </div>

                        <div className="p-6 bg-amber-50 border border-amber-100 rounded-[32px] space-y-4">
                            <div className="flex items-center gap-2 text-amber-600">
                                <AlertTriangle size={18} />
                                <h3 className="text-xs font-black uppercase tracking-widest">{t('profile.doubleSpendPrevention')}</h3>
                            </div>
                            <p className="text-sm font-medium text-amber-900 leading-relaxed">
                                {t('profile.doubleSpendWarning')}
                            </p>
                        </div>

                        <Button onClick={() => {
                            const loggedInProfile = profiles.find(p => p.folderName === selectedProfile);
                            if (loggedInProfile) onLoginSuccess(loggedInProfile.profileName);
                        }} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            {t('profile.acknowledgeAccess')} <ArrowRight size={20} />
                        </Button>
                    </Card>
                </div>
            ) : (
                <>
                    <div className="flex items-center justify-center gap-3 sm:gap-6">
                        <img 
                            src={logo} 
                            alt="Human Money Logo" 
                            className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm" 
                        />
                        <div className="text-left space-y-0 sm:space-y-0.5">
                            <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">HUMAN MONEY</h1>
                            <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">
                                {profiles.length > 0 ? t('auth.loginToYourWallet') : t('dashboard.welcomeTitle')}
                            </p>
                        </div>
                    </div>

                    {profiles.length > 0 ? (
                        <form className="space-y-6" onSubmit={(e) => { e.preventDefault(); handleLogin(); }}>
                            <div className="space-y-3">
                                <div className="flex items-center gap-1.5">
                                    <label htmlFor="profile-select" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1">
                                        <UserCircle size={12}/> 
                                        <span>{t('profile.selectProfile')}</span>
                                    </label>
                                    <HelpIcon topic="profile" size={12} />
                                </div>
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
                                        onClick={() => {
                                            setShowDeletePasswordPrompt(true);
                                            setDeletePassword("");
                                            setDeleteError("");
                                        }}
                                        className="w-14 h-14 rounded-2xl bg-slate-50 border border-slate-200 text-slate-400 hover:text-rose-500 hover:bg-rose-50 hover:border-rose-200 transition-all flex items-center justify-center shadow-sm cursor-pointer"
                                        title={t('profile.deleteProfile')}
                                    >
                                        <Trash2 size={22} />
                                    </button>
                                </div>
                            </div>

                            <div className="space-y-3">
                                <label htmlFor="password-input" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Lock size={12}/> {t('auth.password')}</label>
                                <Input 
                                    id="password-input"
                                    type="password" 
                                    value={password} 
                                    onChange={(e) => { setPassword(e.target.value); if (feedbackMsg.includes("Verification")) setFeedbackMsg(""); }} 
                                    placeholder={t('auth.enterAccessPassword')}
                                    ref={passwordInputRef}
                                    className="py-5 px-6 text-lg font-bold tracking-widest"
                                />
                            </div>

                            <div className="space-y-6 pt-4">
                                {showHandoverUI ? (
                                    <div className="space-y-4 animate-in slide-in-from-bottom-4">
                                        <div className="flex items-center justify-between">
                                            <span className="text-[10px] font-black text-amber-600 uppercase tracking-widest flex items-center gap-1.5">
                                                <ShieldAlert size={12} />
                                                <span>{t('profile.performHandover')}</span>
                                            </span>
                                            <HelpIcon topic="handover" size={14} className="text-amber-600 animate-pulse" />
                                        </div>
                                        <Button 
                                            type="button" 
                                            onClick={handleHandover} 
                                            disabled={isLoggingIn} 
                                            className="w-full py-5 rounded-3xl !bg-amber-600 hover:!bg-amber-700 shadow-premium-lg text-lg gap-3"
                                        >
                                            {isLoggingIn ? <RefreshCw className="animate-spin" /> : <ShieldAlert size={24} />}
                                            {t('profile.performHandover')}
                                        </Button>
                                        <button 
                                            type="button" 
                                            onClick={() => { setShowHandoverUI(false); setFeedbackMsg(""); }} 
                                            disabled={isLoggingIn}
                                            className="w-full text-center text-[10px] font-black uppercase tracking-widest text-theme-light hover:text-theme-primary transition-colors cursor-pointer"
                                        >
                                            {t('profile.cancelProtocol')}
                                        </button>
                                    </div>
                                ) : (
                                    <Button type="submit" disabled={isLoading || isLoggingIn || profiles.length === 0 || !password || !selectedProfile} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                                        {isLoggingIn ? <RefreshCw className="animate-spin" size={24} /> : <LogIn size={24} />}
                                        {isLoggingIn ? t('auth.authorizing') : t('auth.login')}
                                    </Button>
                                )}

                                <div className="relative flex items-center justify-center my-6">
                                    <div className="absolute inset-0 flex items-center">
                                        <div className="w-full border-t border-theme-light-border"></div>
                                    </div>
                                    <span className="relative px-3 bg-white text-[10px] font-black uppercase tracking-widest text-theme-light">
                                        {t('common.or') || 'OR'}
                                    </span>
                                </div>

                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-5 pt-2">
                                    <button 
                                        type="button" 
                                        onClick={onSwitchToCreate} 
                                        className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
                                    >
                                        <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                                            <PlusCircle size={26} className="text-theme-primary" />
                                        </div>
                                        <div className="space-y-1">
                                            <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                                                {t('auth.createNewWalletTitle')}
                                            </h3>
                                            <p className="text-[10px] font-medium text-theme-light">
                                                {t('auth.createNewWalletSub')}
                                            </p>
                                        </div>
                                    </button>
                                    <button 
                                        type="button" 
                                        onClick={onSwitchToRecreate} 
                                        className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
                                    >
                                        <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                                            <RefreshCw size={24} className="text-theme-primary" />
                                        </div>
                                        <div className="space-y-1">
                                            <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                                                {t('auth.loadExistingWalletTitle')}
                                            </h3>
                                            <p className="text-[10px] font-medium text-theme-light">
                                                {t('auth.loadExistingWalletSub')}
                                            </p>
                                        </div>
                                    </button>
                                </div>

                                <button type="button" onClick={onSwitchToReset} className="w-full text-center text-[10px] font-black uppercase tracking-[0.2em] text-theme-light hover:text-theme-primary transition-colors flex items-center justify-center gap-2 cursor-pointer mt-4">
                                    <Key size={12} /> {t('auth.forgotPassword')}
                                </button>
                            </div>

                            {feedbackMsg && (
                                <div className={`p-5 rounded-[32px] border flex items-center gap-4 animate-in slide-in-from-bottom-4 ${feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') || feedbackMsg.includes('Mismatch') ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                                    {feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') || feedbackMsg.includes('Mismatch') ? <ShieldAlert size={20} className="text-rose-500 shrink-0" /> : <CheckCircle2 size={20} className="text-emerald-500 shrink-0" />}
                                    <div>
                                        <h4 className="text-[10px] font-black uppercase tracking-widest mb-1">{feedbackMsg.includes('Error') ? t('profile.verificationSystem') : t('profile.protocolAlert')}</h4>
                                        <p className="text-sm font-bold leading-tight">{feedbackMsg}</p>
                                    </div>
                                </div>
                            )}
                        </form>
                    ) : (
                        <div className="space-y-6 pt-6">
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-5 pt-2">
                                <button 
                                    type="button" 
                                    onClick={onSwitchToCreate} 
                                    className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
                                >
                                    <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                                        <PlusCircle size={26} className="text-theme-primary" />
                                    </div>
                                    <div className="space-y-1">
                                        <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                                            {t('auth.createNewWalletTitle')}
                                        </h3>
                                        <p className="text-[10px] font-medium text-theme-light">
                                            {t('auth.createNewWalletSub')}
                                        </p>
                                    </div>
                                </button>
                                <button 
                                    type="button" 
                                    onClick={onSwitchToRecreate} 
                                    className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
                                >
                                    <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                                        <RefreshCw size={24} className="text-theme-primary" />
                                    </div>
                                    <div className="space-y-1">
                                        <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                                            {t('auth.loadExistingWalletTitle')}
                                        </h3>
                                        <p className="text-[10px] font-medium text-theme-light">
                                            {t('auth.loadExistingWalletSub')}
                                        </p>
                                    </div>
                                </button>
                            </div>
                        </div>
                    )}

                    <div className="pt-4 border-t border-theme-subtle/40 flex flex-col items-center gap-1.5 text-center">
                        {localInstanceId && (
                            <p className="text-[9px] font-black text-slate-400 uppercase tracking-[0.3em] flex items-center gap-2">
                                <ShieldCheck size={12} /> {t('profile.deviceId')} <span className="font-mono">{localInstanceId.slice(0, 12)}...</span>
                            </p>
                        )}
                        <button
                            type="button"
                            onClick={() => navigate({ view: "concept" })}
                            className="text-xs text-theme-primary hover:text-theme-primary/85 font-bold hover:underline focus:outline-none flex items-center gap-1.5 mt-4 cursor-pointer transition-colors"
                        >
                            <BookOpen size={14} /> {t('dashboard.conceptReadConceptLink')}
                        </button>
                        <button
                            type="button"
                            onClick={async (e) => {
                                e.preventDefault();
                                try {
                                    await openUrl('https://menschlich-miteinander.org');
                                } catch (err) {
                                    logger.error(`Failed to open URL: ${err}`);
                                }
                            }}
                            className="text-xs text-theme-light hover:text-theme-secondary hover:underline focus:outline-none mt-2 font-medium cursor-pointer"
                        >
                            {t('auth.supportTeaserLink')}
                        </button>
                    </div>
                </>
            )}
        </AuthLayout>
    );
}