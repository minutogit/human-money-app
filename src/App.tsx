import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import "./App.css";

function App() {
  // State für das Profilerstellungs-Formular
  const [mnemonic, setMnemonic] = useState("");
  const [userPrefix, setUserPrefix] = useState("");
  const [password, setPassword] = useState("");
  // State für Feedback vom Backend (Erfolg oder Fehler)
  const [feedbackMsg, setFeedbackMsg] = useState("");

  async function createProfile() {
    setFeedbackMsg("Creating profile...");
    info(`Frontend: Attempting to create profile with prefix: "${userPrefix || 'none'}"`);
    try {
      await invoke("create_profile", {
        mnemonic,
        // Sende `null`, wenn das Feld leer ist, damit Rust es als `None` interpretiert
        userPrefix: userPrefix || null,
        password,
      });
      setFeedbackMsg("Profile successfully created and wallet is loaded!");
      info("Frontend: Profile creation successful.");
    } catch (e) {
      // `e` ist die Fehlermeldung aus dem Rust-Backend
      setFeedbackMsg(`Error: ${e}`);
      error(`Frontend: Profile creation failed: ${e}`);
    }
  }

  return (
    <main className="container">
      <h1>Voucher Wallet</h1>

      <h2>Create New Profile</h2>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          createProfile();
        }}
      >
        <textarea
          onChange={(e) => setMnemonic(e.currentTarget.value)}
          placeholder="Enter your 12-word mnemonic phrase"
          required
        />
        <input
          onChange={(e) => setUserPrefix(e.currentTarget.value)}
          placeholder="Optional user prefix (e.g., 'user')"
        />
        <input
          type="password"
          onChange={(e) => setPassword(e.currentTarget.value)}
          placeholder="Enter a strong password"
          required
        />
        <button type="submit">Create Profile</button>
      </form>
      <p>{feedbackMsg}</p>
    </main>
  );
}

export default App;
