# Ship v1 builds unsigned on both platforms

Code-signing costs real recurring money: an Apple Developer Program membership (~$99/yr) for macOS notarization, and a code-signing certificate (~$100-400/yr) for Windows. Clipdeck has no subscription or monetization, so there is no revenue to fund either.

We decided to ship unsigned builds for v1. macOS Gatekeeper will block first launch until the user allows it (Control-click > Open on macOS 14 and earlier; System Settings > Privacy & Security > "Open Anyway" on macOS 15 and later, which removed the Control-click bypass); Windows SmartScreen will show an "unknown publisher" warning the user has to click through. Both are documented for end users rather than paid away. Revisit once/if a userbase justifies the recurring cost.

**Known limitation discovered during Ticket 2 (2026-10-09)**: on Windows machines with Smart App Control in enforced ("On") mode, an unsigned build is blocked from running at all - not a dismissible SmartScreen-style warning, but a hard Code Integrity policy block with no per-file "allow anyway" path found. The only ways around it are real code signing/reputation, or the user turning Smart App Control off entirely, which Microsoft states requires reinstalling Windows to undo - far too drastic to ask of anyone casually. This doesn't change the unsigned-for-v1 decision, but it's a materially bigger distribution hurdle than Gatekeeper/SmartScreen alone, worth remembering when Ticket 7 (distribution) and any eventual user-facing install docs get written.

**Status**: accepted
