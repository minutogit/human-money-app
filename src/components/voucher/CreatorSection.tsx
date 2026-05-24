import { User, UserPlus, MoreVertical, Shield, MapPin, Building2, ExternalLink, ShieldAlert, CheckCircle2, Heart, Briefcase } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { InfoRow } from '../ui/InfoRow';
import { useTranslation } from 'react-i18next';
import { PublicProfile, TrustStatus } from '../../types';

interface CreatorSectionProps {
  creator: PublicProfile;
  trustStatus: TrustStatus;
  isContact: boolean;
  onManageContact: () => void;
}

export function CreatorSection({
  creator,
  trustStatus,
  isContact,
  onManageContact
}: CreatorSectionProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-6">
      <Card className="border-none shadow-premium" header={
        <div className="flex items-center justify-between w-full">
          <div className="flex items-center gap-2">
            <User size={18} className="text-theme-primary" />
            <span className="font-black text-xs uppercase tracking-widest">{t('voucher.creatorHeader')}</span>
          </div>
          <Button 
            variant="outline" 
            size="xs" 
            className="h-8 rounded-lg"
            onClick={onManageContact}
          >
            {isContact ? <MoreVertical size={14} /> : <UserPlus size={14} />}
          </Button>
        </div>
      }>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-8 p-2">
          <div className="space-y-6">
            <InfoRow label={t('voucher.legalNameLabel')} icon={User}>{creator.firstName} {creator.lastName}</InfoRow>
            <InfoRow label={t('voucher.accountIdLabel')} icon={Shield} isMono>{creator.id}</InfoRow>
            <InfoRow label={t('voucher.postalAddressLabel')} icon={MapPin}>
              {creator.address?.street} {creator.address?.houseNumber}<br/>
              {creator.address?.zipCode} {creator.address?.city}
              {creator.address?.country && <><br/>{creator.address.country}</>}
            </InfoRow>
            <InfoRow label={t('voucher.coordinatesLabel')} icon={MapPin}>{creator.coordinates}</InfoRow>
          </div>
          <div className="space-y-6">
            <InfoRow label={t('voucher.orgCommunityLabel')} icon={Building2}>{creator.organization || creator.community || t('voucher.independent')}</InfoRow>
            <InfoRow label={t('voucher.contactChannelsLabel')} icon={ExternalLink}>
              {creator.email && <div className="text-theme-accent">{creator.email}</div>}
              {creator.phone && <div>{creator.phone}</div>}
              {creator.url && <a href={creator.url} target="_blank" rel="noopener noreferrer" className="text-theme-primary hover:underline block">{creator.url}</a>}
            </InfoRow>
            <InfoRow label={t('voucher.genderOrientationLabel')}>
              {creator.gender === "1" ? t('gender.male') : creator.gender === "2" ? t('gender.female') : creator.gender === "0" ? t('gender.notKnown') : creator.gender === "9" ? t('gender.notApplicable') : t('common.unknown')}
            </InfoRow>
            <InfoRow label={t('voucher.reputationStatusLabel')}>
              {typeof trustStatus === 'object' && trustStatus !== null && 'knownOffender' in trustStatus ? (
                <span className="text-rose-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                  <ShieldAlert size={14} /> {t('voucher.highRiskAccount')}
                </span>
              ) : (
                <span className="text-emerald-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                  <CheckCircle2 size={14} /> {t('voucher.cleanReputation')}
                </span>
              )}
            </InfoRow>
          </div>
        </div>
      </Card>

      {(creator.serviceOffer || creator.needs) && (
        <Card className="border-none shadow-premium" header={
          <div className="flex items-center gap-2">
            <Heart size={18} className="text-theme-primary" />
            <span className="font-black text-xs uppercase tracking-widest">{t('voucher.communityOfferingsHeader')}</span>
          </div>
        }>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-8 p-2">
            <InfoRow label={t('voucher.helpWithLabel')} icon={Briefcase}>{creator.serviceOffer}</InfoRow>
            <InfoRow label={t('voucher.lookingForLabel')} icon={Heart}>{creator.needs}</InfoRow>
          </div>
        </Card>
      )}
    </div>
  );
}
