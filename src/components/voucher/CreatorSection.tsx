import { User, UserPlus, MoreVertical, Shield, MapPin, Building2, ExternalLink, ShieldAlert, CheckCircle2, Heart, Briefcase } from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { InfoRow } from '../ui/InfoRow';
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
  return (
    <div className="space-y-6">
      <Card className="border-none shadow-premium" header={
        <div className="flex items-center justify-between w-full">
          <div className="flex items-center gap-2">
            <User size={18} className="text-theme-primary" />
            <span className="font-black text-xs uppercase tracking-widest">Creator</span>
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
            <InfoRow label="Legal Name" icon={User}>{creator.firstName} {creator.lastName}</InfoRow>
            <InfoRow label="Account ID" icon={Shield} isMono>{creator.id}</InfoRow>
            <InfoRow label="Postal Address" icon={MapPin}>
              {creator.address?.street} {creator.address?.houseNumber}<br/>
              {creator.address?.zipCode} {creator.address?.city}
              {creator.address?.country && <><br/>{creator.address.country}</>}
            </InfoRow>
            <InfoRow label="Map Coordinates" icon={MapPin}>{creator.coordinates}</InfoRow>
          </div>
          <div className="space-y-6">
            <InfoRow label="Organization / Community" icon={Building2}>{creator.organization || creator.community || "Independent"}</InfoRow>
            <InfoRow label="Contact Channels" icon={ExternalLink}>
              {creator.email && <div className="text-theme-accent">{creator.email}</div>}
              {creator.phone && <div>{creator.phone}</div>}
              {creator.url && <a href={creator.url} target="_blank" rel="noopener noreferrer" className="text-theme-primary hover:underline block">{creator.url}</a>}
            </InfoRow>
            <InfoRow label="Gender Orientation">
              {creator.gender === "1" ? "Male" : creator.gender === "2" ? "Female" : creator.gender === "0" ? "Other / Not Declared" : creator.gender === "9" ? "Not Applicable" : "Unknown"}
            </InfoRow>
            <InfoRow label="Reputation Status">
              {typeof trustStatus === 'object' && trustStatus !== null && 'knownOffender' in trustStatus ? (
                <span className="text-rose-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                  <ShieldAlert size={14} /> High Risk Account
                </span>
              ) : (
                <span className="text-emerald-500 font-black uppercase text-[10px] tracking-widest flex items-center gap-2">
                  <CheckCircle2 size={14} /> Clean Reputation
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
            <span className="font-black text-xs uppercase tracking-widest">Community Offerings</span>
          </div>
        }>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-8 p-2">
            <InfoRow label="I can help with..." icon={Briefcase}>{creator.serviceOffer}</InfoRow>
            <InfoRow label="I am looking for..." icon={Heart}>{creator.needs}</InfoRow>
          </div>
        </Card>
      )}
    </div>
  );
}
