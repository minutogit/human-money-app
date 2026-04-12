---
name: core-api-reference
description: Reference documentation for the human_money_core AppService API as used from Tauri commands. Includes all public functions, data structures, and error handling. Note - always verify against actual code for latest API.
---

# AppService: Public API Facade

This directory contains the `AppService`, which is the primary public-facing API for the `human_money_core`. It is designed as a high-level **facade** to simplify all interactions for client applications (e.g., Tauri, mobile apps).

**Key Responsibilities:**
* Manages the wallet state (`Locked` vs. `Unlocked`).
* Abstracts all complex cryptographic operations.
* Handles all persistent storage (encryption and saving) automatically.
* Provides a simple, transactional interface for all wallet operations.

## Core Concept: State Management

The `AppService` operates as a state machine with two states:
* `AppState::Locked`: The initial state. No wallet data is in memory. Only profile listing and login/creation operations are available.
* `AppState::Unlocked`: After a successful `login`, `create_profile`, or `recover_wallet`. All wallet operations (querying, transferring) are now possible.
* `logout()`: Securely transitions the service back to the `Locked` state, clearing all sensitive data from memory.

## Thread Safety and Locking

The `AppService` implements a pessimistic locking mechanism to prevent concurrent modifications of the same wallet by multiple processes, closing a potential "stale state" double-spending vulnerability. All state-changing operations automatically acquire an exclusive lock on the wallet directory during execution and release it upon completion. This ensures thread-safety and data consistency in multi-process environments.



## Authentication Model

Write operations (any function that modifies wallet state) require authentication. This is handled by the `password: Option<&str>` parameter.

* **Mode A (Always Ask):** Call the function with `password: Some("users-password")`. The password is used for this single operation.
* **Mode B (Session):** Call `unlock_session("users-password", 900)` once. You can then call write operations with `password: None` for the specified duration (e.g., 900 seconds).

---

## API Reference

All methods return a `Result<T, String>`, where `String` is a user-friendly error message.

### 1. Lifecycle & Authentication

These methods manage the application state and user profiles.

#### `pub fn new(base_storage_path: &Path) -> Result<Self, String>`
* **Description:** Initializes a new `AppService` in the `Locked` state.
* **Parameters:**
    * `base_storage_path`: The root directory where all user profile sub-directories will be stored.
* **Usage:** This is the first function you must call.

#### `pub fn list_profiles(&self) -> Result<Vec<ProfileInfo>, String>`
* **Description:** Lists all available user profiles found in the `base_storage_path`.
* **Usage:** Call this in the `Locked` state to populate a login screen.

#### `pub fn create_profile(...) -> Result<(), String>`
* **Description:** Creates a new user profile, encrypts the wallet, and logs in.
* **State:** Transitions from `Locked` -> `Unlocked`.

#### `pub fn login(...) -> Result<(), String>`
* **Description:** Unlocks an existing wallet using its `folder_name` and `password`.
* **State:** Transitions from `Locked` -> `Unlocked`.

#### `pub fn recover_wallet_and_set_new_password(...) -> Result<(), String>`
* **Description:** Recovers an existing wallet using its mnemonic phrase and sets a new password.
* **State:** Transitions from `Locked` -> `Unlocked`.

#### `pub fn logout(&mut self)`
* **Description:** Locks the wallet and clears all sensitive data (like private keys and session keys) from memory.
* **State:** Transitions from `Unlocked` -> `Locked`.

---

### 2. Session Management (Optional Auth)

These methods control the "Remember Password" feature (Mode B).

#### `pub fn unlock_session(&mut self, password: &str, duration_seconds: u64) -> Result<(), String>`
* **Description:** Verifies the password and caches a derived encryption key in memory for `duration_seconds`.
* **Usage:** After calling this, write-operations can be called with `password: None`.

#### `pub fn lock_session(&mut self)`
* **Description:** Immediately clears the cached session key from memory, forcing Mode A for the next operation.

#### `pub fn refresh_session_activity(&mut self)`
* **Description:** Resets the inactivity timer of the "Remember Password" session.
* **Usage:** Call this on UI activity (clicks, mouse movements) to keep the session alive while the user is active.

---

### 3. Core Wallet Operations (Commands)

These methods modify the wallet state and require authentication.

#### `pub fn create_new_voucher(standard_toml_content: &str, lang_preference: &str, data: NewVoucherData, password: Option<&str>) -> Result<Voucher, String>`
* **Description:** Creates a new voucher (e.g., "Minuto") based on a standard definition.
* **Status Behavior:** If the standard requires additional signatures (e.g., guarantors, notaries) that are not yet present, the voucher is created with `VoucherStatus::Incomplete`. If all required signatures are present (rare during initial creation), it becomes `VoucherStatus::Active`.
* **Auth:** Requires `password: Option<&str>`.

#### `pub fn create_transfer_bundle(...) -> Result<CreateBundleResult, String>`
* **Description:** The primary function for **sending** value. It bundles one or more voucher `sources` into an encrypted `SecureContainer` (returned as `bundle_bytes: Vec<u8>`) for the recipient.
* **Auth:** Requires `password: Option<&str>`.

#### `pub fn receive_bundle(...) -> Result<ProcessBundleResult, String>`
* **Description:** The primary function for **receiving** value. It processes a `bundle_data` blob. It validates the transaction, checks for double-spending, and adds the new value to the wallet.
* **Auth:** Requires `password: Option<&str>`.

---

### 4. Signature Workflows

Methods for handling multi-role signatures (e.g., guarantors, notaries).

#### `pub fn create_signing_request_bundle(local_instance_id: &str, config: ContainerConfig) -> Result<Vec<u8>, String>`
* **Description:** Creates an encrypted bundle to send a voucher to another user (e.g., a guarantor) requesting their signature.
* **Parameters:**
    * `local_instance_id`: The local ID of the voucher to be signed.
    * `config`: The encryption configuration. Use `ContainerConfig::TargetDid(did)` for a specific recipient, `TargetDids(vec)` for multiple, `Password(pass)` for symmetric encryption, or `Cleartext` for unencrypted transmission.
* **Auth:** Read-only (if wallet is unlocked), no password needed.

#### `pub fn open_voucher_signing_request(container_bytes: &[u8], password: Option<&str>) -> Result<Voucher, String>`
* **Description:** (Called by the signer). Opens a received signature request bundle and returns the voucher part so the user can preview what they are signing.
* **Parameters:**
    * `container_bytes`: The received encrypted bundle.
    * `password`: Optional password if the container was symmetrically encrypted.
* **Auth:** Read-only, no password needed.

#### `pub fn create_detached_signature_response_bundle(voucher_to_sign: &Voucher, role: &str, include_details: bool, config: ContainerConfig, password: Option<&str>) -> Result<Vec<u8>, String>`
* **Description:** (Called by the signer). Creates an encrypted response bundle containing only the detached signature. Additionally, stores the endorsed voucher in the signer's wallet with status `Endorsed` as a legal record.
* **Parameters:**
    * `voucher_to_sign`: The voucher object received in the signing request.
    * `role`: The semantic role of the signer (e.g., `"guarantor"`, `"notary"`).
    * `include_details`: If `true`, the signer's public profile is embedded.
    * `config`: Encryption configuration for the response (usually returning to the requester).
    * `password`: The password for authentication (Wallet Password).
* **Auth:** Requires `password: Option<&str>`.

#### `pub fn process_and_attach_signature(container_bytes: &[u8], standard_toml_content: &str, container_password: Option<&str>, wallet_password: Option<&str>) -> Result<(), String>`
* **Description:** Receives a signature response bundle, validates it, and attaches it locally.
* **Parameters:**
    * `container_bytes`: The received encrypted signature bundle.
    * `standard_toml_content`: The voucher's standard definition (TOML).
    * `container_password`: Optional password to decrypt the response bundle.
    * `wallet_password`: Optional password to unlock the local wallet for saving.
* **Status Behavior:** If this signature fulfills the last missing requirement, status transitions to `Active`.
* **Auth:** Requires `wallet_password: Option<&str>`.

---

### 5. Data Queries (Read-Only)

These methods read data from the `Unlocked` wallet and do not require authentication.

#### `pub fn get_user_id(&self) -> Result<String, String>`
* **Description:** Returns the unique user ID (e.g., `did:key:...`) of the unlocked profile.

#### `pub fn get_voucher_summaries(...) -> Result<Vec<VoucherSummary>, String>`
* **Description:** Returns a list of all vouchers in the wallet, with optional filters for status or standard UUID.
* **Usage:** Ideal for displaying the main wallet dashboard or voucher list.

#### `pub fn get_total_balance_by_currency(&self) -> Result<Vec<AggregatedBalance>, String>`
* **Description:** Returns the sum of all `Active` vouchers, grouped by currency (e.g., "Minuto", "EUR").

#### `pub fn get_voucher_details(&self, local_id: &str) -> Result<VoucherDetails, String>`
* **Description:** Gets all details for a single voucher, including its full transaction history.

#### `pub fn get_allowed_signature_roles_from_standard(toml: &str) -> Result<Vec<String>, String>`
* **Description:** Helper to extract allowed roles (like `"guarantor"`) from a standard definition.

---

### 6. Conflict Management

Methods for handling double-spend conflicts.

#### `pub fn list_conflicts(&self) -> Result<Vec<ProofOfDoubleSpendSummary>, String>`
* **Description:** Lists summaries of all known double-spend conflicts in the wallet.
* **Auth:** Read-only, no password needed.

#### `pub fn get_proof_of_double_spend(&self, proof_id: &str) -> Result<ProofOfDoubleSpend, String>`
* **Description:** Retrieves the full details of a specific double-spend proof by its ID.
* **Auth:** Read-only, no password needed.

#### `pub fn create_resolution_endorsement(...) -> Result<ResolutionEndorsement, String>`
* **Description:** Creates a signed resolution endorsement for a conflict, indicating that the conflict is resolved from the wallet owner's perspective.
* **Auth:** Read-only, no password needed.

#### `pub fn import_resolution_endorsement(...) -> Result<(), String>`
* **Description:** Imports a `ResolutionEndorsement` from another party (e.g., the victim) to mark a local conflict proof as resolved.
* **Auth:** Requires `password: Option<&str>`.

---

### 7. Encrypted Data Storage

Securely save and load arbitrary application data (like contact lists or settings) using the wallet's encryption.

#### `pub fn save_encrypted_data(...) -> Result<(), String>`
* **Description:** Encrypts and saves a byte slice under a specific name.
* **Auth:** Requires `password: Option<&str>`.

#### `pub fn load_encrypted_data(...) -> Result<Vec<u8>, String>`
* **Description:** Loads and decrypts a previously saved byte slice.
* **Auth:** Requires `password: Option<&str>`.

---

### 8. Static Utility Functions

These helper functions can be called at any time, even when the service is `Locked`.

#### `pub fn generate_mnemonic(word_count: u32) -> Result<String, String>`
* **Description:** Generates a new, cryptographically secure BIP-39 mnemonic phrase.
* **Usage:** Used during new profile creation.

#### `pub fn validate_mnemonic(mnemonic: &str) -> Result<(), String>`
* **Description:** Validates a given mnemonic phrase for correctness.
* **Usage:** Used for input validation in recovery or creation forms.