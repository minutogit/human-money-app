import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { User, Search, X, AlertTriangle, ClipboardPaste, QrCode } from 'lucide-react';
import { UserAvatar } from '../ui/UserAvatar';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { QrScannerModal } from '../ui/QrScannerModal';
import { ContactBadge } from '../ui/ContactBadge';
import { readFromClipboard } from '../../utils/clipboard';
import { parseHumocoInput, normalizeUserIdInput } from '../../utils/humocoUri';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';
import { Contact, TrustStatus } from '../../types';

interface RecipientSelectorProps {
  recipientId: string;
  onRecipientChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onApplyRawCode?: (raw: string) => boolean | Promise<boolean>;
  onSelectContact: (contact: Contact) => void;
  contacts: Contact[];
  trustStatus: TrustStatus;
  recipientError: boolean;
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
  onClearRecipient
}: RecipientSelectorProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState('');
  const [scannerOpen, setScannerOpen] = useState(false);

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
      .slice(0, 6);
  }, [contacts, searchQuery]);

  const currentContact = contacts.find(c => c.did === recipientId);

  const handleInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const rawVal = e.target.value;
    const inputEl = e.target;
    const trimmed = rawVal.trim();
    const lower = trimmed.toLowerCase();

    const parsed = parseHumocoInput(rawVal);
    if (onApplyRawCode && parsed) {
      const ok = await onApplyRawCode(rawVal);
      if (ok) {
        const normalized = normalizeUserIdInput(rawVal);
        if (normalized) {
          inputEl.value = normalized;
        }
        setSearchQuery('');
        return;
      }
    }

    if (
      lower.includes('humoco://pay') ||
      lower.startsWith('did:') ||
      lower.includes('@did:') ||
      Boolean(recipientId)
    ) {
      onRecipientChange(e);
      setSearchQuery(rawVal.replace(/\s+/g, ''));
      return;
    }

    setSearchQuery(rawVal);
    if (!rawVal) {
      onClearRecipient();
    }
  };

  const handlePasteCode = async () => {
    if (!onApplyRawCode) return;
    try {
      const { text } = await readFromClipboard();
      if (text && text.trim()) {
        const ok = await onApplyRawCode(text.trim());
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

  const handleClear = () => {
    setSearchQuery('');
    onClearRecipient();
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
          <div className="flex items-center gap-1.5">
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="recipient-paste-code-btn"
              onClick={handlePasteCode}
              className="gap-1 rounded-xl"
            >
              <ClipboardPaste size={12} />
              <span>{t('transfer.pasteCodeButton')}</span>
            </Button>
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="recipient-scan-qr-btn"
              onClick={() => setScannerOpen(true)}
              className="gap-1 rounded-xl"
            >
              <QrCode size={12} />
              <span>{t('transfer.scanQrButton')}</span>
            </Button>
          </div>
        </div>
      }
    >
      <div className="space-y-4">
        {currentContact ? (
          <div className="flex items-center justify-between p-3.5 bg-theme-primary/5 border-2 border-theme-primary rounded-2xl animate-in zoom-in duration-200 shadow-sm">
            <ContactBadge did={currentContact.did} contacts={contacts} size="lg" />
            <button
              type="button"
              onClick={handleClear}
              title={t('common.clear')}
              aria-label={t('common.clear')}
              className="p-2 hover:bg-rose-50 text-theme-light hover:text-rose-500 rounded-xl transition-all cursor-pointer"
            >
              <X size={18}/>
            </button>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="relative">
              <Input
                data-testid="recipient-search-input"
                aria-label={t('transfer.userId')}
                value={recipientId || searchQuery}
                onChange={handleInputChange}
                placeholder={t('transfer.recipientSearchPlaceholder')}
                className={`py-3 px-4 pr-10 rounded-2xl text-sm transition-all ${recipientError ? 'border-rose-500 shadow-rose-100 ring-2 ring-rose-100' : ''}`}
              />
              {recipientId || searchQuery ? (
                <button
                  type="button"
                  onClick={handleClear}
                  title={t('common.clear')}
                  aria-label={t('common.clear')}
                  className="absolute right-3 top-1/2 -translate-y-1/2 p-1 text-theme-light hover:text-rose-500 rounded-lg transition-colors cursor-pointer"
                >
                  <X size={16} />
                </button>
              ) : (
                <div className="absolute right-3.5 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none">
                  <Search size={16}/>
                </div>
              )}
            </div>

            {!recipientId && quickContacts.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {quickContacts.map(contact => {
                  const name = getContactDisplayName(contact, t('transfer.anonymous'));
                  return (
                    <button
                      key={contact.did}
                      type="button"
                      data-testid={`quick-contact-${contact.did}`}
                      onClick={() => {
                        setSearchQuery('');
                        onSelectContact(contact);
                      }}
                      className="inline-flex items-center gap-2 px-3 py-1.5 bg-slate-50/80 hover:bg-theme-primary/5 border border-theme-subtle/70 hover:border-theme-primary/40 rounded-full text-left transition-all cursor-pointer max-w-full"
                    >
                      <UserAvatar
                        size={22}
                        pictureUrl={contact.profile.pictureUrl}
                        name={name}
                      />
                      <span className="text-xs font-bold text-theme-secondary truncate max-w-[160px]">
                        {name}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        )}

        {typeof trustStatus === 'object' && trustStatus !== null && 'knownOffender' in trustStatus && (
          <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-start gap-3 animate-in shake duration-500 shadow-sm">
            <AlertTriangle className="text-rose-500 shrink-0 mt-0.5" size={20} />
            <div>
              <h4 className="text-xs font-black text-rose-900 uppercase tracking-widest mb-1">{t('transfer.reputationAlert')}</h4>
              <p className="text-xs text-rose-800 font-medium leading-relaxed">{t('transfer.reputationAlertDesc')}</p>
            </div>
          </div>
        )}
      </div>

      <QrScannerModal
        isOpen={scannerOpen}
        onClose={() => setScannerOpen(false)}
        onScan={(raw) => {
          onApplyRawCode?.(raw);
        }}
        title={t('transfer.scanQrButton')}
        hint={t('transfer.scanQrHint')}
      />
    </Card>
  );
}
