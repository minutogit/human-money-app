# Frequently Asked Questions (FAQ)

A comprehensive guide answering the most common questions about the Human Money App and the underlying value exchange system.

---

## Table of Contents

- [General Concept](#general-concept)
- [How It Differs from Existing Systems](#how-it-differs-from-existing-systems)
- [Vouchers & Standards](#vouchers--standards)
- [Transactions & Offline Use](#transactions--offline-use)
- [Trust, Identity & Reputation](#trust-identity--reputation)
- [Security & Fraud](#security--fraud)
- [Privacy](#privacy)
- [Getting Started & Using the App](#getting-started--using-the-app)
- [Technical & Developer Questions](#technical--developer-questions)
- [Community & Future](#community--future)

---

## General Concept

### What is Human Money?

Human Money is a decentralized value exchange system where **people, businesses, and communities create their own digital value instruments** (called vouchers). These vouchers are backed by whatever the issuer chooses — their skills, time, products, services, or any other real-world value. The system operates entirely offline, without a blockchain or central server, and relies on human trust and reputation rather than algorithms.

### Why is it called "Human Money"?

Because value creation starts with the **human**, not with technology. In traditional systems, money is issued by central banks or minted by protocol algorithms. In Human Money, every person can issue their own value instruments, backed by their own abilities and promises. The name reflects the philosophy that economic exchange should be centered around people.

### Is this a cryptocurrency?

No. Human Money is fundamentally different from cryptocurrencies:
- There is **no blockchain** and no global ledger
- There is **no mining**, no proof-of-work, no proof-of-stake
- There are **no tokens** to speculate on
- The system is built on **trust**, not trustlessness
- It works **completely offline**

Human Money is best understood as a new class of **complementary digital value instruments** — closer in spirit to mutual credit systems and local currencies than to Bitcoin or Ethereum.

### Is this a replacement for regular money?

No. Human Money is designed as a **complement** to existing monetary systems, not a replacement. It opens new possibilities that traditional money and cryptocurrencies cannot offer — such as allowing individuals to create their own value instruments, maintaining hyperlocal economic flows, and operating without any infrastructure dependency.

### Who is behind this project?

The project is supported by the **Menschlich Miteinander** association ([menschlich-miteinander.org](https://menschlich-miteinander.org/en/)), whose mission is to redesign the tools of exchange for more self-determination, genuine community, and living diversity. The software is open source (MIT License) and welcomes community contributions.

---

## How It Differs from Existing Systems

### How is this different from Bitcoin / Ethereum / other crypto?

| Aspect | Cryptocurrencies | Human Money |
|---|---|---|
| Trust model | Trustless — math replaces human relationships | Trust-based — your DID key builds reputation over time |
| Fraud approach | Prevent fraud via consensus | Detect fraud deterministically after the fact |
| Infrastructure | Requires blockchain + network consensus | No infrastructure needed — works fully offline |
| Who creates value | Protocol/miners/validators | Any person, business, or community |
| Data storage | Global public ledger | Knowledge lives only in the exchanged files |
| Capital flow | Tends to accumulate centrally | Hyperlocal — value always returns to its issuer |

### How is this different from PayPal / Venmo / bank transfers?

These are centralized payment services that move existing fiat currency between bank accounts. They require:
- An internet connection at all times
- A trusted third party (the company) to process every transaction
- Bank accounts and identity verification through centralized institutions

Human Money requires **none of these**. Transactions are direct file exchanges between people, with no intermediary.

### How does this compare to local/regional currencies (Chiemgauer, LETS, time banks)?

Human Money can **represent and digitize** any of these systems. A regional currency association could define their own standard within the app and use it to manage their local currency digitally. Key advantages over existing complementary currency systems:

- **No central server or database** needed to track balances
- **Every participant can issue** vouchers (not just the organization)
- **Automatic fraud detection** built into the cryptography
- **Works offline** — no internet dependency for transactions
- **Portable** — vouchers are files that can be exchanged over any medium
- **Flexible standards** — time-backed, commodity-backed, fiat-pegged, or custom

### What can Human Money do that other systems cannot?

The most unique properties are:

1. **Individual value creation:** Every person can issue their own value instruments, not just organizations
2. **Hyperlocal by design:** Every voucher inherently returns to its issuer, preventing capital drain from communities
3. **Zero infrastructure:** No servers, no blockchain, no internet required for transactions
4. **Cryptographic fraud detection:** Fraud is not prevented but is mathematically guaranteed to be discovered
5. **Flexible standards:** The same system can represent time banks, regional currencies, business credit, commodity-backed instruments, or anything else

---

## Vouchers & Standards

### What exactly is a "voucher"?

A voucher is a **digitally signed JSON file** that represents a claim of value. Think of it as a digital banknote that carries its complete transaction history within itself — like a self-contained mini-blockchain. Each voucher contains:

- The **issuer's identity** (DID key) and their promise of value
- The **standard** it follows (rules about backing, validity, signatures)
- A **chain of signatures** documenting every transfer from person to person
- An **expiration date**

### What is a "standard"?

A standard is a set of rules defined in a TOML configuration file that determines how a type of voucher works. It specifies:

- **What backs the voucher** (time, goods, services, fiat currency, etc.)
- **How long vouchers are valid**
- **What signatures are required** (e.g., must 2 guarantors also sign?)
- **Denomination and units** (hours, euros, custom units)
- **Any additional rules** the community agrees on

Anyone can create a new standard. A village council could define a "Dorftaler" standard; a business could define a "Service Credit" standard; a cooperative could define a time-bank standard.

### What is the FreeTaler standard?

FreeTaler is a **test standard** included with the app. It has no real-world value and is intentionally worthless. Its purpose is to let new users safely experiment with creating vouchers, sending them, and receiving them — without any risk.

### What is the Minuto standard?

Minuto is inspired by the existing paper-based [Minuto Cash](https://minutocash.org/) system, which has been tested by various community groups. In this standard:

- Vouchers are **backed by the issuer's time and skills** (e.g., 1 Minuto ≈ 1 minute of quality work)
- Optionally, **two guarantors** can add their signatures for additional security
- The standard demonstrates how a real-world complementary currency can work digitally

Note: The Minuto standard included in the app is currently a demonstration. While technically capable of creating binding vouchers, production use is not yet recommended during this early prototype phase.

### Can I create my own standard?

Yes — that is one of the core ideas. Standards are defined as TOML files and can represent virtually anything:

- **Time-based:** 1 unit = 1 hour of the issuer's work
- **Commodity-backed:** 1 unit = 1 kg of wheat, 1 liter of olive oil, etc.
- **Fiat-pegged:** 1 unit = 1 Euro (managed by a local association)
- **Service credits:** 1 unit = one specific service (e.g., a haircut, a tutoring session)
- **Community-defined:** Whatever rules a group of people agrees on

**Please note:** While custom standards are fully supported by the core architecture, the app does not yet have an interface to easily import and manage new standards. To avoid application bloat, standard creation may be handled by external developer tools, but direct support for importing and using custom standards within the app is a planned feature on our roadmap. Currently, new standards must be manually integrated into the application's configuration files.

### Do vouchers expire?

Yes. Vouchers have a **defined validity period** set by the standard. This is intentional and serves an important purpose: it encourages circulation rather than hoarding. In practice, vouchers remain value-stable as long as they circulate quickly and are redeemed by the issuer before expiry. When a voucher is redeemed, the issuer can create new ones, maintaining healthy economic flow.

### What does "hyperlocal" mean in this context?

Every voucher is intrinsically tied to its issuer. No matter how far it travels or how many hands it passes through, it eventually **returns to the person who created it** — who must then honor their promise (deliver the work, goods, or service that backs it). This means:

- Value naturally circulates within the issuer's local community
- Money cannot drain away from a region (unlike fiat currency)
- Even at national scale, the system preserves its local character
- Capital accumulation is structurally limited

---

## Transactions & Offline Use

### How do I send value to someone?

Sending value means **transferring a voucher file** to another person. In the app, you:

1. Select one or more vouchers from your wallet
2. Choose the recipient (by their DID key or from your address book)
3. Choose an encryption mode (DID-encrypted, password-protected, or cleartext)
4. The app creates a signed transfer bundle (a file)
5. You send this file to the recipient via **any method** — email, USB stick, messaging app, local network, QR code, etc.

### How do I receive value?

Receiving value means **importing a transfer bundle file** into your app:

1. Open or drop the received file into the app
2. The app verifies the cryptographic signatures automatically
3. If the file is encrypted, you may be prompted for a password
4. The voucher appears in your wallet with its full transaction history

### Does this really work without internet?

Yes, completely. The transaction itself is a **file exchange** — no server, no network, no internet required. You can exchange vouchers via:

- USB sticks
- Bluetooth / AirDrop
- Email (when you have occasional connectivity)
- SD cards
- Any file-sharing method

The system is designed for environments where internet access may be unreliable, expensive, or undesirable.

### What happens if I'm offline for a long time?

Nothing negative. Since there is no central server to synchronize with, being offline has no impact on your existing vouchers or your ability to transact locally. The only consideration is that **fraud detection gossip** (anonymous fingerprints that help detect double-spending) is distributed more slowly when participants are offline for extended periods.

### Can I send vouchers to someone I don't know?

Technically yes, but the system is designed around **known relationships**. Trust is fundamental — you should know who you are accepting value from, because:

- You need to assess whether the issuer's promise is credible
- You take responsibility for the vouchers you accept
- If a problem occurs, you need to be able to trace the chain back to known people
- Accepting vouchers from unknown identities increases risk

### What file formats are used?

The app uses several file types for different workflows:
- **Transfer bundles** — encrypted or cleartext files containing signed vouchers
- **Signature request files** (`.ask`) — requests for additional signatures (e.g., from guarantors)
- **Signature response files** (`.sig`) — completed signature contributions

All files are human-readable JSON (though encrypted content requires the app to decrypt).

---

## Trust, Identity & Reputation

### What is a DID key?

A DID (Decentralized Identifier) key is your **cryptographic identity** in the Human Money system. It is a public key derived from your secret mnemonic seed phrase. Think of it as your digital fingerprint — it uniquely identifies you in every transaction you participate in, and it is used to:

- **Sign** vouchers and transfers (proving you authorized them)
- **Verify** incoming transfers (confirming who sent them)
- **Build reputation** over time through honest behavior

### Why is trust important? Isn't trustless better?

"Trustless" systems (like Bitcoin) solve a specific problem: enabling transactions between people who don't know or trust each other. But this comes at enormous cost — massive energy consumption, slow transaction speeds, and zero recourse if something goes wrong.

Human Money takes the opposite approach: **trust is a feature, not a bug**. In real-world communities, people already know and trust each other to varying degrees. The system leverages this existing social fabric rather than trying to replace it with algorithms. Trust-based systems are:

- Far more **efficient** (no consensus overhead)
- More **resilient** (no infrastructure dependency)
- More **humane** (aligned with how real communities work)
- **Self-regulating** (bad actors face real social consequences)

### How does reputation work?

Your DID key accumulates reputation over time through your behavior:

- **Honest transactions** build trust — people see that you honor your vouchers
- **Fraud** destroys reputation — double-spending is cryptographically proven and publicly attributable
- **Community standing** matters — your reputation exists in the eyes of the people you transact with

There is no central "reputation score." Reputation is organic and social — the people in your network decide whether to accept your vouchers based on their experience with you.

### What if I lose access to my identity / DID key?

Your DID key is derived from a **BIP-39 mnemonic seed phrase** — a series of words that you write down and store safely during profile creation. If you lose access to your device, you can **recover your identity** using this seed phrase on any new device.

The app supports multiple languages for mnemonic phrases, including a custom high-security German wordlist.

> **Important:** If you lose your seed phrase, your identity cannot be recovered. There is no "forgot password" feature — this is the trade-off for a fully decentralized system.

### Can I have multiple identities?

The app supports **multi-profile management** and **Separated Account Identity (SAI)**, which allows strict account separation (e.g., one identity for your PC, another for your phone) derived from a single mnemonic. This prevents state inconsistencies across devices while maintaining a single root identity.

---

## Security & Fraud

### What prevents someone from creating unlimited vouchers?

Nothing technically prevents it — and that is by design. Anyone **can** issue vouchers freely. However:

- Every voucher is **signed with your DID key**, tying it to your identity
- If you issue more than you can honor, people will **stop accepting your vouchers**
- Your **reputation** is your limit — it is the equivalent of a credit rating, but organic and decentralized
- Standards can require **guarantor signatures**, adding a social layer of accountability

This is similar to how trust works in the real world: a local carpenter can promise services and be trusted by their community, but if they over-promise, they lose credibility.

### How exactly does fraud detection work?

The system uses **NIZK Identity Traps** (Non-Interactive Zero-Knowledge proofs) embedded in the signature chain:

1. When you transfer a voucher, you sign the transaction with your private key
2. If you try to transfer the **same voucher to two different people** (double-spend), you create two different signatures for the same state
3. These two signatures mathematically form a **proof of fraud** — they expose your private key identity deterministically
4. As vouchers circulate, the paths eventually converge (even if not directly — via gossip fingerprints)
5. The fraud is **always discovered**, and the proof is undeniable

### What happens when fraud is detected?

When a double-spend is detected:

1. The system generates a **cryptographic conflict proof** linking both transactions to the same identity
2. Both affected vouchers are **quarantined** automatically
3. The fraudster's DID key is **flagged** across the network via gossip
4. All participants can see "Known Offender" warnings for that identity
5. Community members can add **local conflict notes** (and optionally forgive if the situation is resolved)

The fraudster effectively **excludes themselves** from the community. Their vouchers will no longer be accepted, and their DID key is permanently tainted with mathematical proof of dishonesty.

### Can someone just create a new identity after committing fraud?

They can create a new DID key, but:

- A brand-new identity has **zero reputation** — nobody will accept their vouchers
- Building trust takes time and real transactions with real people
- Creating many fake identities (**Sybil attack**) is possible but impractical — each new identity starts from zero trust, and the people who first accepted vouchers from an unknown DID become suspects in the fraud chain
- The system incentivizes **long-term honest behavior** as the optimal strategy

### What is the WalletSeal?

WalletSeal is a **rollback protection mechanism**. The wallet continuously stores a cryptographic hash of its current state. If someone tries to restore an older backup of the wallet (e.g., to "un-spend" a voucher they already sent), the hash mismatch is detected and the wallet **locks down**, requiring explicit recovery.

### What is Device Binding?

Device Binding prevents accidental **wallet cloning**. Each wallet instance is bound to a unique Host/Instance ID. If you copy your wallet file to another device (intentionally or accidentally), the system detects this and initiates a secure **device handover** workflow instead of allowing two copies to operate independently.

---

## Privacy

### Who can see my transactions?

Only people who have **direct access to the voucher files** can read the transaction chain. There is no public ledger. Specifically:

- **You** can see all transactions in vouchers you hold (though identities of previous owners who transferred in Stealth Mode remain hidden).
- **The recipient** of a transfer can see the full chain of the vouchers they receive (along with the identity of the direct sender, even if they used Stealth Mode).
- **The issuer** can see the full chain when a voucher returns for redemption (but cannot see the identities of any intermediate owners who transferred in Stealth Mode).
- **Nobody else** has access unless a file is explicitly shared with them.

### What is Stealth Mode?

Stealth Mode is an optional privacy feature that **hides your DID key** from the transaction chain when you send a voucher. Instead of your real identity, an ephemeral (one-time) key is used. This means:

- Future holders of the voucher **cannot see** that you were a previous owner.
- Only the **direct recipient** of your transfer can identify you (since the identity payload is encrypted specifically for their DID key).
- The **original issuer** cannot use Stealth Mode when creating or issuing a voucher, as the creator's identity must always be public for verification. The issuer also cannot identify subsequent stealth senders (unless they are the direct recipient of their transfer).
- If you commit fraud (double-spend) while in Stealth Mode, the NIZK trap still **mathematically exposes your real identity** to anyone who detects the double-spend.

### Is Stealth Mode recommended?

It depends on the context. Stealth Mode provides privacy, but it introduces a trade-off:

- **Without Stealth Mode:** Full transparency — every participant in the chain is visible. Fraud is trivially traceable.
- **With Stealth Mode:** Enhanced privacy — but if a Sybil identity commits fraud, tracing requires more effort. The people who first accepted vouchers from an unknown stealth sender may become suspects.

For communities where trust and transparency are valued, leaving Stealth Mode off is generally recommended. For situations involving potential repression or where privacy is a priority, Stealth Mode offers protection.

### Can the government / authorities track my transactions?

Since there is no central server, no blockchain, and no public ledger, there is **nothing to subpoena or surveil**. Transaction data exists only in the voucher files held by participants. The only way to access transaction history is to obtain the actual files from the people involved.

---

## Getting Started & Using the App

### How do I install the app?

**Option 1 — Download a pre-built release:**
Visit the [Releases page](https://github.com/minutogit/human-money-app/releases) and download the installer for your operating system (Windows, macOS, or Linux).

**Option 2 — Build from source:**
```sh
git clone https://github.com/minutogit/human-money-app.git
cd human-money-app
npm install
npm run tauri dev
```
This requires Rust, Node.js (v20+), and [Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/).

### What happens when I first open the app?

You are guided through a **profile creation** process:

1. Choose a username and set a password
2. A **BIP-39 mnemonic seed phrase** is generated — **write this down and store it safely!**
3. Your cryptographic identity (DID key) is created automatically
4. You can optionally fill in profile information (name, location, skills, needs)

### How do I get my first vouchers?

Two ways:

1. **Create your own:** Using the "Create Voucher" function, you can issue vouchers based on any available standard (start with FreeTaler for testing)
2. **Receive from others:** Someone sends you a transfer bundle file, and you import it into the app

### Is there a test mode?

Yes. The **FreeTaler** standard is specifically designed for testing. Vouchers created with this standard are worthless and clearly marked as test vouchers in the UI. Use them freely to learn how the system works.

### What languages does the app support?

The app is fully localized in **5 languages**: English, German, Spanish, French, and Italian. The language is detected automatically based on your system settings, and can be changed manually in the app's settings.

### What platforms does the app run on?

The app is a cross-platform desktop application built with [Tauri v2](https://tauri.app/), available for:
- **Windows** (10/11)
- **macOS** (Apple Silicon and Intel)
- **Linux** (AppImage, .deb)

### Can I use the app on my phone?

Not yet. The current MVP is a desktop application. However, the underlying [`human_money_core`](https://github.com/minutogit/human-money-core) Rust library is designed to be platform-agnostic, making it possible for developers to build mobile clients in the future.

---

## Technical & Developer Questions

### What is the tech stack?

- **Frontend:** React 19 + TypeScript + Tailwind CSS v4
- **Desktop Framework:** [Tauri v2](https://tauri.app/) — tiny footprint, maximum security
- **Core Library:** [`human_money_core`](https://github.com/minutogit/human-money-core) — pure Rust
- **Cryptography:** Ed25519 (signatures) + X25519/ChaCha20-Poly1305 (encryption)
- **Storage:** Encrypted local JSON files (no database server)

### What is `human_money_core`?

[`human_money_core`](https://github.com/minutogit/human-money-core) is the open-source Rust library that contains **all business logic, cryptography, and validation**. The desktop app is intentionally a "thin bridge" — it provides the UI but delegates all critical operations to the core library. This architecture means:

- Developers can build their own clients (mobile, web, embedded) using the same core
- Security-critical code is centralized in one auditable library
- The app cannot accidentally bypass cryptographic rules

### Can I build my own app on top of the core library?

Yes, that is explicitly encouraged. Add it to your `Cargo.toml`:

```toml
[dependencies]
human_money_core = { git = "https://github.com/minutogit/human-money-core" }
```

The core library provides a high-level `AppService` facade that handles session management, voucher operations, transfer processing, conflict detection, and more.

### How is data stored?

All data is stored as **encrypted JSON files** on the local filesystem. There is no external database. The `human_money_core` library uses a storage-agnostic trait (`Storage`) with a default encrypted implementation (`FileStorage`). This means:

- Your wallet data never leaves your device unless you explicitly share it.
- **A note on backups:** Although your raw wallet files can be manually copied, restoring older file backups is dangerous and strongly discouraged. Because the wallet relies on strict state tracking, restoring an old wallet state can cause accidental double-spending of vouchers you have already transferred. This would trigger the NIZK trap, revealing your real identity and flagging you as a fraudster. Dedicated, safe backup tools that prevent state rollbacks are planned for future versions.
- No database server to install or maintain.

### What is the Layer 2 concept?

Layer 2 is an **optional, future enhancement** — not required for the system to work. The concept:

- A decentralized online registry where voucher fingerprints can be checked.
- Enables **preventive** double-spend detection (before accepting a voucher, check if it has already been spent elsewhere).
- Requires **no global consensus** — just a simple fingerprint lookup.
- The core library is already prepared for this, but it is not yet implemented.

This layer would add the familiar feeling of "guaranteed payment" without sacrificing the offline-first, decentralized nature of the system. Once established, it will allow the creation of highly performant currency systems that are in no way inferior to the comfort and speed of modern fiat payment systems.

### How do I run the test suite?

```sh
# Full quality gate (ESLint + TypeScript + Vitest + Clippy + Rust tests)
./run-tests.sh --quiet

# Frontend tests only
./run-tests.sh --frontend-only

# Backend tests only
./run-tests.sh --backend-only
```

### How do I contribute?

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes and ensure tests pass (`./run-tests.sh --quiet`)
4. Submit a Pull Request

---

## Community & Future

### Is this production-ready?

Not yet. This is an **early-stage MVP / Prototype** — the first software to make the Human Money concept tangible and testable. Core workflows (create, send, receive, fraud detection) are functional and tested, but the software is not yet recommended for high-value production transactions.

### What is the current development status?

The app is in **beta** (v0.1.0-beta). The following features are complete and functional:
- ✅ Profile creation and encrypted storage
- ✅ Voucher creation, signing, and multi-signature workflows
- ✅ Sending and receiving value transfers (3 encryption modes)
- ✅ Transaction history and audit logs
- ✅ Decentralized fraud/conflict detection and management
- ✅ Address book with contact trust management
- ✅ Wallet recovery via mnemonic seed phrases
- ✅ WalletSeal rollback protection
- ✅ Device binding and anti-cloning
- ✅ Localization in 5 languages

### What is planned for the future?

- 🔜 Layer 2 integration for preventive fraud checks
- 🔜 Trust level badges and Web of Trust integration
- 🔜 BLE/NFC-based face-to-face contact exchange
- 🔜 Mobile clients (leveraging the core Rust library)
- 🔜 Session timeout UX improvements

### How can I support the project?

- ⭐ **Star** the [repository](https://github.com/minutogit/human-money-app) on GitHub
- 🐛 **Report bugs** via [GitHub Issues](https://github.com/minutogit/human-money-app/issues)
- 🏗️ **Contribute code** — the project welcomes Pull Requests
- 💬 **Join the community** at [menschlich-miteinander.org](https://menschlich-miteinander.org/en/)
- 💰 **Donate** to support development at [menschlich-miteinander.org/unterstuetzen](https://menschlich-miteinander.org/unterstuetzen/)
- 📣 **Spread the word** — share the project with communities that could benefit

### Where can I learn more?

- 🌐 **Association website:** [menschlich-miteinander.org](https://menschlich-miteinander.org/en/)
- 📖 **Core library docs:** [human-money-core on GitHub](https://github.com/minutogit/human-money-core)
- 📰 **Blog post — "Everyone prints their own money?":** [Read on menschlich-miteinander.org](https://www.menschlich-miteinander.org/posts/2026/jeder-druckt-sein-eigenes-geld/) *(German)*
- 📄 **Minuto paper system:** [minutocash.org](https://minutocash.org/)

---

<div align="center">

*Have a question that isn't answered here? [Open an issue](https://github.com/minutogit/human-money-app/issues) and we'll add it!*

</div>
