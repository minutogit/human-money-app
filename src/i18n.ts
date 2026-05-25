import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import de from './locales/de.json';
import fr from './locales/fr.json';
import es from './locales/es.json';
import it from './locales/it.json';

const supportedLngs = ['de', 'en', 'es', 'fr', 'it'];
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
      fr: { translation: fr },
      es: { translation: es },
      it: { translation: it },
    },
    lng: initialLng,
    fallbackLng: 'en',   // Fallback: Englisch (für nicht übersetzte Keys)
    keySeparator: false, // Prevents i18next from parsing dots as nested objects
    interpolation: {
      escapeValue: false, // React escaped bereits
    },
  });

export default i18n;
