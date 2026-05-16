// Window icon management: swaps the taskbar/title-bar icon to reflect recording state.
//
// Normal  → dark mic icon (idle)
// Recording → red mic icon  (PTT active or toggle-recording active)
//
// Only compiled on Windows; all public items are no-ops on other platforms.

#[cfg(windows)]
mod inner {
    use std::ffi::CString;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use winapi::ctypes::c_void;
    use winapi::shared::windef::{HBITMAP, HICON};
    use winapi::um::wingdi::{
        CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use winapi::um::winuser::{
        CreateIconIndirect, DestroyIcon, FindWindowA, SendMessageA, ICONINFO, WM_SETICON,
    };

    const ICON_SMALL_ID: usize = 0;
    const ICON_BIG_ID: usize = 1;
    const SIZE: i32 = 32;

    // Stores raw HICON pointer so we can destroy the old one on next update.
    static CURRENT_ICON: AtomicUsize = AtomicUsize::new(0);

    // ── BGR colour constants (u32 = 0x00RRGGBB as stored in a 32-bpp BI_RGB DIB) ──
    // Dark background (same as app bg)
    const BG_NORMAL: u32 = 0x002C_313A; // #2C313A dark charcoal
                                        // Red background while recording
    const BG_RECORD: u32 = 0x00C6_2828; // #C62828 deep red

    const WHITE: u32 = 0x00F5_F5F5; // off-white for mic body

    // ── Icon pixel drawing ────────────────────────────────────────────────────────

    /// Fill one pixel if it is inside the canvas.
    #[inline]
    fn put(buf: &mut [u32; (SIZE * SIZE) as usize], x: i32, y: i32, c: u32) {
        if (0..SIZE).contains(&x) && (0..SIZE).contains(&y) {
            buf[(y * SIZE + x) as usize] = c;
        }
    }

    /// Fill a horizontal span.
    fn hline(buf: &mut [u32; (SIZE * SIZE) as usize], y: i32, x0: i32, x1: i32, c: u32) {
        for x in x0..=x1 {
            put(buf, x, y, c);
        }
    }

    /// Draw the microphone shape (white pixels) on top of a pre-filled background.
    ///
    /// Layout (32 × 32, centred):
    ///   Body capsule : rows  3-17, cols 11-20  (oval caps on top & bottom)
    ///   Stand arc    : rows 18-24, centred at col 15
    ///   Stem         : rows 24-27, cols 15-16
    ///   Base bar     : rows 27-28, cols 11-20
    fn draw_mic(buf: &mut [u32; (SIZE * SIZE) as usize]) {
        let cx = 15i32; // horizontal centre (0-indexed)

        // ── Body capsule ──────────────────────────────────────────────────────────
        // Top semicircle: centre (cx, 8), r=5
        for y in 3..9i32 {
            for x in 9..23i32 {
                let dx = x - cx;
                let dy = y - 8;
                if dx * dx + dy * dy <= 25 {
                    put(buf, x, y, WHITE);
                }
            }
        }
        // Rectangular shaft
        for y in 8..16i32 {
            hline(buf, y, 10, 20, WHITE);
        }
        // Bottom semicircle: centre (cx, 16), r=5
        for y in 16..22i32 {
            for x in 9..23i32 {
                let dx = x - cx;
                let dy = y - 16;
                if dx * dx + dy * dy <= 25 {
                    put(buf, x, y, WHITE);
                }
            }
        }

        // ── Stand arc (U-shape below mic) ──────────────────────────────────────
        // Annulus: outer r=8, inner r=5, only the lower half (dy >= 0)
        // Centred at (cx, 21)
        for y in 21..29i32 {
            for x in 6..26i32 {
                let dx = x - cx;
                let dy = y - 21;
                let d2 = dx * dx + dy * dy;
                if (25..=64).contains(&d2) && dy >= 0 {
                    put(buf, x, y, WHITE);
                }
            }
        }

        // ── Stem ─────────────────────────────────────────────────────────────────
        for y in 29..31i32 {
            hline(buf, y, 14, 16, WHITE);
        }

        // ── Base bar ─────────────────────────────────────────────────────────────
        hline(buf, 31, 10, 20, WHITE);
    }

    /// Build a 32×32 pixel buffer for the icon.
    fn make_pixels(recording: bool) -> [u32; (SIZE * SIZE) as usize] {
        let bg = if recording { BG_RECORD } else { BG_NORMAL };
        let mut buf = [bg; (SIZE * SIZE) as usize];

        // Outer rounded corners: cut the very corners to make a softer square.
        const CORNER: &[(i32, i32)] = &[
            (0, 0),
            (1, 0),
            (0, 1),
            (31, 0),
            (30, 0),
            (31, 1),
            (0, 31),
            (1, 31),
            (0, 30),
            (31, 31),
            (30, 31),
            (31, 30),
        ];
        for &(x, y) in CORNER {
            buf[(y * SIZE + x) as usize] = 0; // transparent (AND mask handles it)
        }

        draw_mic(&mut buf);
        buf
    }

    // ── HICON creation ────────────────────────────────────────────────────────────

    /// Convert a pixel buffer (32-bpp 0x00RRGGBB, top-down) to an HICON.
    unsafe fn pixels_to_hicon(pixels: &[u32; (SIZE * SIZE) as usize]) -> HICON {
        // ── Colour (XOR) bitmap ───────────────────────────────────────────────────
        let hdc = CreateCompatibleDC(std::ptr::null_mut());

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE,
            biHeight: -SIZE, // negative = top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let hbm_color: HBITMAP = CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );

        if !hbm_color.is_null() && !bits.is_null() {
            std::ptr::copy_nonoverlapping(
                pixels.as_ptr() as *const u8,
                bits as *mut u8,
                (SIZE * SIZE * 4) as usize,
            );
        }

        // ── Mask (AND) bitmap: 1 bpp, 4 bytes per row (DWORD-aligned) ────────────
        // Bit = 0 → opaque (show XOR colour), bit = 1 → transparent.
        // Default: all opaque (0x00). Transparent pixels were already zeroed out of
        // the colour buffer; set their mask bits to 1 for correct transparency.
        const MASK_BYTES: usize = (SIZE * SIZE / 8) as usize; // 128 bytes
        let mut mask = [0x00u8; MASK_BYTES];

        // Mark corner pixels (that were zeroed above) as transparent.
        let corner_pixels: &[(i32, i32)] = &[
            (0, 0),
            (1, 0),
            (0, 1),
            (31, 0),
            (30, 0),
            (31, 1),
            (0, 31),
            (1, 31),
            (0, 30),
            (31, 31),
            (30, 31),
            (31, 30),
        ];
        for &(x, y) in corner_pixels {
            let bit_pos = (y * SIZE + x) as usize;
            mask[bit_pos / 8] |= 0x80 >> (bit_pos % 8);
        }

        let hbm_mask: HBITMAP = CreateBitmap(SIZE, SIZE, 1, 1, mask.as_ptr() as *const c_void);

        // ── Compose icon ─────────────────────────────────────────────────────────
        let mut info = ICONINFO {
            fIcon: 1, // TRUE = icon
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };
        let hicon = CreateIconIndirect(&mut info);

        // Bitmaps are copied into the icon; release originals.
        if !hbm_color.is_null() {
            DeleteObject(hbm_color as *mut _);
        }
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask as *mut _);
        }
        DeleteDC(hdc);

        hicon
    }

    // ── Public entry point ────────────────────────────────────────────────────────

    /// Update the window icon to reflect the current recording state.
    /// Safe to call from any thread.
    pub fn update(recording: bool) {
        unsafe {
            let pixels = make_pixels(recording);
            let hicon = pixels_to_hicon(&pixels);
            if hicon.is_null() {
                return;
            }

            // Find the app window by title.
            let title = CString::new("whisper-typeless").unwrap();
            let hwnd = FindWindowA(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                SendMessageA(hwnd, WM_SETICON, ICON_SMALL_ID, hicon as isize);
                SendMessageA(hwnd, WM_SETICON, ICON_BIG_ID, hicon as isize);
            }

            // Swap and destroy the old icon.
            let old = CURRENT_ICON.swap(hicon as usize, Ordering::Relaxed);
            if old != 0 {
                DestroyIcon(old as HICON);
            }
        }
    }
}

// ── Platform-neutral façade ───────────────────────────────────────────────────

/// Update the taskbar / title-bar icon to reflect recording state.
/// Does nothing on non-Windows platforms.
pub fn update(recording: bool) {
    #[cfg(windows)]
    inner::update(recording);

    #[cfg(not(windows))]
    let _ = recording;
}
