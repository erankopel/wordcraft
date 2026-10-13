# Hardware parity with Microsoft Word

> **Last reviewed:** 2026-10-12 · **Last updated:** 2026-10-13 · **Change:** trivial (touch screen rows for the unofficial Android build; previously trivial: 32-bit Windows starts on OpenGL; previously major: first version) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4 and Word for Windows

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
| Touch screen: select and context menu | A double tap then a drag selects by words, as on the desktop. A long press opens the context menu (egui reports a long touch as a secondary click). Ribbon buttons, menus and dialogs are the desktop ones: touch targets are still desktop sized, and the only adjustment is that the first start sets the interface size to 125 percent |
| Touch screen: pinch zoom | Two fingers zoom the page with egui's own pinch handling, the same code the desktop uses (#179); a second finger cancels a scroll in progress |
| On-screen keyboard | The build uses GameActivity (not NativeActivity) because only it delivers soft keyboard and composing text to the app. A keyboard button at the right end of the status bar (Android only, command `ui.keyboard`) focuses the page and asks the system to show the keyboard. Typing through the on-screen keyboard, composing text included, has not been tried on a device |
| Stylus | No stylus work: no pressure, tilt or palm rejection |
| GPU rendering | eframe with wgpu on Vulkan; nothing has been drawn on a device yet |
| Printers | None: the Android host offers no printing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-13 | trivial | Android section for the unofficial sideload build: finger scroll, double tap and long press, pinch, on-screen keyboard button, desktop-sized touch targets, no stylus work, no printing; none of it tried on a device, not scored, no percentage change |
| 2026-10-12 | trivial | 32-bit Windows starts on OpenGL: DirectX 12 graphics pipeline creation returns "The parameter is incorrect" and the app exited before the window appeared |
| 2026-10-12 | minor | Desktop File > Print now discovers printers and submits PDF jobs through pinned craft-print with printer choice, copies, duplex and grayscale; browser printing and PDF export remain available. The upstream preview, page ranges, parity and Print Markup remain available. Remaining print parity includes driver-specific tray/options and pages-per-sheet UI; physical printer fidelity needs hardware validation. |
| 2026-10-10 | trivial | Windows falls back to OpenGL when DirectX 12 offers only the software adapter (#316) |
| 2026-10-10 | trivial | Desktop File › Print opens the PDF in the system viewer (#286) |
| 2026-10-10 | trivial | Inertial touchpad scrolling landed (#252) |
| 2026-10-10 | major | First version |
