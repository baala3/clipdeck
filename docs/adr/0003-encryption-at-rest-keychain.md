# Encryption at rest is an opt-in beta toggle, keyed from the OS keychain

Clipboard history can contain secrets, but requiring encryption (or a passphrase) for everyone would add friction most users don't want, and the local store already lives inside the user's OS-encrypted home directory in most setups. We decided encryption at rest is off by default and labeled beta, so users who want it can opt in through Settings rather than it being forced on everyone.

When enabled, the key is derived transparently from the OS keychain/credential store - no passphrase prompts. This ties the encrypted data to that machine/OS account (it won't be portable without the same keychain access), which is the trade-off for not burdening the unlock flow with a passphrase. A passphrase-protected, portable export can be a future feature if that trade-off turns out to matter to users.

**Status**: accepted
