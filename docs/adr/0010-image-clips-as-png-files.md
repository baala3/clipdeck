# Image Clips are PNG files beside the stores, told apart by their pixels

The v1 spec had every copied image captured into History, but only text ever was.
The engine's `ClipContent::Image` held the image's bytes inline, and no adapter produced one.
Finishing that as it stood would not have held up: History is read in full on every capture and every menu, so a couple of hundred screenshots would have meant loading hundreds of megabytes each time.

So an image Clip in History or the Pinned list is now only a handle: a digest, the image's size in pixels, and the size of its file.
The image itself is a PNG that Clipdeck encodes, kept with a small thumbnail in an `images` folder next to the two SQLite stores, behind a third engine trait, `ImageStore`.
History and Pinned share that folder, so pinning an image copies the handle, not the file.
A file is deleted once neither list refers to it, and anything left behind by a crash is swept on the next start.

The digest is a SHA-256 of the decoded pixels, not of the file.
The same picture arrives in different formats (a PNG from a browser, a bitmap from an older app, a TIFF on macOS), and Clipdeck's own copy comes back through the clipboard when a Clip is chosen.
Comparing pixels makes all of those the same Clip, so choosing an image moves it to the top of History instead of adding a twin.
On macOS that only holds because Clipdeck puts the PNG on the pasteboard itself, rather than an `NSImage` that AppKit would re-encode.

Decisions that follow from this:

- **Text wins over an image.**
  A copy that carries both, such as spreadsheet cells, is captured as text.
  The cost is that an app which adds a caption or URL as text next to an image has only that text captured.
- **Bitmaps without a PNG are read as opaque on Windows.**
  Windows converts whatever bitmap format the app copied, and that conversion has no dependable alpha.
  Apps that care about transparency also copy a PNG, which is read first.
- **There are limits.**
  An image over 40 megapixels, or whose PNG is over 20 MB, is not captured.
  History's images may take 256 MB together; past that the oldest image Clips are evicted even if History has room.
  Pinned images are never evicted and don't count.
- **Menus show a thumbnail and the size** ("Image 800 × 500"), since an image has no first line to show.
  ADR-0006 left thumbnails out because Tauri's menu icons are 16 pixels on Windows.
  `clip-windows/src/menu_images.rs` sets larger bitmaps on the native menu instead, all the same size so the rows line up; macOS uses Tauri's icon, which is 18 points high.
- **A pinned image can be removed but not edited.**
- **The stores gained columns rather than a new format.**
  The old `image` column stays, unused, so an older Clipdeck can still open a newer store.

Encryption at rest (ADR-0003), when it is built, has to cover the `images` folder as well as the stores.

The macOS side type-checks but has not been run on a Mac.

**Status**: accepted
