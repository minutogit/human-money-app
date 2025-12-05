 Human Money App (Tauri Prototype)

This repository contains the source code for a cross-platform desktop wallet for the human money core. It is built as a prototype using the Tauri framework, ensuring it runs on Windows, macOS, and Linux from a single codebase.

The application serves as a client for the `human_money_core` Rust library, which handles all core business logic for the voucher system.

## ✨ Core Features (MVP)

This prototype focuses on implementing the essential wallet functionalities:

* **Profile Management:** Create a new user profile or log in to an existing one.
* **Dashboard View:** Display the user's total balance aggregated by currency and list all available vouchers.
* **Transactions:** Create and receive voucher transfers through secure data bundles.

## 🛠️ Tech Stack & Architecture

The project follows a strict separation between the backend and frontend, ensuring a clean and maintainable architecture.

* **Framework:** [**Tauri**](https://tauri.app/) for building a lightweight and secure desktop application using web technologies for the frontend.
* **Backend:** Written in **Rust**, the backend acts as a thin bridge, exposing functions from the `human_money_core` to the frontend via Tauri commands. It contains no business logic itself.
* **Frontend:** A modern, responsive UI built with **React**, **TypeScript**, and styled with **Tailwind CSS**.
* **State Management:** Utilizes React's native `useState` hook for simplicity in this prototype stage.

## 🚀 Getting Started

### Prerequisites

To build and run this project, you need to have the following installed on your system:

* [Rust & Cargo](https://www.rust-lang.org/tools/install)
* [Node.js & npm](https://nodejs.org/)
* [Tauri development prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites)

### Installation & Development

1.  **Clone the repository:**

    ```sh
    git clone <your-repository-url>
    cd <repository-folder>
    ```

2.  **Install frontend dependencies:**

    ```sh
    npm install
    ```

3.  **Run the application in development mode:**
    This command will start the frontend's Vite dev server and the Tauri backend process.

    ```sh
    npm run tauri dev
    ```

    Alternatively, you can use the provided shell script:

    ```sh
    ./start-dev.sh
    ```

## 📄 License

This project is licensed under the MIT License. See the `LICENSE` file for more details.