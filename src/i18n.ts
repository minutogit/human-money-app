import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import de from './locales/de.json';

const supportedLngs = ['de', 'en'];
const savedLng = localStorage.getItem('app_language');
let initialLng = 'en';

if (savedLng && supportedLngs.includes(savedLng)) {
  initialLng = savedLng;
} else {
  const systemLng = navigator.language ? navigator.language.split('-')[0] : 'en';
  if (supportedLngs.includes(systemLng)) {
    initialLng = systemLng;
  }
}

i18n
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      de: { translation: de },
    },
    lng: initialLng,
    fallbackLng: 'en',   // Fallback: Englisch (für nicht übersetzte Keys)
    keySeparator: false, // Prevents i18next from parsing dots as nested objects
    interpolation: {
      escapeValue: false, // React escaped bereits
    },
  });

export default i18n;
