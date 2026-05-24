// src/components/CreateNewProfile.tsx
import { useState, useEffect, useRef, FormEvent } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";
import { profileService } from "../services/profileService";
import { authService } from "../services/authService";
import { AuthLayout } from "./AuthLayout";
import { logger } from "../utils/log";
import { MnemonicLanguage } from "../types";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Card } from "./ui/Card";
import { translateError } from "../utils/errorHelper";
import {
    ShieldCheck,
    Lock,
    Fingerprint,
    Languages,
    BookOpen,
    AlertTriangle,
    CheckCircle2,
    ArrowRight,
    ArrowLeft,
    RefreshCw,
    ShieldAlert,
    Info,
    User,
    HelpCircle
} from "lucide-react";
import { PrefixInfoModal } from "./ui/PrefixInfoModal";

type WizardStep = "display_seed" | "confirm_seed" | "set_password";

interface ConfirmationWord {
    index: number;
    value: string;
}

interface CreateNewProfileProps {
    onProfileCreated: () => void;
    onSwitchToRecreate: () => void;
    onSwitchToLogin?: () => void;
}

export function CreateNewProfile({ onProfileCreated, onSwitchToRecreate, onSwitchToLogin }: CreateNewProfileProps) {
    const { t } = useTranslation();
    const [wizardStep, setWizardStep] = useState<WizardStep>("display_seed");
    const [generatedSeed, setGeneratedSeed] = useState<string[]>([]);
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>("english");
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [isLoading, setIsLoading] = useState(false);

    const [confirmationWords, setConfirmationWords] = useState<ConfirmationWord[]>([]);
    const [userConfirmationInput, setUserConfirmationInput] = useState<{ [key: number]: string }>({});
    const [isBulkMode, setIsBulkMode] = useState(false);
    const [bulkSeedInput, setBulkSeedInput] = useState("");

    const [passphrase, setPassphrase] = useState<string>("");
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>("");
    const [profileName, setProfileName] = useState("");
    const [userPrefix, setUserPrefix] = useState("");
    const [isSubAccount, setIsSubAccount] = useState(false);
    const [password, setPassword] = useState("");
    const [confirmPassword, setConfirmPassword] = useState("");
    const [showPrefixInfo, setShowPrefixInfo] = useState(false);
    const passwordInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        logger.info("CreateNewProfile component displayed");
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
    }, []);

    useEffect(() => {
        async function generateNewSeed() {
            setIsLoading(true);
            try {
                const newSeed: string = await profileService.generateMnemonic(wordCount, selectedLanguage);
                setGeneratedSeed(newSeed.split(' '));
            } catch (e) {
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
            } finally {
                setIsLoading(false);
            }
        }
        generateNewSeed();
    }, [wordCount, selectedLanguage, t]);

    useEffect(() => {
        if (!isBulkMode) return;
        const cleanSeedText = (text: string) => text.toLowerCase().replace(/[0-9.,\-:]/g, ' ').replace(/[\r\n\t]/g, ' ').replace(/\s+/g, ' ').trim();
        const cleaned = cleanSeedText(bulkSeedInput);
        if ((/[0-9.,\-:]/.test(bulkSeedInput) || /\s\s/.test(bulkSeedInput)) && cleaned !== bulkSeedInput) setBulkSeedInput(cleaned);
    }, [bulkSeedInput, isBulkMode]);

    const prepareConfirmationStep = () => {
        const shuffled = [...generatedSeed].map((word, index) => ({ word, index }));
        shuffled.sort(() => 0.5 - Math.random());
        const selected = shuffled.slice(0, 3).map(item => ({ index: item.index, value: "" }));
        selected.sort((a, b) => a.index - b.index);
        setConfirmationWords(selected);
        setUserConfirmationInput({});
        setBulkSeedInput("");
        setIsBulkMode(false);
        setFeedbackMsg("");
        setWizardStep("confirm_seed");
    };

    const cleanSeedText = (text: string) => text.toLowerCase().replace(/[0-9.,\-:]/g, ' ').replace(/[\r\n\t]/g, ' ').replace(/\s+/g, ' ').trim();

    const handleConfirmSeedSubmit = (e: FormEvent) => {
        e.preventDefault();
        
        // Passphrase confirmation
        if (passphrase !== confirmPassphrase) {
            setFeedbackMsg(t('profile.passphraseMismatch'));
            return;
        }

        if (isBulkMode) {
            if (cleanSeedText(bulkSeedInput) !== generatedSeed.join(' ')) {
                setFeedbackMsg(t('profile.seedMismatch'));
                return;
            }
        } else {
            for (const word of confirmationWords) {
                if (userConfirmationInput[word.index] !== generatedSeed[word.index]) {
                    setFeedbackMsg(t('profile.seedWordsIncorrect'));
                    return;
                }
            }
        }
        setFeedbackMsg(t('profile.seedConfirmed'));
        setTimeout(() => { setFeedbackMsg(""); setWizardStep("set_password"); }, 1000);
    };

    async function handleCreateProfileSubmit(e: FormEvent) {
        e.preventDefault();
        if (password !== confirmPassword) { setFeedbackMsg(t('profile.passwordMismatch')); return; }
        if (password.length < 8) { setFeedbackMsg(t('profile.passwordMinLength')); return; }
        if (passphrase !== confirmPassphrase) { setFeedbackMsg(t('profile.passphraseMismatch')); return; }

        setIsLoading(true);
        setFeedbackMsg(t('profile.creatingProgress'));
        setTimeout(async () => {
            try {
                const localInstanceId = await authService.getLocalInstanceId();
                await profileService.createProfile({
                    profileName,
                    mnemonic: generatedSeed.join(' '),
                    passphrase: passphrase || undefined,
                    userPrefix: isSubAccount ? (userPrefix || undefined) : undefined,
                    password,
                    localInstanceId,
                    language: selectedLanguage,
                });
                onProfileCreated();
            } catch (e) {
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
            } finally {
                setIsLoading(false);
            }
        }, 150);
    }

    const renderContent = () => {
        switch (wizardStep) {
            case "display_seed":
                return (
                    <div className="space-y-6 sm:space-y-8 animate-in fade-in duration-500">
                        <div className="text-center space-y-2">
                            <h2 className="text-2xl font-black text-theme-primary tracking-tight">{t('profile.step1Title')}</h2>
                            <p className="text-sm font-medium text-theme-light">{t('profile.step1Subtitle')}</p>
                        </div>

                        <div className="p-6 bg-rose-50 border-2 border-rose-100 rounded-[32px] flex items-start gap-4 shadow-sm">
                            <ShieldAlert className="text-rose-500 shrink-0 mt-1" size={24} />
                            <div>
                                <h3 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">{t('profile.warning')}</h3>
                                <p className="text-xs text-rose-800 font-medium leading-relaxed">
                                    {t('profile.seedWarning')}
                                </p>
                            </div>
                        </div>

                        <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                            {generatedSeed.map((word, index) => (
                                <div key={index} data-testid={`word-display-${index}`} className="flex items-center gap-3 p-3 bg-theme-primary/5 border border-theme-primary/10 rounded-2xl shadow-inner-soft group hover:bg-white transition-all hover:shadow-md hover:scale-[1.02] duration-300">
                                    <span className="text-[10px] font-black text-theme-primary/40 w-5">{index + 1}</span>
                                    <span className="text-sm font-mono font-bold text-theme-secondary tracking-tight">{word}</span>
                                </div>
                            ))}
                        </div>

                        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
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
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><BookOpen size={10}/> {t('profile.wordCount')}</label>
                                <select value={wordCount} onChange={(e) => setWordCount(parseInt(e.target.value, 10) as 12 | 24)} className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-2.5 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none">
                                    <option value={12}>{t('profile.wordCountStandard')}</option>
                                    <option value={24}>{t('profile.wordCountHigh')}</option>
                                </select>
                            </div>
                        </div>

                        <div className="space-y-4 pt-4 border-t border-theme-primary/10">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><Lock size={10}/> {t('profile.optionalPassphrase', { ordinal: wordCount === 12 ? '13th' : '25th' })}</label>
                                <Input 
                                    type="text" 
                                    value={passphrase} 
                                    onChange={(e) => setPassphrase(e.target.value)} 
                                    placeholder={t('profile.passphrasePlaceholder')} 
                                    className="rounded-2xl"
                                />
                                <p className="text-[9px] font-bold text-amber-600 bg-amber-50 p-3 rounded-xl border border-amber-100 flex items-start gap-2">
                                    <AlertTriangle size={12} className="shrink-0 mt-0.5" />
                                    {t('profile.passphraseAdvanced', { wordCount })}
                                </p>
                            </div>
                        </div>

                        <div className="flex flex-col items-center gap-4 pt-4">
                            <Button size="lg" onClick={prepareConfirmationStep} disabled={isLoading || generatedSeed.length === 0} className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                                {isLoading ? <RefreshCw className="animate-spin" size={24} /> : <CheckCircle2 size={24} />}
                                {t('profile.savedSeed')}
                            </Button>
                            <Button type="button" variant="secondary" onClick={onSwitchToLogin} className="w-full py-3 rounded-2xl shadow-sm text-sm gap-2">
                                <ArrowLeft size={18} /> {t('auth.backToLogin')}
                            </Button>
                            <button onClick={onSwitchToRecreate} className="text-[10px] font-black uppercase tracking-widest text-theme-light hover:text-theme-primary transition-colors flex items-center gap-2">
                                <RefreshCw size={12} /> {t('profile.recreateHere')}
                            </button>
                        </div>
                    </div>
                );

            case "confirm_seed":
                return (
                    <form onSubmit={handleConfirmSeedSubmit} className="space-y-6 sm:space-y-8 animate-in slide-in-from-right-8 duration-500">
                        <div className="text-center space-y-2">
                            <h2 className="text-2xl font-black text-theme-primary tracking-tight">{t('profile.step2Title')}</h2>
                            <p className="text-sm font-medium text-theme-light">{t('profile.step2Subtitle')}</p>
                        </div>

                        <div className="flex justify-center">
                            <button type="button" onClick={() => { setIsBulkMode(!isBulkMode); setFeedbackMsg(""); }} className="px-4 py-1.5 bg-theme-primary/5 border border-theme-primary/10 rounded-full text-[10px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-2">
                                <Languages size={12} /> {isBulkMode ? t('profile.interactiveMode') : t('profile.switchBulkPaste')}
                            </button>
                        </div>

                        {isBulkMode ? (
                            <div className="space-y-3">
                                <label htmlFor="bulk-seed-input" className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.pasteFullSeed')}</label>
                                <textarea
                                    id="bulk-seed-input"
                                    value={bulkSeedInput}
                                    onChange={(e) => setBulkSeedInput(cleanSeedText(e.target.value))}
                                    className="w-full h-32 px-4 py-3 bg-white border border-theme-subtle rounded-2xl text-theme-primary font-mono text-sm focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all"
                                    placeholder={t('profile.pasteHere')}
                                    required
                                />
                                <p className="text-[9px] font-bold text-theme-light italic flex items-center gap-1.5"><Info size={10}/> {t('profile.formattingAuto')}</p>
                            </div>
                        ) : (
                            <div className="space-y-5">
                                {confirmationWords.map(({ index }) => (
                                    <div key={index} className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.verifyWord', { number: index + 1 })}</label>
                                        <Input
                                            type="text"
                                            value={userConfirmationInput[index] || ""}
                                            onChange={(e) => handleConfirmationInputChange(index, e.target.value)}
                                            required
                                            className="font-mono font-bold py-4 text-center text-lg"
                                            autoComplete="off"
                                            data-testid={`word-input-${index}`}
                                        />
                                    </div>
                                ))}
                            </div>
                        )}

                        {passphrase && (
                            <div className="space-y-2 pt-4 border-t border-theme-primary/10">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.confirmPassphrase')}</label>
                                <Input
                                    type="text"
                                    value={confirmPassphrase}
                                    onChange={(e) => setConfirmPassphrase(e.target.value)}
                                    required
                                    className="font-mono font-bold py-4 text-center text-lg"
                                    autoComplete="off"
                                    placeholder={t('profile.enterExtraWord')}
                                />
                            </div>
                        )}
                        
                        <div className="flex gap-4 pt-4">
                            <Button type="button" variant="secondary" onClick={() => setWizardStep("display_seed")} className="flex-1 rounded-2xl gap-2">
                                <ArrowLeft size={18} /> {t('profile.back')}
                            </Button>
                            <Button type="submit" className="flex-[2] rounded-3xl py-4 shadow-premium-lg gap-2">
                                {t('profile.confirmSeed')} <ArrowRight size={18} />
                            </Button>
                        </div>
                    </form>
                );

            case "set_password":
                return (
                    <form onSubmit={handleCreateProfileSubmit} className="space-y-6 sm:space-y-8 animate-in slide-in-from-right-8 duration-500 pb-10">
                        <div className="text-center space-y-2">
                            <h2 className="text-2xl font-black text-theme-primary tracking-tight">{t('profile.step3Title')}</h2>
                            <p className="text-sm font-medium text-theme-light">{t('profile.step3Subtitle')}</p>
                        </div>

                        <div className="space-y-6">
                            <Card header={<div className="flex items-center gap-2"><Lock size={14}/><span className="font-black text-[10px] uppercase tracking-widest">{t('profile.profileDetails')}</span></div>}>
                                <div className="space-y-4">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"><User size={10}/> {t('profile.profileLabel')}</label>
                                        <Input value={profileName} onChange={(e) => setProfileName(e.target.value)} placeholder={t('profile.profileNamePlaceholder')} required />
                                    </div>
                                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.accessPassword')}</label>
                                            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="••••••••" required ref={passwordInputRef} />
                                        </div>
                                        <div className="space-y-2">
                                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('profile.confirmPassword')}</label>
                                            <Input type="password" value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} placeholder="••••••••" required />
                                        </div>
                                    </div>
                                </div>
                            </Card>

                            <Card header={<div className="flex items-center gap-2"><ShieldCheck size={14}/><span className="font-black text-[10px] uppercase tracking-widest">{t('profile.deviceSettings')}</span></div>}>
                                <div className="space-y-4">
                                    <div className="flex items-start gap-2.5 py-1">
                                        <input
                                            id="checkbox-sub-account"
                                            type="checkbox"
                                            checked={isSubAccount}
                                            onChange={(e) => {
                                                setIsSubAccount(e.target.checked);
                                                if (!e.target.checked) {
                                                    setUserPrefix("");
                                                }
                                            }}
                                            className="mt-1 h-4.5 w-4.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary/30 cursor-pointer"
                                        />
                                        <div className="flex flex-col">
                                            <label htmlFor="checkbox-sub-account" className="text-xs font-bold text-theme-secondary cursor-pointer select-none">
                                                {t('profile.subAccountOption')}
                                            </label>
                                            <p className="text-[10px] font-medium text-theme-light leading-normal mt-0.5">
                                                {t('profile.subAccountDescription')}
                                            </p>
                                        </div>
                                    </div>

                                    {isSubAccount && (
                                        <div className="space-y-2 pt-2 border-t border-theme-primary/5 animate-in fade-in duration-300">
                                            <div className="flex items-center justify-between">
                                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                                                    {t('profile.subAccountName')}
                                                </label>
                                                <button 
                                                    type="button" 
                                                    onClick={() => setShowPrefixInfo(true)}
                                                    className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20"
                                                >
                                                    <HelpCircle size={12} />
                                                    <span>{t('profile.readInstructions')}</span>
                                                </button>
                                            </div>
                                            <Input 
                                                value={userPrefix} 
                                                onChange={(e) => setUserPrefix(e.target.value)} 
                                                placeholder={t('profile.prefixPlaceholder')} 
                                            />
                                        </div>
                                    )}

                                    <div className="space-y-1.5 p-3.5 bg-theme-subtle/20 border border-theme-subtle rounded-2xl">
                                        <p className="text-[10px] text-theme-secondary leading-relaxed">
                                            {t('profile.prefixDescription')}
                                        </p>
                                        <p className="text-[9px] font-black text-rose-600 flex items-start gap-1.5 leading-tight italic">
                                            <AlertTriangle size={10} className="shrink-0 mt-0.5" />
                                            {t('profile.criticalPrefix')}
                                        </p>
                                    </div>
                                </div>
                            </Card>

                            <PrefixInfoModal 
                                isOpen={showPrefixInfo} 
                                onClose={() => setShowPrefixInfo(false)} 
                            />
                        </div>

                        <div className="flex flex-col items-center gap-4 pt-4">
                            <div className="flex gap-4 w-full">
                                <Button type="button" variant="secondary" onClick={() => setWizardStep("confirm_seed")} className="flex-1 py-4 rounded-2xl gap-2">
                                    <ArrowLeft size={18} /> {t('profile.back')}
                                </Button>
                                <Button type="submit" disabled={isLoading} className="flex-[2] py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                                    {isLoading ? <RefreshCw className="animate-spin" size={24} /> : <Fingerprint size={24} />}
                                    {isLoading ? t('profile.creating') : t('profile.create')}
                                </Button>
                            </div>
                            <p className="text-[10px] font-bold text-theme-light flex items-center gap-2">
                                <Lock size={12} /> {t('profile.dataEncrypted')}
                            </p>
                        </div>
                    </form>
                );
        }
    };

    const handleConfirmationInputChange = (index: number, value: string) => {
        setUserConfirmationInput(prev => ({ ...prev, [index]: value.trim().toLowerCase() }));
    };

    return (
        <AuthLayout maxWidth="max-w-2xl">
            <div className="flex items-center justify-center gap-3 sm:gap-6">
                <img
                    src={logo}
                    alt="Human Money Logo"
                    className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm"
                />
                <div className="text-left space-y-0 sm:space-y-0.5">
                    <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">HUMAN MONEY</h1>
                    <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">{t('profile.createNewProfile')}</p>
                </div>
            </div>

            {renderContent()}

                {feedbackMsg && !isLoading && (
                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                        <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                        <p className="text-sm font-bold text-rose-800 leading-tight">{feedbackMsg}</p>
                    </div>
                )}
        </AuthLayout>
    );
}
