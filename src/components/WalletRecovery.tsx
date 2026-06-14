// src/components/WalletRecovery.tsx
import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";
import { profileService } from "../services/profileService";
import { authService } from "../services/authService";
import { AuthLayout } from "./AuthLayout";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Card } from "./ui/Card";
import { ProfileInfo, MnemonicLanguage } from "../types";
import { translateError } from "../utils/errorHelper";
import { HelpIcon } from "./ui/HelpIcon";
import {
    Key,
    Lock,
    Fingerprint,
    Languages,
    BookOpen,
    CheckCircle2,
    ArrowLeft,
    RefreshCw,
    ShieldAlert,
    Info,
    User,
    Grid,
    Type
} from "lucide-react";

interface WalletRecoveryProps {
    onRecoverySuccess: () => void;
    onSwitchToLogin: () => void;
}
type InputMode = "words" | "phrase";

export function WalletRecovery({ onRecoverySuccess, onSwitchToLogin }: WalletRecoveryProps) {
    const { t } = useTranslation();
    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [selectedProfile, setSelectedProfile] = useState<string>("");

    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>("english");
    const [mnemonicWords, setMnemonicWords] = useState<string[]>(Array(12).fill(""));
    const [inputMode, setInputMode] = useState<InputMode>("words");

    const [passphrase, setPassphrase] = useState("");
    const [newPassword, setNewPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");

    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [isValidMnemonic, setIsValidMnemonic] = useState(false);

    const [rawPhrase, setRawPhrase] = useState("");
    const [bip39Wordlist, setBip39Wordlist] = useState<string[]>([]);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const prevRawPhraseRef = useRef("");

    useEffect(() => {
        logger.info("WalletRecovery component displayed");
        const systemLang = navigator.language || "en";
        let detectedLanguage: MnemonicLanguage = "english";
        const langMap: Record<string, MnemonicLanguage> = {
            "de": "german", "es": "spanish", "fr": "french", "it": "italian",
            "ja": "japanese", "ko": "korean", "pt": "portuguese", "cs": "czech",
            "zh-CN": "chineseSimplified", "zh-TW": "chineseTraditional"
        };
        for (const [key, val] of Object.entries(langMap)) {
            if (systemLang.startsWith(key)) { detectedLanguage = val; break; }
        }
        setSelectedLanguage(detectedLanguage);

        async function fetchProfiles() {
            setIsLoading(true);
            try {
                const availableProfiles = await authService.listProfiles();
                setProfiles(availableProfiles);
                if (availableProfiles.length > 0) setSelectedProfile(availableProfiles[0].folderName);
                else                 setFeedbackMsg(t('auth.noProfilesToRecover'));
            } catch (e) {
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProfiles();
    }, [t]);

    useEffect(() => {
        setMnemonicWords(prev => {
            const currentWords = prev.join(" ").split(" ").filter(Boolean);
            return Array(wordCount).fill("").map((_, i) => currentWords[i] || "");
        });
    }, [wordCount]);

    useEffect(() => {
        async function fetchWordlist() {
            try {
                const list = await profileService.getWordlist(selectedLanguage);
                setBip39Wordlist(list);
            } catch (e) {
                logger.error(`Failed to fetch BIP-39 wordlist: ${translateError(e, t)}`);
            }
        }
        fetchWordlist();
    }, [selectedLanguage, t]);

    useEffect(() => {
        if (inputMode !== 'phrase') return;
        const prevPhrase = prevRawPhraseRef.current;
        const needsCleaning = /[0-9]+\.|\n|\r/.test(rawPhrase);

        if (needsCleaning) {
            const singleLine = rawPhrase.replace(/(\r\n|\n|\r)/gm, " ");
            const cleaned = singleLine.replace(/[0-9]+\./g, '').trim().replace(/\s+/g, ' ').toLowerCase();
            if (cleaned !== rawPhrase) { setRawPhrase(cleaned); return; }
        }

        const words = rawPhrase.trim().replace(/\s+/g, ' ').split(' ').filter(Boolean);
        setMnemonicWords(Array(wordCount).fill("").map((_, i) => words[i] || ""));
        if (words.length === 12 || words.length === 24) setWordCount(words.length);

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

    useEffect(() => {
        const validate = async () => {
            const nonEmptyWords = mnemonicWords.filter(Boolean);
            if (nonEmptyWords.length > 0 && mnemonicWords.every(word => word && word.length > 1)) {
                const fullMnemonic = mnemonicWords.join(" ");
                try {
                    await profileService.validateMnemonic(fullMnemonic, selectedLanguage);
                    setIsValidMnemonic(true);
                    setFeedbackMsg(t('auth.seedPhraseValid'));
                } catch (e) {
                    setIsValidMnemonic(false);
                    setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
                }
            } else {
                setIsValidMnemonic(false);
                setFeedbackMsg(nonEmptyWords.length > 0 ? t('auth.awaitingSequence') : "");
            }
        };
        validate();
    }, [mnemonicWords, selectedLanguage, wordCount, t]);

    const handleWordChange = (index: number, value: string) => {
        const cleanedText = value.replace(/[0-9]+\.\s*/g, '');
        const pastedWords = cleanedText.trim().replace(/\s+/g, ' ').split(' ');
        const newWords = [...mnemonicWords];
        for (let i = 0; i < pastedWords.length; i++) {
            const targetIndex = index + i;
            if (targetIndex < wordCount) newWords[targetIndex] = pastedWords[i].toLowerCase().trim();
        }
        setMnemonicWords(newWords);
        if (pastedWords.length > 1) {
            const nextIndex = index + pastedWords.length;
            if (nextIndex < wordCount) document.getElementById(`word-${nextIndex}`)?.focus();
        }
    };

    const handleInputModeToggle = () => {
        const newMode = inputMode === "words" ? "phrase" : "words";
        if (newMode === 'phrase') setRawPhrase(mnemonicWords.join(" ").trim());
        setInputMode(newMode);
    };

    async function handleRecovery() {
        if (!selectedProfile) { setFeedbackMsg(t('auth.selectProfileToRecover')); return; }
        if (!isValidMnemonic) { setFeedbackMsg(t('auth.seedPhraseInvalid')); return; }
        if (newPassword !== confirmPassword) { setFeedbackMsg(t('auth.passwordsDontMatch')); return; }
        if (newPassword.length < 8) { setFeedbackMsg(t('auth.passwordMinLength')); return; }

        setIsLoading(true);
        setFeedbackMsg(t('auth.recoveringWallet'));
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await profileService.recoverWallet({
                folderName: selectedProfile,
                mnemonic: mnemonicWords.join(" "),
                passphrase: passphrase || undefined,
                newPassword,
                localInstanceId,
                language: selectedLanguage,
            });
            onRecoverySuccess();
        } catch (e) {
            setFeedbackMsg(`${t('auth.errorRecoveringWallet')}: ${translateError(e, t)}`);
        } finally {
            setIsLoading(false);
        }
    }

    return (
        <AuthLayout maxWidth="max-w-3xl">
            <div className="flex items-center justify-center gap-3 sm:gap-6">
                <img
                    src={logo}
                    alt="Human Money Logo"
                    className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm"
                />
                <div className="text-left space-y-0 sm:space-y-0.5">
                    <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">HUMAN MONEY</h1>
                    <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">{t('auth.recoverWallet')}</p>
                </div>
            </div>

            <form className="space-y-8" onSubmit={(e) => { e.preventDefault(); handleRecovery(); }}>
                    <Card header={<div className="flex items-center gap-2"><User size={14}/><label htmlFor="profile-select" className="font-black text-[10px] uppercase tracking-widest cursor-pointer">{t('profile.selectProfile')}</label></div>}>
                        <div className="space-y-4">
                            {profiles.length > 0 ? (
                                <select
                                    id="profile-select"
                                    value={selectedProfile}
                                    onChange={(e) => setSelectedProfile(e.target.value)}
                                    data-testid="profile-select"
                                    className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3.5 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                >
                                    {profiles.map((profile) => (
                                        <option key={profile.folderName} value={profile.folderName}>
                                            {profile.profileName}
                                        </option>
                                    ))}
                                </select>
                            ) : (
                                <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3">
                                    <ShieldAlert size={20} className="text-rose-500" />
                                    <p className="text-xs font-bold text-rose-800">{t('auth.noProfilesToRecover')}</p>
                                </div>
                            )}
                        </div>
                    </Card>

                    <Card header={
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                                <Key size={14}/>
                                <span className="font-black text-[10px] uppercase tracking-widest">{t('auth.enterMasterKey')}</span>
                                <HelpIcon topic="mnemonic" size={12} />
                            </div>
                            <button type="button" onClick={handleInputModeToggle} className="px-3 py-1 bg-theme-primary/5 border border-theme-primary/10 rounded-full text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-2">
                                {inputMode === "words" ? <Type size={10}/> : <Grid size={10}/>}
                                {inputMode === "words" ? t('auth.enterFullPhrase') : t('auth.useSingleFields')}
                            </button>
                        </div>
                    }>
                        <div className="space-y-6">
                            <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Languages size={10}/> {t('common.dictionary')}</label>
                                    <select value={selectedLanguage} onChange={(e) => setSelectedLanguage(e.target.value as MnemonicLanguage)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-2.5 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none">
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
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><BookOpen size={10}/> {t('auth.sequenceDepth')}</label>
                                    <select value={wordCount} onChange={(e) => setWordCount(parseInt(e.target.value, 10) as 12 | 24)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-2.5 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none">
                                        <option value={12}>{t('profile.wordCount12')}</option>
                                        <option value={24}>{t('profile.wordCount24')}</option>
                                    </select>
                                </div>
                            </div>

                            {inputMode === 'words' ? (
                                <div className="grid grid-cols-3 sm:grid-cols-4 gap-3">
                                    {mnemonicWords.map((word, index) => (
                                        <div key={index} className="relative group">
                                            <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[9px] font-black text-theme-primary/30 w-4">{index + 1}</span>
                                            <Input
                                                id={`word-${index}`}
                                                data-testid={`word-input-${index}`}
                                                value={word}
                                                onChange={(e) => handleWordChange(index, e.target.value)}
                                                className="pl-7 font-mono font-bold text-xs py-2.5 group-hover:border-theme-primary/30"
                                                autoComplete="off"
                                            />
                                        </div>
                                    ))}
                                </div>
                            ) : (
                                <div className="space-y-3">
                                    <textarea
                                        ref={textareaRef}
                                        value={rawPhrase}
                                        onChange={(e) => setRawPhrase(e.target.value)}
                                        placeholder={t('auth.seedPhrasePlaceholder')}
                                        className="w-full h-32 px-4 py-3 bg-white border border-theme-subtle rounded-2xl text-theme-primary font-mono text-sm focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all"
                                        rows={4}
                                    />
                                    <p className="text-[9px] font-bold text-theme-light italic flex items-center gap-1.5"><Info size={10}/> {t('auth.autoCleaned')}</p>
                                </div>
                            )}

                            <div className="space-y-2 pt-4 border-t border-theme-primary/10">
                                <div className="flex items-center gap-1.5">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1">
                                        <Lock size={10}/> 
                                        <span>{t('auth.passphraseIfUsed')}</span>
                                    </label>
                                    <HelpIcon topic="passphrase" size={12} />
                                </div>
                                <Input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} placeholder={t('auth.passphrasePlaceholder')} />
                            </div>
                        </div>
                    </Card>

                    <Card header={<div className="flex items-center gap-2"><Lock size={14}/><span className="font-black text-[10px] uppercase tracking-widest">{t('auth.reEncryptionCredentials')}</span></div>}>
                        <div className="space-y-6">
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('auth.newWalletPassword')}</label>
                                    <Input type="password" value={newPassword} onChange={(e) => { setNewPassword(e.target.value); if (feedbackMsg.includes("match")) setFeedbackMsg(""); }} placeholder={t('auth.passwordMinChars')} onFocus={() => setFeedbackMsg("")} />
                                </div>
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('auth.confirmPassword')}</label>
                                    <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder={t('auth.repeatPassword')} />
                                </div>
                            </div>
                        </div>
                    </Card>

                    <div className="flex flex-col items-center gap-4 pt-4">
                        <Button type="submit" disabled={isLoading || !isValidMnemonic || profiles.length === 0} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            {isLoading ? <RefreshCw className="animate-spin" size={24} /> : <Fingerprint size={24} />}
                            {isLoading ? t('auth.loadingWallet') : t('auth.recoverWalletAndLogin')}
                        </Button>
                        <Button type="button" variant="secondary" onClick={onSwitchToLogin} className="w-full py-4 rounded-2xl shadow-sm gap-2">
                            <ArrowLeft size={18} /> {t('auth.backToLogin')}
                        </Button>
                    </div>

                    {feedbackMsg && (
                        <div data-testid="feedback-message" className={`p-4 rounded-2xl flex items-center gap-3 border animate-in slide-in-from-bottom-2 ${feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                            {feedbackMsg.includes('Error') || feedbackMsg.includes('Failure') ? <ShieldAlert size={18} className="text-rose-500 shrink-0" /> : <CheckCircle2 size={18} className="text-emerald-500 shrink-0" />}
                            <p className="text-xs font-bold leading-tight">{feedbackMsg}</p>
                        </div>
                    )}
                </form>
        </AuthLayout>
    );
}
