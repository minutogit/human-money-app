// src/components/Sidebar.tsx
import React from 'react';
import Avatar from "boring-avatars";
import logo from '../assets/logo.png';
import { AppState } from '../App';
import { useSession } from '../context/SessionContext';
import { 
  LayoutDashboard, 
  Wallet, 
  Send, 
  Download, 
  PlusCircle, 
  History, 
  ClipboardList, 
  Contact, 
  ShieldAlert, 
  Settings, 
  LogOut,
  ChevronRight
} from 'lucide-react';

interface SidebarProps {
  appState: AppState;
  setAppState: (state: AppState) => void;
  profileName: string;
  appVersion: string;
  onLogout: () => void;
  isOpen: boolean;
  setIsOpen: (open: boolean) => void;
}

export const INTERNAL_VIEWS = [
  'logged_in', 
  'wallet', 
  'activities', 
  'transaction_history', 
  'address_book', 
  'conflict_list', 
  'settings', 
  'create_voucher', 
  'send_vouchers', 
  'receive_bundle', 
  'voucher_details', 
  'conflict_details',
  'receive_success',
  'transfer_success',
  'sign_request'
];

export const Sidebar: React.FC<SidebarProps> = ({
  appState,
  setAppState,
  profileName,
  appVersion,
  onLogout,
  isOpen,
  setIsOpen
}) => {
  const { isSessionActive } = useSession();
  const currentView = appState.view;

  // Only show sidebar for internal views
  if (!INTERNAL_VIEWS.includes(currentView)) {
    return null;
  }

  const navGroups = [
    {
      label: 'Main',
      items: [
        { id: 'logged_in', label: 'Dashboard', icon: LayoutDashboard, view: { view: 'logged_in' } },
        { id: 'wallet', label: 'Wallet', icon: Wallet, view: { view: 'wallet' } },
      ]
    },
    {
      label: 'Actions',
      items: [
        { id: 'send_vouchers', label: 'Send', icon: Send, view: { view: 'send_vouchers' } },
        { id: 'receive_bundle', label: 'Receive / Process', icon: Download, view: { view: 'receive_bundle' } },
        { id: 'create_voucher', label: 'Create Voucher', icon: PlusCircle, view: { view: 'create_voucher', previousView: appState } },
      ]
    },
    {
      label: 'Records & Network',
      items: [
        { id: 'activities', label: 'Activity Log', icon: ClipboardList, view: { view: 'activities' } },
        { id: 'transaction_history', label: 'Bundle History', icon: History, view: { view: 'transaction_history' } },
        { id: 'address_book', label: 'Address Book', icon: Contact, view: { view: 'address_book' } },
      ]
    },
    {
      label: 'Security',
      items: [
        { id: 'conflict_list', label: 'Fraud Reports', icon: ShieldAlert, view: { view: 'conflict_list' } },
      ]
    }
  ];

  const isActive = (itemId: string) => {
    if (itemId === 'logged_in' && currentView === 'logged_in') return true;
    return currentView === itemId;
  };

  return (
    <>
      {/* Sidebar Overlay for Mobile */}
      {isOpen && (
        <div 
          className="fixed inset-0 z-30 bg-black/40 backdrop-blur-sm transition-opacity md:hidden"
          onClick={() => setIsOpen(false)}
        />
      )}

      <aside
        className={`fixed inset-y-0 left-0 z-40 w-72 transform bg-white dark:bg-gray-900 shadow-2xl transition-all duration-300 ease-in-out will-change-transform md:relative md:translate-x-0 ${
          isOpen ? "translate-x-0" : "-translate-x-full"
        } flex flex-col border-r border-theme-subtle`}
      >
        {/* Brand Header */}
        <div className="px-6 py-4 flex items-center gap-3 border-b border-theme-subtle/50">
          <div className="w-8 h-8 rounded-lg bg-white shadow-sm flex items-center justify-center overflow-hidden border border-theme-subtle">
            <img src={logo} alt="Logo" className="w-7 h-7 object-contain" />
          </div>
          <h1 className="text-sm font-bold text-theme-secondary tracking-tight">Human Money App</h1>
        </div>

        {/* Profile Header */}
        <div className="p-3 border-b border-theme-subtle bg-bg-app/30">
          <div className="flex items-center gap-3">
            <div className="ring-2 ring-theme-primary/20 rounded-full p-0.5 shadow-sm">
              <Avatar
                size={32}
                name={profileName || "Guest"}
                variant="beam"
                colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
              />
            </div>
            <div className="flex flex-col min-w-0">
              <h2 className="text-sm font-bold text-theme-secondary truncate flex items-center gap-1.5">
                {profileName || (isSessionActive ? "Logged In" : "Session Locked")}
                {!isSessionActive && profileName && (
                  <svg className="w-3.5 h-3.5 text-theme-warning" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                )}
              </h2>
              <span className="text-[10px] font-medium text-theme-light uppercase tracking-widest flex items-center gap-1">
                <span className={`w-1.5 h-1.5 rounded-full ${isSessionActive ? "bg-theme-success animate-pulse" : "bg-theme-warning"}`}></span>
                {isSessionActive ? "Active Wallet" : "Session Locked"}
              </span>
            </div>
          </div>
        </div>

        {/* Navigation Content */}
        <div className="flex-1 overflow-y-auto py-6 px-4 space-y-8 scrollbar-hide">
          {navGroups.map((group) => (
            <div key={group.label} className="space-y-2">
              <h3 className="px-4 text-[11px] font-bold text-theme-placeholder uppercase tracking-[0.15em]">
                {group.label}
              </h3>
              <div className="space-y-1">
                {group.items.map((item) => {
                  const active = isActive(item.id);
                  return (
                    <button
                      key={item.id}
                      onClick={() => {
                        setAppState(item.view as AppState);
                        setIsOpen(false);
                      }}
                      className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 ${
                        active 
                          ? "bg-theme-primary/10 text-theme-primary font-semibold shadow-sm" 
                          : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <item.icon 
                          size={18} 
                          className={`transition-transform duration-200 group-hover:scale-110 ${
                            active ? "text-theme-primary" : "text-theme-placeholder"
                          }`} 
                        />
                        <span className="text-sm">{item.label}</span>
                      </div>
                      {active && <ChevronRight size={14} className="text-theme-primary/50" />}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>

        {/* Footer Section */}
        <div className="p-4 mt-auto border-t border-theme-subtle bg-bg-app/10 space-y-2">
          <button 
            onClick={() => {
              setAppState({ view: 'settings' });
              setIsOpen(false);
            }}
            className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-xl transition-all duration-200 ${
              currentView === 'settings' 
                ? "bg-theme-primary/10 text-theme-primary font-semibold" 
                : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
            }`}
          >
            <Settings size={18} className={currentView === 'settings' ? "text-theme-primary" : "text-theme-placeholder"} />
            <span className="text-sm">Settings</span>
          </button>
          
          <button 
            onClick={onLogout}
            className="w-full flex items-center gap-3 px-4 py-2.5 rounded-xl text-theme-light hover:bg-red-50 hover:text-theme-error transition-all duration-200"
          >
            <LogOut size={18} className="text-theme-placeholder group-hover:text-theme-error" />
            <span className="text-sm">Logout</span>
          </button>

          <div className="pt-4 text-center">
            <span className="text-[10px] font-medium text-theme-placeholder bg-theme-subtle/30 px-2 py-1 rounded-full">
              Version {appVersion}
            </span>
          </div>
        </div>
      </aside>
    </>
  );
};
