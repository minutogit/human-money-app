import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Globe, ChevronDown, Check } from 'lucide-react';

interface LanguageSelectorProps {
    variant?: 'compact' | 'card';
}

export function LanguageSelector({ variant = 'compact' }: LanguageSelectorProps) {
    const { i18n } = useTranslation();
    const [isOpen, setIsOpen] = useState(false);
    const dropdownRef = useRef<HTMLDivElement>(null);

    const languages = [
        { code: 'en', label: 'English', flag: '🇺🇸' },
        { code: 'de', label: 'Deutsch', flag: '🇩🇪' }
    ];

    const currentLangCode = (i18n.language || '').startsWith('de') ? 'de' : 'en';
    const currentLang = languages.find(l => l.code === currentLangCode) || languages[0];

    useEffect(() => {
        function handleClickOutside(event: MouseEvent) {
            if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
                setIsOpen(false);
            }
        }
        document.addEventListener('mousedown', handleClickOutside);
        return () => {
            document.removeEventListener('mousedown', handleClickOutside);
        };
    }, []);

    const handleLanguageChange = (code: string) => {
        i18n.changeLanguage(code);
        localStorage.setItem('app_language', code);
        setIsOpen(false);
    };

    if (variant === 'card') {
        return (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                {languages.map((lang) => {
                    const isSelected = currentLangCode === lang.code;
                    return (
                        <button
                            key={lang.code}
                            type="button"
                            onClick={() => handleLanguageChange(lang.code)}
                            className={`relative flex items-center gap-3 p-4 rounded-2xl border-2 transition-all cursor-pointer group text-left ${
                                isSelected 
                                    ? 'bg-theme-primary/5 border-theme-primary shadow-sm' 
                                    : 'bg-white border-theme-subtle hover:border-theme-primary/30'
                            }`}
                        >
                            <span className="text-2xl">{lang.flag}</span>
                            <span className={`text-sm font-black tracking-tight ${isSelected ? 'text-theme-primary' : 'text-theme-secondary'}`}>
                                {lang.label}
                            </span>
                            {isSelected && (
                                <div className="absolute top-3 right-3 text-theme-primary">
                                    <Check size={16} />
                                </div>
                            )}
                        </button>
                    );
                })}
            </div>
        );
    }

    // Default: 'compact' custom dropdown
    return (
        <div className="relative inline-block text-left" ref={dropdownRef}>
            <button
                type="button"
                onClick={() => setIsOpen(!isOpen)}
                className="flex items-center gap-2 px-3 py-2 bg-white/85 hover:bg-white border border-theme-subtle hover:border-theme-primary/30 rounded-xl shadow-sm transition-all text-xs font-bold text-theme-secondary cursor-pointer"
            >
                <Globe size={14} className="text-theme-primary" />
                <span>{currentLang.flag}</span>
                <span className="hidden sm:inline font-bold">{currentLang.label}</span>
                <ChevronDown size={12} className={`transition-transform duration-200 text-theme-light ${isOpen ? 'rotate-180' : ''}`} />
            </button>

            {isOpen && (
                <div className="absolute right-0 mt-1.5 w-40 rounded-xl bg-white border border-theme-subtle shadow-premium z-50 py-1 overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150">
                    {languages.map((lang) => {
                        const isSelected = currentLangCode === lang.code;
                        return (
                            <button
                                key={lang.code}
                                type="button"
                                onClick={() => handleLanguageChange(lang.code)}
                                className={`w-full flex items-center justify-between px-4 py-2.5 text-left text-xs transition-colors hover:bg-theme-primary/5 cursor-pointer ${
                                    isSelected ? 'font-black text-theme-primary bg-theme-primary/5' : 'font-medium text-theme-secondary'
                                }`}
                            >
                                <div className="flex items-center gap-2">
                                    <span>{lang.flag}</span>
                                    <span>{lang.label}</span>
                                </div>
                                {isSelected && <Check size={12} className="text-theme-primary" />}
                            </button>
                        );
                    })}
                </div>
            )}
        </div>
    );
}
