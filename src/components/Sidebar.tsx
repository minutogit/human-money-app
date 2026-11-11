// src/components/Sidebar.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import Avatar from "boring-avatars";
import logo from '../assets/logo.png';
import { AppState } from '../types';
import { useSession } from '../context/SessionContext';
import { useNavigation } from '../context/NavigationContext';
import { profileService } from '../services/profileService';
import { integrityService } from '../services/integrityService';
import { error } from "@tauri-apps/plugin-log";
import { stringifyError } from '../utils/errorHelper';
import { 
  LayoutDashboard, 
  Wallet, 
  History, 
  Contact, 
  ShieldAlert, 
  Settings, 
  LogOut,
  ChevronRight,
  ChevronDown,
  Heart,
  Bug,
  BookOpen,
  Lock,
  Store
} from 'lucide-react';

export const INTERNAL_VIEWS = [
  'logged_in', 
  'wallet', 
  'marketplace',
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
  'sign_request',
  'support',
  'bug_report',
  'help_center'
];

export interface SidebarProps {
  openClaimsCount?: number;
}

export const Sidebar: React.FC<SidebarProps> = ({ openClaimsCount }) => {
  const { t } = useTranslation();
  const { isSessionActive, isAuthenticated, profileName, notifyLogout, protectAction, lockSession } = useSession();
  const { appState, navigate, appVersion, isSidebarOpen, setSidebarOpen } = useNavigation();
  const currentView = appState.view;
  const isLoggedIn = isAuthenticated ?? isSessionActive;

  // Local state for fetched conflicts count if not passed via props
  const [fetchedClaimsCount, setFetchedClaimsCount] = useState<number>(0);
  const effectiveOpenClaims = openClaimsCount !== undefined ? openClaimsCount : fetchedClaimsCount;

  // Collapsible 'Mehr' state - collapsed by default, stored in localStorage
  const [isMoreOpen, setIsMoreOpen] = useState<boolean>(() => {
    try {
      return localStorage.getItem('humoco_sidebar_more_open') === 'true';
    } catch {
      return false;
    }
  });

  const toggleMore = () => {
    setIsMoreOpen((prev) => {
      const next = !prev;
      try {
        localStorage.setItem('humoco_sidebar_more_open', String(next));
      } catch {
        // ignore best-effort storage
      }
      return next;
    });
  };

  useEffect(() => {
    if (openClaimsCount !== undefined || !isLoggedIn) return;
    let isMounted = true;
    integrityService
      .getDoubleSpendConflicts()
      .then((conflicts) => {
        if (isMounted) setFetchedClaimsCount((conflicts || []).length);
      })
      .catch(() => {});
    return () => {
      isMounted = false;
    };
  }, [openClaimsCount, isLoggedIn]);

  // Only show sidebar for internal views when a session is active
  if (!INTERNAL_VIEWS.includes(currentView) || !isLoggedIn) {
    return null;
  }

  const handleLogout = () => {
    profileService.logout().catch(e => error(`Logout failed: ${stringifyError(e)}`));
    setSidebarOpen(false);
    notifyLogout();
    navigate({ view: "needs_login" });
  };

  const navGroups = [
    {
      label: t('sidebar.groupOverview', 'Übersicht'),
      items: [
        { id: 'logged_in', label: t('dashboard.title'), icon: LayoutDashboard, view: { view: 'logged_in' } },
        { id: 'wallet', label: t('wallet.title'), icon: Wallet, view: { view: 'wallet' } },
        { id: 'transaction_history', label: t('history.transactionHistory'), icon: History, view: { view: 'transaction_history' } },
      ]
    },
    {
      label: t('sidebar.groupPeople', 'Menschen'),
      items: [
        { id: 'address_book', label: t('contacts.addressBook'), icon: Contact, view: { view: 'address_book' } },
        { id: 'marketplace', label: t('marketplace.title'), icon: Store, view: { view: 'marketplace' } },
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
        className={`fixed inset-y-0 left-0 z-40 w-72 transform bg-white dark:bg-gray-900 shadow-2xl transition-all duration-300 ease-in-out will-change-transform md:relative ${
          isSidebarOpen ? "translate-x-0" : "max-md:-translate-x-full"
        } md:translate-x-0 flex flex-col border-r border-theme-subtle`}
      >
        {/* Brand Header */}
        <div className="px-6 py-4 flex items-center gap-3 border-b border-theme-subtle/50 shrink-0">
          <div className="w-8 h-8 rounded-lg bg-white shadow-sm flex items-center justify-center overflow-hidden border border-theme-subtle">
            <img src={logo} alt="Logo" className="w-7 h-7 object-contain" />
          </div>
          <h1 className="text-sm font-bold text-theme-secondary tracking-tight">{t('common.appName')}</h1>
        </div>

        {/* Profile Header */}
        <div className="p-3 border-b border-theme-subtle bg-bg-app/30 flex items-center justify-between gap-2 shrink-0">
          <div 
            onClick={!isSessionActive ? () => { protectAction(async () => {}).catch(() => {}); } : undefined}
            onKeyDown={!isSessionActive ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                protectAction(async () => {}).catch(() => {});
              }
            } : undefined}
            role={!isSessionActive ? "button" : undefined}
            tabIndex={!isSessionActive ? 0 : undefined}
            title={!isSessionActive ? t('auth.sessionLocked') : undefined}
            className={`flex items-center gap-3 min-w-0 flex-1 transition-colors ${
              !isSessionActive ? "cursor-pointer hover:bg-theme-warning/10 rounded-xl p-1 -m-1" : ""
            }`}
          >
            <div className="ring-2 ring-theme-primary/20 rounded-full p-0.5 shadow-sm shrink-0">
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
                  <svg className="w-3.5 h-3.5 text-theme-warning shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                )}
              </h2>
              <span className="text-[10px] font-medium text-theme-light uppercase tracking-widest flex items-center gap-1">
                <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${isSessionActive ? "bg-theme-success animate-pulse" : "bg-theme-warning"}`}></span>
                {isSessionActive ? t('common.activeWallet') : t('auth.sessionLocked')}
              </span>
            </div>
          </div>
          {isSessionActive && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                lockSession();
              }}
              title={t('auth.lockSession')}
              className="p-2 text-theme-light hover:text-theme-primary hover:bg-theme-primary/10 rounded-xl transition-colors cursor-pointer shrink-0"
            >
              <Lock size={16} />
            </button>
          )}
        </div>

        {/* Navigation Content */}
        <div className="flex-1 min-h-0 overflow-y-auto px-4 pt-4 pb-8 space-y-6 scrollbar-hide">
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
                      className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
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

          {/* Conditional Konflikte entry above 'Mehr' when open claims > 0 */}
          {effectiveOpenClaims > 0 && (
            <div className="space-y-1 pt-2">
              <button
                type="button"
                onClick={() => {
                  navigate({ view: 'conflict_list' });
                  setSidebarOpen(false);
                }}
                className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
                  isActive('conflict_list')
                    ? "bg-rose-50 text-rose-700 dark:bg-rose-950/40 dark:text-rose-300 font-semibold shadow-sm"
                    : "text-rose-600 hover:bg-rose-50/60 dark:hover:bg-rose-950/30"
                }`}
                data-testid="sidebar-conflict-top-entry"
              >
                <div className="flex items-center gap-3">
                  <ShieldAlert
                    size={18}
                    className="text-rose-500 transition-transform duration-200 group-hover:scale-110"
                  />
                  <span className="text-sm font-semibold">{t('conflict.fraudReports')}</span>
                </div>
                <span className="px-2 py-0.5 text-[10px] font-bold bg-rose-500 text-white rounded-full">
                  {effectiveOpenClaims}
                </span>
              </button>
            </div>
          )}

          {/* Collapsible 'Mehr' section */}
          <div className="space-y-2 pt-2 border-t border-theme-subtle/40">
            <button
              type="button"
              onClick={toggleMore}
              className="w-full flex items-center justify-between px-4 py-2 text-[11px] font-bold text-theme-placeholder uppercase tracking-[0.15em] hover:text-theme-primary transition-colors cursor-pointer"
              aria-expanded={isMoreOpen}
              data-testid="sidebar-more-toggle"
            >
              <div className="flex items-center gap-2">
                <span>{t('sidebar.groupMore', 'Mehr')}</span>
                {!isMoreOpen && effectiveOpenClaims > 0 && (
                  <span
                    className="w-2 h-2 rounded-full bg-rose-500 shrink-0 animate-pulse"
                    data-testid="sidebar-more-indicator-dot"
                    title={t('conflict.fraudReports')}
                  />
                )}
              </div>
              <ChevronDown
                size={14}
                className={`transition-transform duration-200 ${isMoreOpen ? 'rotate-180' : ''}`}
              />
            </button>

            {isMoreOpen && (
              <div className="space-y-1 animate-in fade-in duration-150" data-testid="sidebar-more-content">
                {/* Konflikte */}
                <button
                  type="button"
                  onClick={() => {
                    navigate({ view: 'conflict_list' });
                    setSidebarOpen(false);
                  }}
                  className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
                    isActive('conflict_list')
                      ? "bg-theme-primary/10 text-theme-primary font-semibold shadow-sm"
                      : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
                  }`}
                  data-testid="sidebar-more-item-conflicts"
                >
                  <div className="flex items-center gap-3">
                    <ShieldAlert
                      size={18}
                      className={`transition-transform duration-200 group-hover:scale-110 ${
                        isActive('conflict_list') ? "text-theme-primary" : "text-theme-placeholder"
                      }`}
                    />
                    <span className="text-sm">{t('conflict.fraudReports')}</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    {effectiveOpenClaims > 0 && (
                      <span className="px-1.5 py-0.5 text-[10px] font-bold bg-rose-500 text-white rounded-full">
                        {effectiveOpenClaims}
                      </span>
                    )}
                    {isActive('conflict_list') && <ChevronRight size={14} className="text-theme-primary/50" />}
                  </div>
                </button>

                {/* Hilfe & Konzept */}
                <button
                  type="button"
                  onClick={() => {
                    navigate({ view: 'help_center', previousView: appState });
                    setSidebarOpen(false);
                  }}
                  className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
                    currentView === 'help_center'
                      ? "bg-theme-primary/10 text-theme-primary font-semibold shadow-sm"
                      : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
                  }`}
                  data-testid="sidebar-more-item-help"
                >
                  <div className="flex items-center gap-3">
                    <BookOpen
                      size={18}
                      className={`transition-transform duration-200 group-hover:scale-110 ${
                        currentView === 'help_center' ? "text-theme-primary" : "text-theme-placeholder"
                      }`}
                    />
                    <span className="text-sm">{t('helpCenter.menuTitle')}</span>
                  </div>
                  {currentView === 'help_center' && <ChevronRight size={14} className="text-theme-primary/50" />}
                </button>

                {/* Unterstützen */}
                <button
                  type="button"
                  onClick={() => {
                    navigate({ view: 'support' });
                    setSidebarOpen(false);
                  }}
                  className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
                    currentView === 'support'
                      ? "bg-theme-primary/10 text-theme-primary font-semibold shadow-sm"
                      : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
                  }`}
                  data-testid="sidebar-more-item-support"
                >
                  <div className="flex items-center gap-3">
                    <Heart
                      size={18}
                      className={`transition-transform duration-200 group-hover:scale-110 ${
                        currentView === 'support' ? "text-theme-primary" : "text-theme-placeholder"
                      }`}
                    />
                    <span className="text-sm">{t('common.supportMenu')}</span>
                  </div>
                  {currentView === 'support' && <ChevronRight size={14} className="text-theme-primary/50" />}
                </button>

                {/* Fehler melden */}
                <button
                  type="button"
                  onClick={() => {
                    navigate({ view: 'bug_report' });
                    setSidebarOpen(false);
                  }}
                  className={`w-full group flex items-center justify-between px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
                    currentView === 'bug_report'
                      ? "bg-theme-primary/10 text-theme-primary font-semibold shadow-sm"
                      : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
                  }`}
                  data-testid="sidebar-more-item-bug"
                >
                  <div className="flex items-center gap-3">
                    <Bug
                      size={18}
                      className={`transition-transform duration-200 group-hover:scale-110 ${
                        currentView === 'bug_report' ? "text-theme-primary" : "text-theme-placeholder"
                      }`}
                    />
                    <span className="text-sm">{t('common.bugReportMenu')}</span>
                  </div>
                  {currentView === 'bug_report' && <ChevronRight size={14} className="text-theme-primary/50" />}
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Footer Section */}
        <div className="p-4 mt-auto border-t border-theme-subtle bg-bg-app/10 space-y-2 shrink-0">
          <button 
            onClick={() => {
              navigate({ view: 'settings' });
              setSidebarOpen(false);
            }}
            className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-xl transition-all duration-200 cursor-pointer ${
              currentView === 'settings' 
                ? "bg-theme-primary/10 text-theme-primary font-semibold" 
                : "text-theme-light hover:bg-bg-app hover:text-theme-secondary"
            }`}
            data-testid="sidebar-settings-button"
          >
            <Settings size={18} className={currentView === 'settings' ? "text-theme-primary" : "text-theme-placeholder"} />
            <span className="text-sm">{t('settings.title')}</span>
          </button>
          
          <button 
            onClick={handleLogout}
            className="w-full flex items-center gap-3 px-4 py-2.5 rounded-xl text-theme-light hover:bg-red-50 hover:text-theme-error transition-all duration-200 cursor-pointer"
            data-testid="sidebar-logout-button"
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
