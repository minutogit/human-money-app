import React, { useState } from 'react';
import { User, UserPlus, Search, X, AlertTriangle } from 'lucide-react';
import Avatar from 'boring-avatars';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { ContactBadge } from '../ui/ContactBadge';
import { Contact, TrustStatus } from '../../types';

interface RecipientSelectorProps {
  recipientId: string;
  onRecipientChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onSelectContact: (contact: Contact) => void;
  contacts: Contact[];
  trustStatus: TrustStatus;
  recipientError: boolean;
  onShowContactPicker: () => void;
  onClearRecipient: () => void;
}

export function RecipientSelector({
  recipientId,
  onRecipientChange,
  onSelectContact,
  contacts,
  trustStatus,
  recipientError,
  onShowContactPicker,
  onClearRecipient
}: RecipientSelectorProps) {
  const [showSuggestions, setShowSuggestions] = useState(false);
  
  const suggestions = React.useMemo(() => {
    if (!recipientId || recipientId.length < 2) return [];
    return contacts.filter(c =>
      c.did.toLowerCase().includes(recipientId.toLowerCase()) ||
      (c.profile.firstName || '').toLowerCase().includes(recipientId.toLowerCase()) ||
      (c.profile.lastName || '').toLowerCase().includes(recipientId.toLowerCase()) ||
      (c.profile.organization || '').toLowerCase().includes(recipientId.toLowerCase())
    ).slice(0, 5);
  }, [recipientId, contacts]);

  const currentContact = contacts.find(c => c.did === recipientId);

  return (
    <Card header={
      <div className="flex items-center gap-2">
        <User size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Recipient</span>
      </div>
    }>
      <div className="space-y-6">
        <div className="space-y-3">
          <label htmlFor="recipientId" className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-between">
            User ID (DID)
            <button type="button" onClick={onShowContactPicker} className="text-theme-primary hover:underline flex items-center gap-1.5 transition-colors">
              <UserPlus size={12}/> Address Book
            </button>
          </label>
          
          <div className="relative group">
            {currentContact ? (
              <div className="flex items-center justify-between p-4 bg-theme-primary/5 border-2 border-theme-primary rounded-3xl animate-in zoom-in duration-300 shadow-sm">
                <ContactBadge did={currentContact.did} contacts={contacts} size="lg" />
                <button 
                  type="button" 
                  onClick={onClearRecipient} 
                  className="p-2 hover:bg-rose-50 text-theme-light hover:text-rose-500 rounded-xl transition-all"
                >
                  <X size={20}/>
                </button>
              </div>
            ) : (
              <div className="relative">
                <Input
                  id="recipientId"
                  value={recipientId}
                  onChange={onRecipientChange}
                  onFocus={() => suggestions.length > 0 && setShowSuggestions(true)}
                  onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
                  placeholder="did:key:z..."
                  className={`py-5 px-6 rounded-3xl font-mono text-xs transition-all ${recipientError ? 'border-rose-500 shadow-rose-100 ring-2 ring-rose-100' : ''}`}
                />
                <div className="absolute right-4 top-1/2 -translate-y-1/2 text-theme-light">
                  <Search size={18}/>
                </div>
                
                {showSuggestions && (
                  <div className="absolute z-30 w-full mt-2 bg-white/90 backdrop-blur-xl border border-theme-subtle rounded-3xl shadow-premium-lg overflow-hidden animate-in slide-in-from-top-2 duration-200">
                    {suggestions.map(contact => (
                      <button 
                        key={contact.did} 
                        type="button" 
                        onClick={() => onSelectContact(contact)} 
                        className="w-full flex items-center gap-4 p-4 hover:bg-theme-primary/5 text-left border-b border-theme-subtle/40 last:border-0 transition-all"
                      >
                        <Avatar size={32} name={contact.did} variant="beam" />
                        <div className="min-w-0">
                          <p className="text-sm font-black text-theme-secondary truncate">
                            {(contact.profile.firstName || contact.profile.lastName) 
                              ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim() 
                              : (contact.profile.organization || 'Anonymous')}
                          </p>
                          <p className="text-[9px] font-mono text-theme-light truncate">{contact.did}</p>
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
          
          {typeof trustStatus === 'object' && 'knownOffender' in (trustStatus as any) && (
            <div className="p-5 bg-rose-50 border border-rose-100 rounded-[32px] flex items-start gap-4 animate-in shake duration-500 shadow-sm">
              <AlertTriangle className="text-rose-500 shrink-0 mt-1" size={24} />
              <div>
                <h4 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">Reputation Alert</h4>
                <p className="text-xs text-rose-800 font-medium leading-relaxed">This identity is associated with past ledger conflicts. Exercise extreme caution.</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </Card>
  );
}
