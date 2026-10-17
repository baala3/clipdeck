# History and Pinned work alike: both bounded, both listed in Settings

Amends [ADR-0014](0014-settings-in-a-sidebar-window.md) and the Pinned list of [ADR-0006](0006-clipy-style-native-menus.md), which had no capacity cap.

The Settings Window listed every pinned item with Edit and Remove, but its History page only set how many Clips to keep; History's Clips could be managed only from the menu.
The user wanted the two to behave the same, while staying two lists: "Pin tab should show all and History tab should show all history", each with a limit and the same actions.

So both pages now have the same shape:
- a capacity, "Clips to keep", with how many the list holds now;
- every Clip in the list, newest first, each with Edit (text only) and Remove;
- History adds Pin or Unpin on each Clip, and a Clear History button; Pinned adds New.

The Pinned list is bounded like History: once it's full, pinning or writing one more removes its oldest item.
The user chose this over refusing new pins, so both lists behave the same; the default is 50, as for History.

Pinned items are ones the user chose to keep, so two guards keep the limit from throwing them away unasked:
- Lowering either limit below what its list holds asks first, in the page, saying how many of the oldest Clips go.
- A settings file from before the Pinned limit existed gets a limit big enough for the items already pinned (`SettingsFile::predates`, `Settings::make_room_for_pinned`), so updating Clipdeck removes nothing. New installs start at 50.

A History Clip's Edit in the menu now opens its editor in the History page's list, as a pinned item's Edit does in Pinned, in place of a separate Edit Clip page.

**Status**: accepted
