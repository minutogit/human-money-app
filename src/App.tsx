// src/App.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error } from "@tauri-apps/plugin-log";
import { CreateProfile } from "./components/CreateProfile";
import { Dashboard } from "./components/Dashboard";
import { PasswordReset } from "./components/PasswordReset";
import { Login } from "./components/Login";
import "./App.css";

type AppState = "loading" | "needs_profile" | "needs_login" | "logged_in" | "needs_reset";

function App() {
  const [appState, setAppState] = useState<AppState>("loading");

  useEffect(() => {
    async function checkProfile() {
      try {
        const exists = await invoke<boolean>("profile_exists");
        setAppState(exists ? "needs_login" : "needs_profile");
      } catch (e) {
        error(`Failed to check if profile exists: ${e}`);
        // Fallback to creation screen if backend check fails
        setAppState("needs_profile");
      }
    }
    checkProfile();
  }, []);

  function renderContent() {
    switch (appState) {
      case "loading":
        return <p className="text-white">Loading...</p>;
      case "needs_profile":
        return <CreateProfile onProfileCreated={() => setAppState("logged_in")} />;
      case "needs_login":
        return <Login onLoginSuccess={() => setAppState("logged_in")} onSwitchToCreate={() => setAppState("needs_profile")} onSwitchToReset={() => setAppState("needs_reset")} />;
      case "logged_in":
        return <Dashboard onLogout={() => setAppState("needs_login")} />;
      case "needs_reset":
        return <PasswordReset onResetSuccess={() => setAppState("logged_in")} onSwitchToLogin={() => setAppState("needs_login")} />;
      default:
        return <p className="text-red-500">Invalid application state.</p>;
    }
  }

  return (
    <main className="flex flex-col items-center justify-center min-h-screen bg-bg-app p-4 font-sans text-theme-secondary">
      {renderContent()}
    </main> 
  );
}

export default App;
