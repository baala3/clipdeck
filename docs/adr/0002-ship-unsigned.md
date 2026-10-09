# Ship v1 builds unsigned on both platforms

Code-signing costs real recurring money: an Apple Developer Program membership (~$99/yr) for macOS notarization, and a code-signing certificate (~$100-400/yr) for Windows. Clipdeck has no subscription or monetization, so there is no revenue to fund either.

We decided to ship unsigned builds for v1. macOS Gatekeeper will block first launch until the user right-clicks and chooses "Open" (or runs a one-time terminal bypass); Windows SmartScreen will show an "unknown publisher" warning the user has to click through. Both are documented for end users rather than paid away. Revisit once/if a userbase justifies the recurring cost.

**Status**: accepted
