# Hardware parity with Microsoft Word

> **Last reviewed:** 2026-10-12 · **Last updated:** 2026-10-13 · **Change:** trivial (Android touch gate, keyboard bridge and mouse rows; previously trivial: Android hardware keyboard, clipboard and selection handle rows; previously trivial: touch screen rows for the unofficial Android build; previously trivial: 32-bit Windows starts on OpenGL; previously major: first version) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4 and Word for Windows

> **Printing update:** 2026-10-12 · direct desktop printing through craft-print; details below.

Word is not a hardware-heavy app, but a few devices matter to its users: printers above all, then
pens, microphones (Dictate) and speakers (Read Aloud), and displays.

**Dimension: ~40% (estimated), 20–40 h to full** (pen/ink hours are counted with the Draw tab in
[`target-app-parity.md`](target-app-parity.md)).

| Hardware | Word | WordCraft macOS | Windows | Linux / BSD | Web | Parity | Hours |
|---|---|---|---|---|---|---|---|
| Printers: native print dialog, printer choice, copies, duplex, page ranges, tray, scaling | ✅ (`WordPDE.plugin` print-dialog extension on Mac) | 🟡 Direct CUPS printing; printer, copies, collation, duplex, ranges and grayscale | 🟡 Direct spooler printing; printer, copies, collation, duplex, ranges and grayscale | 🟡 Direct spooler printing; printer, copies, collation, duplex, ranges and grayscale | ✅ browser print dialog (#209) | 35% | 5–8 |
| GPU rendering, HiDPI/Retina, pixel-aligned text | ✅ | ✅ wgpu + vello_cpu rasteriser, whole-pixel text (#110, #165) | ✅ DirectX 12 default (#50), OpenGL when DirectX 12 has no GPU driver (#316) or on 32-bit; startup crash on Intel UHD (#170) | ✅ (soft text on KDE at some zooms, #140) | ✅ | 80% | 3–5 (bugs) |
| Multiple monitors, per-monitor DPI, remembered window position | ✅ | ✅ (#38, #76) | ✅ (winit DPI fix pending, #155) | ✅ | — | 85% | 1 |
| Pen / stylus: ink, pressure, eraser, ink to shape/math | ✅ (Draw tab, `InkRender.bundle`) | ❌ | ❌ | ❌ | ❌ | 0% | (Draw tab, 15–25) |
| Microphone: Dictate | ✅ (cloud speech service) | ❌ | ❌ | ❌ | ❌ | 0% | 8–15 + owner (speech model choice) |
| Speakers: Read Aloud | ✅ | ✅ system speech, speed, sentence skip (#190) | ✅ | 🟡 | 🟡 | 75% | 1–2 |
| Touch screens and touchpads: pinch zoom, inertial scroll | ✅ | ✅ pinch (#179) | ✅ | ✅ inertial scroll (#252) | ✅ | 85% | 0–1 |
| Scanners/cameras (Insert from device / Continuity Camera on Mac) | ✅ | ❌ | ❌ | ❌ | ❌ | 0% | 2–4 |

## Android

WordCraft's own unofficial Android build (`apps/wordcraft-android`, for personal sideloading) is not scored in the dimension above: Word's own Android app is out of scope. The rows below say what the build does with the hardware of a phone or a tablet, read from the source, so they have no parity or hours columns. None of it has been tried on a physical device or an emulator; the library compiles and links for `aarch64-linux-android`, and the touch handling is covered by headless tests that feed egui touch events.

| Hardware | WordCraft on Android |
|---|---|
| Touch screen: finger scroll | One finger dragging on the page scrolls it, and the page coasts after the finger lifts; the caret and selection the press changed are put back. A touch that belongs to a tool (a picture drag, a table border, the pen) is never taken over. `crates/ui-egui/src/touch.rs`, with tests in that file and `a_finger_drag_scrolls_the_page_and_a_tap_places_the_caret` in `crates/ui-egui/src/lib.rs` |
| Touch screen: tap, select and context menu | A finger's press is held back (`touch::Gate`, Android shell only) until the finger lifts (a tap: the page gets the press, and its release with the next frame), travels (a scroll: the page never saw a press) or rests for 0.2 s (the page gets the press); the pointer's moves always go through, because egui starts a pinch only when the pointer has a position. A touch outside the page, over a dialog or menu, in the 18 point strips where the page's scroll bars are (only on an axis the page overflows), with a tool (pen, eraser, lasso, Draw Table) selected or as the second tap of a double tap is the page's at once, and on File the page isn't under the finger at all. A double tap then a drag selects by words, as on the desktop. Pinch zoom keeps the point under the fingers under them. Resting a finger opens the context menu and selects the word about 0.5 s after the press (egui's long touch, with `max_click_duration` set to 0.5 s, which is also how long a press on a button may last and still be a click); the page doesn't scroll while the finger rests. A picture or a table border held that way can be dragged; a swipe never moves one. Ribbon buttons, menus and dialogs are the desktop ones: touch targets are still desktop sized, and the only adjustment is that the first start sets the interface size to 125 percent on a screen 600 dp wide or more, 100 percent on a narrower one. `crates/ui-egui/src/touch.rs`, frame by frame tests in `crates/ui-egui/src/tests_touch.rs` |
| Touch screen: pinch zoom | Two fingers zoom the page with egui's own pinch handling, the same code the desktop uses (#179); a second finger cancels a scroll in progress |
| On-screen keyboard | The build uses GameActivity (not NativeActivity) because only it delivers soft keyboard and composing text to the app; winit drops that text, so the app reads the keyboard's buffer itself (`apps/wordcraft-android/src/ime.rs`, `keyboard.rs`) and keeps it filled with the text before the caret, so Backspace, capitalisation and suggestions follow the document. A keyboard button at the right end of the status bar (Android only, command `ui.keyboard`) focuses the page and asks the system to show the keyboard. Moving the keyboard's own cursor (space bar swipe) restarts its buffer at the editor's caret; a paste of more than 64 KiB through the keyboard's clipboard suggestion can crash the app (GameTextInput's fixed buffer). Typing through the on-screen keyboard, composing text included, has not been tried on a device |
| Mouse and trackpad | winit's Android backend turns every button press of a Bluetooth mouse or a DeX pointer into a touch and ignores the wheel and hover, so a mouse behaves like a finger: click-dragging scrolls, the wheel does nothing |
| Hardware keyboard | Letters, Enter, Backspace and the arrow, Home, End and Page keys work. Ctrl, Shift and Alt are never reported by winit's Android backend, so Ctrl+B, Ctrl+C, Ctrl+S and Shift+arrows do nothing; the ribbon and the context menu are the way. Fixing it needs a patch to winit. While the on-screen keyboard is up, Android keeps the arrow keys for the keyboard's own buffer, so with a hardware keyboard the page does not raise it by itself. Not tried on a device |
| Clipboard | Copy and paste work inside the app only: there is no bridge to the system clipboard |
| Selection handles | None: a double tap or a long press selects a word, and a drag selects by words; there are no handles to extend a selection and no magnifier |
| Stylus | No stylus work: no pressure, tilt or palm rejection |
| GPU rendering | eframe with wgpu on Vulkan; nothing has been drawn on a device yet |
| Printers | None: the Android host offers no printing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-13 | trivial | Android (fourth pass, after the third review): a cancelled touch or a focus loss releases the press the page has; a pinch keeps the point under the fingers; the bottom strip belongs to the gate only when a horizontal bar exists; a drift after the menu opened is no scroll; a coast against the document's end stops; the keyboard's page ownership test was a contradiction (always false) and is fixed; the quiet-moment refill only runs when the buffer's length is off; typing right after a write is accepted at once |
| 2026-10-13 | trivial | Android (third pass): the touch gate lets the pointer's moves through (a pinch needs them), delivers a tap's release one frame after its press, keeps the scroll bar strips and the File tab out of its area and asks for its wake frame inside the pass; the click duration is 0.5 s; the keyboard bridge keeps a composing word out of the document while a text field has the keyboard, restarts on a release, refuses buffers that aren't an edit, and drops what it reads right after writing the buffer |
| 2026-10-13 | trivial | Android (after the second review): a finger's press waits until it is a tap, a scroll or a hold; the keyboard's buffer follows the caret; a mouse counts as a finger; the 600 dp rule for the first interface size |
| 2026-10-13 | trivial | Android: hardware keyboard modifiers (Ctrl, Shift, Alt never reported by winit), no system clipboard bridge and no selection handles listed as missing |
| 2026-10-13 | trivial | Android section for the unofficial sideload build: finger scroll, double tap and long press, pinch, on-screen keyboard button, desktop-sized touch targets, no stylus work, no printing; none of it tried on a device, not scored, no percentage change |
| 2026-10-12 | trivial | 32-bit Windows starts on OpenGL: DirectX 12 graphics pipeline creation returns "The parameter is incorrect" and the app exited before the window appeared |
| 2026-10-12 | minor | Desktop File > Print now discovers printers and submits PDF jobs through pinned craft-print with printer choice, copies, duplex and grayscale; browser printing and PDF export remain available. The upstream preview, page ranges, parity and Print Markup remain available. Remaining print parity includes driver-specific tray/options and pages-per-sheet UI; physical printer fidelity needs hardware validation. |
| 2026-10-10 | trivial | Windows falls back to OpenGL when DirectX 12 offers only the software adapter (#316) |
| 2026-10-10 | trivial | Desktop File › Print opens the PDF in the system viewer (#286) |
| 2026-10-10 | trivial | Inertial touchpad scrolling landed (#252) |
| 2026-10-10 | major | First version |
