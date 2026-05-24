// src/components/Sidebar.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import Avatar from "boring-avatars";
import logo from '../assets/logo.png';
import { AppState } from '../types';
import { useSession } from '../context/SessionContext';
import { useNavigation } from '../context/NavigationContext';
import { profileService } from '../services/profileService';
import { error } from "@tauri-apps/plugin-log";
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

export const Sidebar: React.FC = () => {
  const { t } = useTranslation();
  const { isSessionActive, profileName, notifyLogout } = useSession();
  const { appState, navigate, appVersion, isSidebarOpen, setSidebarOpen } = useNavigation();
  const currentView = appState.view;

  // Only show sidebar for internal views
  if (!INTERNAL_VIEWS.includes(currentView)) {
    return null;
  }

  const handleLogout = () => {
    profileService.logout().catch(e => error(`Logout failed: ${e}`));
    setSidebarOpen(false);
    notifyLogout();
    navigate({ view: "needs_login" });
  };

  const navGroups = [
    {
      label: t('common.sidebarMain'),
      items: [
        { id: 'logged_in', label: t('dashboard.title'), icon: LayoutDashboard, view: { view: 'logged_in' } },
        { id: 'wallet', label: t('wallet.title'), icon: Wallet, view: { view: 'wallet' } },
      ]
    },
    {
      label: t('common.sidebarActions'),
      items: [
        { id: 'send_vouchers', label: t('transfer.send'), icon: Send, view: { view: 'send_vouchers' } },
        { id: 'receive_bundle', label: t('transfer.receiveProcess'), icon: Download, view: { view: 'receive_bundle' } },
        { id: 'create_voucher', label: t('voucher.create'), icon: PlusCircle, view: { view: 'create_voucher', previousView: appState } },
      ]
    },
    {
      label: t('common.sidebarRecords'),
      items: [
        { id: 'activities', label: t('history.activityLog'), icon: ClipboardList, view: { view: 'activities' } },
        { id: 'transaction_history', label: t('history.transactionHistory'), icon: History, view: { view: 'transaction_history' } },
        { id: 'address_book', label: t('contacts.addressBook'), icon: Contact, view: { view: 'address_book' } },
      ]
    },
    {
      label: t('common.sidebarSecurity'),
      items: [
        { id: 'conflict_list', label: t('conflict.fraudReports'), icon: ShieldAlert, view: { view: 'conflict_list' } },
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
      {isSidebarOpen && (
        <div 
          className="fixed inset-0 z-30 bg-black/40 backdrop-blur-sm transition-opacity md:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      <aside
        className={`fixed inset-y-0 left-0 z-40 w-72 transform bg-white dark:bg-gray-900 shadow-2xl transition-all duration-300 ease-in-out will-change-transform md:relative md:translate-x-0 ${
          isSidebarOpen ? "translate-x-0" : "-translate-x-full"
        } flex flex-col border-r border-theme-subtle`}
      >
        {/* Brand Header */}
        <div className="px-6 py-4 flex items-center gap-3 border-b border-theme-subtle/50">
          <div className="w-8 h-8 rounded-lg bg-white shadow-sm flex items-center justify-center overflow-hidden border border-theme-subtle">
            <img src={logo} alt="Logo" className="w-7 h-7 object-contain" />
          </div>
          <h1 className="text-sm font-bold text-theme-secondary tracking-tight">{t('common.appName')}</h1>
        </div>

        {/* Profile Header */}
        <div className="p-3 border-b border-theme-subtle bg-bg-app/30">
          <div className="flex items-center gap-3">
            <div className="ring-2 ring-theme-primary/20 rounded-full p-0.5 shadow-sm">
              <Avatar
                size={32}
                name={profileName || t('common.guest')}
                variant="beam"
                colors={["#E63946", "#F4A261", "#E76F51", "#2A9D8F", "#2B1B17"]}
              />
            </div>
            <div className="flex flex-col min-w-0">
              <h2 className="text-sm font-bold text-theme-secondary truncate flex items-center gap-1.5">
                {profileName || (isSessionActive ? t('auth.loggedIn') : t('auth.sessionLocked'))}
                {!isSessionActive && profileName && (
                  <svg className="w-3.5 h-3.5 text-theme-warning" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                )}
              </h2>
              <span className="text-[10px] font-medium text-theme-light uppercase tracking-widest flex items-center gap-1">
                <span className={`w-1.5 h-1.5 rounded-full ${isSessionActive ? "bg-theme-success animate-pulse" : "bg-theme-warning"}`}></span>
                {isSessionActive ? t('common.activeWallet') : t('auth.sessionLocked')}
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
                        navigate(item.view as AppState);
                        setSidebarOpen(false);
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
              navigate({ view: 'settings' });
              setSidebarOpen(false);
            }}
            className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-xl transition-all duration-200 ${
              currentView === 'settings' 
                ? "bg-theme-primary/10 text-theme-primary font-semibold" 
                : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
            }`}
          >
            <Settings size={18} className={currentView === 'settings' ? "text-theme-primary" : "text-theme-placeholder"} />
            <span className="text-sm">{t('settings.title')}</span>
          </button>
          
          <button 
            onClick={handleLogout}
            className="w-full flex items-center gap-3 px-4 py-2.5 rounded-xl text-theme-light hover:bg-red-50 hover:text-theme-error transition-all duration-200"
          >
            <LogOut size={18} className="text-theme-placeholder group-hover:text-theme-error" />
            <span className="text-sm">{t('auth.logout')}</span>
          </button>

          <div className="pt-4 text-center">
            <span className="text-[10px] font-medium text-theme-placeholder bg-theme-subtle/30 px-2 py-1 rounded-full">
              {t('common.version', { version: appVersion })}
            </span>
          </div>
        </div>
      </aside>
    </>
  );
};
