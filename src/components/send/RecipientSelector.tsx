import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { User, UserPlus, Search, X, AlertTriangle, ClipboardPaste, QrCode, Camera } from 'lucide-react';
import Avatar from 'boring-avatars';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { ModalShell } from '../ui/ModalShell';
import { ContactBadge } from '../ui/ContactBadge';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { readFromClipboard } from '../../utils/clipboard';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';
import { Contact, TrustStatus } from '../../types';

interface BarcodeDetectorInstance {
  detect: (source: HTMLVideoElement) => Promise<Array<{ rawValue?: string }>>;
}

type BarcodeDetectorConstructor = new (options?: { formats?: string[] }) => BarcodeDetectorInstance;

interface RecipientSelectorProps {
  recipientId: string;
  onRecipientChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onApplyRawCode?: (raw: string) => boolean;
  onSelectContact: (contact: Contact) => void;
  contacts: Contact[];
  trustStatus: TrustStatus;
  recipientError: boolean;
  onShowContactPicker: () => void;
  onClearRecipient: () => void;
}

function getContactDisplayName(contact: Contact, anonymousLabel: string): string {
  const fullName = `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim();
  return fullName || contact.profile.organization || anonymousLabel;
}

export function RecipientSelector({
  recipientId,
  onRecipientChange,
  onApplyRawCode,
  onSelectContact,
  contacts,
  trustStatus,
  recipientError,
  onShowContactPicker,
  onClearRecipient
}: RecipientSelectorProps) {
  const { t } = useTranslation();
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [scannerOpen, setScannerOpen] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const scanTimerRef = useRef<number | null>(null);

  const stopCamera = useCallback(() => {
    if (scanTimerRef.current) {
      clearTimeout(scanTimerRef.current);
      scanTimerRef.current = null;
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    }
    if (videoRef.current) {
      videoRef.current.srcObject = null;
    }
  }, []);

  const startCamera = useCallback(async () => {
    setCameraError(null);
    if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
      setCameraError(t('contacts.cameraNotSupported', 'Kamera auf diesem Gerät nicht verfügbar.'));
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' },
      });
      streamRef.current = stream;
      if (videoRef.current) {
        videoRef.current.srcObject = stream;
        videoRef.current.play().catch(() => {});
      }
    } catch (err) {
      logger.info(`Camera access not available or denied: ${stringifyError(err)}`);
      setCameraError(t('contacts.cameraAccessDenied', 'Kamera konnte nicht gestartet werden. Bitte erlaube den Kamerazugriff oder nutze eine andere Methode.'));
    }
  }, [t]);

  useEffect(() => {
    if (!scannerOpen) {
      stopCamera();
      return;
    }
    startCamera();
    return () => {
      stopCamera();
    };
  }, [scannerOpen, startCamera, stopCamera]);

  useEffect(() => {
    if (!scannerOpen) return;
    let isScanning = true;
    let detector: BarcodeDetectorInstance | null = null;

    if (typeof window !== 'undefined' && 'BarcodeDetector' in window) {
      try {
        const DetectorCtor = (window as unknown as { BarcodeDetector: BarcodeDetectorConstructor }).BarcodeDetector;
        detector = new DetectorCtor({ formats: ['qr_code'] });
      } catch (err) {
        logger.info(`BarcodeDetector creation failed: ${stringifyError(err)}`);
      }
    }

    const scanLoop = async () => {
      if (!isScanning) return;
      if (detector && videoRef.current && videoRef.current.readyState >= 2) {
        try {
          const barcodes = await detector.detect(videoRef.current);
          if (barcodes && barcodes.length > 0 && barcodes[0].rawValue) {
            const raw = barcodes[0].rawValue;
            isScanning = false;
            stopCamera();
            setScannerOpen(false);
            onApplyRawCode?.(raw);
            return;
          }
        } catch {
          // Continue scanning on frame detection errors
        }
      }
      if (isScanning && scannerOpen) {
        scanTimerRef.current = window.setTimeout(scanLoop, 250);
      }
    };

    scanTimerRef.current = window.setTimeout(scanLoop, 400);
    return () => {
      isScanning = false;
      if (scanTimerRef.current) {
        clearTimeout(scanTimerRef.current);
        scanTimerRef.current = null;
      }
    };
  }, [scannerOpen, stopCamera, onApplyRawCode]);

  const suggestions = React.useMemo(() => {
    if (!recipientId || recipientId.length < 2) return [];
    const q = recipientId.toLowerCase();
    return contacts.filter(c =>
      c.did.toLowerCase().includes(q) ||
      (c.profile.firstName || '').toLowerCase().includes(q) ||
      (c.profile.lastName || '').toLowerCase().includes(q) ||
      (c.profile.organization || '').toLowerCase().includes(q)
    ).slice(0, 5);
  }, [recipientId, contacts]);

  const quickContacts = React.useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    return contacts
      .filter(c =>
        !q ||
        c.did.toLowerCase().includes(q) ||
        (c.profile.firstName || '').toLowerCase().includes(q) ||
        (c.profile.lastName || '').toLowerCase().includes(q) ||
        (c.profile.organization || '').toLowerCase().includes(q)
      )
      .slice(0, 5);
  }, [contacts, searchQuery]);

  const currentContact = contacts.find(c => c.did === recipientId);

  const handleSearchOrCodeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    const trimmed = val.trim();
    if (
      onApplyRawCode &&
      (trimmed.includes('humoco://pay') ||
        trimmed.includes('humoco://contact') ||
        trimmed.startsWith('did:') ||
        trimmed.includes('@did:'))
    ) {
      const ok = onApplyRawCode(trimmed);
      if (ok) {
        setSearchQuery('');
        return;
      }
    }
    setSearchQuery(val);
  };

  const handlePasteCode = async () => {
    if (!onApplyRawCode) return;
    try {
      const { text } = await readFromClipboard();
      if (text && text.trim()) {
        const ok = onApplyRawCode(text.trim());
        if (ok) {
          setSearchQuery('');
        }
      } else {
        onApplyRawCode('');
      }
    } catch (err) {
      logger.info(`Clipboard read failed: ${stringifyError(err)}`);
    }
  };

  return (
    <Card
      className="!overflow-visible"
      header={
        <div className="flex items-center justify-between gap-2 w-full">
          <div className="flex items-center gap-2">
            <User size={18} className="text-theme-primary"/>
            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.recipient')}</span>
          </div>
          <button type="button" onClick={onShowContactPicker} className="text-[10px] font-black uppercase tracking-widest text-theme-primary hover:underline flex items-center gap-1.5 transition-colors cursor-pointer">
            <UserPlus size={12}/> {t('contacts.addressBook')}
          </button>
        </div>
      }
    >
      <div className="space-y-5">
        {currentContact ? (
          <div className="flex items-center justify-between p-4 bg-theme-primary/5 border-2 border-theme-primary rounded-3xl animate-in zoom-in duration-300 shadow-sm">
            <ContactBadge did={currentContact.did} contacts={contacts} size="lg" />
            <button
              type="button"
              onClick={onClearRecipient}
              title={t('common.clear')}
              aria-label={t('common.clear')}
              className="p-2 hover:bg-rose-50 text-theme-light hover:text-rose-500 rounded-xl transition-all"
            >
              <X size={20}/>
            </button>
          </div>
        ) : (
          <div className="space-y-4">
            {recipientId ? (
              <div className="flex items-center justify-between gap-3 p-3.5 bg-theme-primary/5 border border-theme-primary/30 rounded-2xl">
                <TechId value={recipientId} kind="did" />
                <button
                  type="button"
                  onClick={onClearRecipient}
                  title={t('common.clear')}
                  aria-label={t('common.clear')}
                  className="p-1.5 hover:bg-rose-50 text-theme-light hover:text-rose-500 rounded-xl transition-all shrink-0"
                >
                  <X size={18}/>
                </button>
              </div>
            ) : (
              <div className="space-y-3">
                <div className="relative">
                  <Input
                    data-testid="recipient-search-input"
                    value={searchQuery}
                    onChange={handleSearchOrCodeChange}
                    placeholder={t('transfer.recipientSearchPlaceholder')}
                    className={`py-3.5 px-5 pr-10 rounded-2xl text-sm transition-all ${recipientError ? 'border-rose-500 shadow-rose-100 ring-2 ring-rose-100' : ''}`}
                  />
                  <div className="absolute right-4 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none">
                    <Search size={16}/>
                  </div>
                </div>

                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    data-testid="recipient-paste-code-btn"
                    onClick={handlePasteCode}
                    className="gap-1.5 rounded-xl"
                  >
                    <ClipboardPaste size={13} />
                    {t('transfer.pasteCodeButton')}
                  </Button>
                  <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    data-testid="recipient-scan-qr-btn"
                    onClick={() => setScannerOpen(true)}
                    className="gap-1.5 rounded-xl"
                  >
                    <QrCode size={13} />
                    {t('transfer.scanQrButton')}
                  </Button>
                </div>

                {quickContacts.length > 0 && (
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 pt-1">
                    {quickContacts.map(contact => (
                      <button
                        key={contact.did}
                        type="button"
                        data-testid={`quick-contact-${contact.did}`}
                        onClick={() => onSelectContact(contact)}
                        className="flex items-center gap-3 p-2.5 bg-slate-50/70 hover:bg-theme-primary/5 border border-theme-subtle/60 hover:border-theme-primary/40 rounded-2xl text-left transition-all cursor-pointer"
                      >
                        <Avatar size={30} name={contact.did} variant="beam" />
                        <div className="min-w-0 flex-1">
                          <p className="text-xs font-bold text-theme-secondary truncate">
                            {getContactDisplayName(contact, t('transfer.anonymous'))}
                          </p>
                          <Advanced>
                            <p className="text-[10px] font-mono text-theme-light truncate">{contact.did}</p>
                          </Advanced>
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}

            <Advanced>
              <div className="space-y-2 pt-2 border-t border-theme-subtle/40">
                <label htmlFor="recipientId" className="text-[10px] font-black text-theme-light uppercase tracking-widest block">
                  {t('transfer.userId')}
                </label>
                <div className="relative group">
                  <Input
                    id="recipientId"
                    value={recipientId}
                    onChange={onRecipientChange}
                    onFocus={() => suggestions.length > 0 && setShowSuggestions(true)}
                    onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
                    placeholder="did:key:z..."
                    className={`py-4 px-5 rounded-2xl font-mono text-xs transition-all ${recipientError ? 'border-rose-500 shadow-rose-100 ring-2 ring-rose-100' : ''}`}
                  />
                  {showSuggestions && (
                    <div className="absolute z-40 w-full mt-2 bg-white/95 backdrop-blur-xl border border-theme-subtle rounded-3xl shadow-premium-lg overflow-hidden animate-in slide-in-from-top-2 duration-200">
                      {suggestions.map(contact => (
                        <button
                          key={contact.did}
                          type="button"
                          onClick={() => onSelectContact(contact)}
                          className="w-full flex items-center gap-4 p-3.5 hover:bg-theme-primary/5 text-left border-b border-theme-subtle/40 last:border-0 transition-all"
                        >
                          <Avatar size={30} name={contact.did} variant="beam" />
                          <div className="min-w-0">
                            <p className="text-sm font-black text-theme-secondary truncate">
                              {getContactDisplayName(contact, t('transfer.anonymous'))}
                            </p>
                            <p className="text-[9px] font-mono text-theme-light truncate">{contact.did}</p>
                          </div>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </Advanced>
          </div>
        )}

        {typeof trustStatus === 'object' && trustStatus !== null && 'knownOffender' in trustStatus && (
          <div className="p-5 bg-rose-50 border border-rose-100 rounded-[32px] flex items-start gap-4 animate-in shake duration-500 shadow-sm">
            <AlertTriangle className="text-rose-500 shrink-0 mt-1" size={24} />
            <div>
              <h4 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">{t('transfer.reputationAlert')}</h4>
              <p className="text-xs text-rose-800 font-medium leading-relaxed">{t('transfer.reputationAlertDesc')}</p>
            </div>
          </div>
        )}
      </div>

      <ModalShell
        isOpen={scannerOpen}
        onClose={() => setScannerOpen(false)}
        maxWidth="md"
        title={t('transfer.scanQrButton')}
        description={t('transfer.scanQrHint')}
      >
        <div className="space-y-4 py-2">
          <div className="relative w-full aspect-square max-h-[320px] bg-slate-900 rounded-2xl overflow-hidden flex items-center justify-center border border-slate-700">
            {cameraError ? (
              <div className="p-6 text-center space-y-3 max-w-sm">
                <Camera size={36} className="text-slate-400 mx-auto" />
                <p className="text-xs text-slate-300 leading-relaxed">{cameraError}</p>
              </div>
            ) : (
              <>
                <video ref={videoRef} className="w-full h-full object-cover" playsInline muted />
                <div className="absolute inset-0 border-2 border-white/30 rounded-2xl pointer-events-none flex items-center justify-center">
                  <div className="w-48 h-48 border-2 border-emerald-400 rounded-2xl shadow-[0_0_0_9999px_rgba(0,0,0,0.4)]" />
                </div>
              </>
            )}
          </div>
          <Button type="button" variant="outline" onClick={() => setScannerOpen(false)} className="w-full">
            {t('common.close')}
          </Button>
        </div>
      </ModalShell>
    </Card>
  );
}
