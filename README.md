# Human Money App (Tauri Desktop Wallet)

This repository contains the source code for a cross-platform desktop wallet for the human money ecosystem. Built with **Tauri v2**, **React 19**, and **Tailwind CSS v4**, it serves as a secure, high-performance client for the `human_money_core` Rust library.

The application is designed as a thin bridge, ensuring that all core cryptographic and business logic resides in the shared core library while providing a premium desktop experience on Windows, macOS, and Linux.

## ✨ Key Features

This prototype implements the full lifecycle of a decentralized voucher wallet:

*   **🔒 Secure Profile Management:** Multi-profile support with encrypted local storage. High-security authentication including pessimistic locking and configurable session timeouts.
*   **👤 Encrypted User Profiles:** Store personal metadata (Names, Gender, Address, Service Offers, Needs) securely within the wallet file. Used to auto-populate voucher creator and signature details.
*   **🎫 Voucher Lifecycle:** Create new vouchers based on extensible standards (e.g., Minuto, Silver), including full support for guarantor and notary signature roles.
*   **💸 Secure Transactions:** Send and receive value through signed data bundles. Supports **flexible encryption modes** (DID-asymmetric, Password-symmetric, or Cleartext) to maximize interoperability.
*   **📂 Multi-Signature Workflows:** Full GUI support for requesting, creating, and attaching additional signatures to incomplete vouchers.
*   **📊 Transaction History:** Comprehensive history of sent and received transfers with detailed audit logs and balance aggregation by currency.
*   **🛠️ Developer Productivity:** Context-aware development with Antigravity AI, integrated status tracking (`STATUS.md`), and automated release workflows.

## 🛠️ Tech Stack & Architecture

*   **Framework:** [Tauri v2](https://tauri.app/) for a tiny footprint and maximum security.
*   **Frontend:** React 19 + TypeScript + Tailwind CSS v4 (using the `@tailwindcss/vite` plugin).
*   **Backend:** Rust bridge to `human_money_core::AppService` facade.
*   **Security:** Pessimistic directory locking to prevent stale state in multi-process environments.

## 🚀 Getting Started

### Prerequisites

*   [Rust & Cargo](https://www.rust-lang.org/tools/install)
*   [Node.js (v20+) & npm](https://nodejs.org/)
*   [Tauri v2 Prerequisites](https://v2.tauri.app/start/prerequisites/)

### Installation & Development

1.  **Clone the repository:**
    ```sh
    git clone https://github.com/minutogit/human-money-app.git
    cd human-money-app
    ```

2.  **Install dependencies:**
    ```sh
    npm install
    ```

3.  **Run in development mode:**
    ```sh
    npm run dev
    ```
    This starts the Vite dev server and the Tauri window simultaneously.

## 📄 License

This project is licensed under the MIT License. See the `LICENSE` file for more details.