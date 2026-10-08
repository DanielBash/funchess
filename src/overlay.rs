// Overlay mode: always on top, borderless, never takes focus, and everything outside
// the "hit" rects lets clicks and scrolling fall through to the window underneath.
use macroquad::prelude::Rect;

pub const TITLE: &str = "Fun Chess Overlay";
pub const WM_CLASS: &str = "funchess-overlay";

#[derive(Default)]
pub struct Overlay {
    tries: u32,
    last: Vec<[i32; 4]>,
    #[cfg(target_os = "linux")]
    x: Option<x::X>,
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(windows)]
    passthrough: bool,
}

impl Overlay {
    /// Call every frame with the interactive areas in window pixels.
    pub fn update(&mut self, hit: &[Rect]) {
        let rects: Vec<[i32; 4]> = hit.iter().map(|r| [r.x as i32, r.y as i32, r.w.ceil() as i32, r.h.ceil() as i32]).collect();
        #[cfg(target_os = "linux")]
        {
            if self.x.is_none() && self.tries < 600 {
                self.tries += 1;
                self.x = x::X::find();
            }
            if let Some(x) = &self.x {
                if rects != self.last {
                    x.set_input(&rects);
                }
            }
        }
        #[cfg(windows)]
        unsafe {
            use win::*;
            if self.hwnd == 0 && self.tries < 600 {
                self.tries += 1;
                let title: Vec<u16> = TITLE.encode_utf16().chain([0]).collect();
                self.hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
                if self.hwnd != 0 {
                    SetWindowLongPtrW(self.hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);
                    SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, (WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
                    SetLayeredWindowAttributes(self.hwnd, KEY, 235, LWA_COLORKEY | LWA_ALPHA);
                    SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_NOACTIVATE);
                }
            }
            if self.hwnd != 0 {
                // Windows has no input shape for GL windows, so flip click-through on/off under the cursor.
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                ScreenToClient(self.hwnd, &mut p);
                let inside = rects.iter().any(|r| p.x >= r[0] && p.y >= r[1] && p.x < r[0] + r[2] && p.y < r[1] + r[3]);
                if inside == self.passthrough {
                    self.passthrough = !inside;
                    let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32;
                    let ex = if inside { ex & !WS_EX_TRANSPARENT } else { ex | WS_EX_TRANSPARENT };
                    SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex as isize);
                }
            }
        }
        self.last = rects;
    }
}

/// Background colour Windows treats as see-through (and click-through).
#[cfg(windows)]
pub const CLEAR: macroquad::color::Color = macroquad::color::Color::new(1.0 / 255.0, 0.0, 1.0 / 255.0, 1.0);
#[cfg(not(windows))]
pub const CLEAR: macroquad::color::Color = macroquad::color::Color::new(0.0, 0.0, 0.0, 0.0);

#[cfg(target_os = "linux")]
mod x {
    use x11rb::connection::Connection;
    use x11rb::protocol::shape::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::*;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    pub struct X {
        c: RustConnection,
        win: Window,
    }

    impl X {
        /// Works on X11 and on Wayland through XWayland (GNOME has no Wayland always-on-top).
        pub fn find() -> Option<X> {
            let (c, n) = x11rb::connect(None).ok()?;
            let root = c.setup().roots[n].root;
            let atom = |s: &str| Some(c.intern_atom(false, s.as_bytes()).ok()?.reply().ok()?.atom);
            let list = c.get_property(false, root, atom("_NET_CLIENT_LIST")?, AtomEnum::WINDOW, 0, 4096).ok()?.reply().ok()?;
            let win = list.value32()?.find(|&w| {
                c.get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
                    .ok()
                    .and_then(|r| r.reply().ok())
                    .is_some_and(|r| r.value.starts_with(super::WM_CLASS.as_bytes()))
            })?;
            // input hint = false: the WM never gives us keyboard focus
            c.change_property32(PropMode::REPLACE, win, AtomEnum::WM_HINTS, AtomEnum::WM_HINTS, &[1, 0, 0, 0, 0, 0, 0, 0, 0]).ok()?;
            let motif = atom("_MOTIF_WM_HINTS")?;
            c.change_property32(PropMode::REPLACE, win, motif, motif, &[2, 0, 0, 0, 0]).ok()?; // no decorations
            let state = atom("_NET_WM_STATE")?;
            let ev = ClientMessageEvent::new(32, win, state, [1, atom("_NET_WM_STATE_ABOVE")?, atom("_NET_WM_STATE_SKIP_TASKBAR")?, 1, 0]);
            c.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev).ok()?;
            c.flush().ok()?;
            Some(X { c, win })
        }

        pub fn set_input(&self, rects: &[[i32; 4]]) {
            let r: Vec<Rectangle> = rects
                .iter()
                .map(|r| Rectangle { x: r[0] as i16, y: r[1] as i16, width: r[2].max(0) as u16, height: r[3].max(0) as u16 })
                .collect();
            let _ = self.c.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, self.win, 0, 0, &r);
            let _ = self.c.flush();
        }
    }
}

#[cfg(windows)]
#[allow(non_snake_case, clippy::upper_case_acronyms)]
mod win {
    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }
    pub const GWL_STYLE: i32 = -16;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const WS_POPUP: u32 = 0x8000_0000;
    pub const WS_VISIBLE: u32 = 0x1000_0000;
    pub const WS_EX_LAYERED: u32 = 0x8_0000;
    pub const WS_EX_TOPMOST: u32 = 0x8;
    pub const WS_EX_TRANSPARENT: u32 = 0x20;
    pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    pub const WS_EX_TOOLWINDOW: u32 = 0x80;
    pub const HWND_TOPMOST: isize = -1;
    pub const SWP_NOSIZE: u32 = 0x1;
    pub const SWP_NOMOVE: u32 = 0x2;
    pub const SWP_NOACTIVATE: u32 = 0x10;
    pub const SWP_FRAMECHANGED: u32 = 0x20;
    pub const LWA_COLORKEY: u32 = 0x1;
    pub const LWA_ALPHA: u32 = 0x2;
    pub const KEY: u32 = 0x0001_0001; // COLORREF of overlay::CLEAR
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> isize;
        pub fn GetWindowLongPtrW(h: isize, i: i32) -> isize;
        pub fn SetWindowLongPtrW(h: isize, i: i32, v: isize) -> isize;
        pub fn SetWindowPos(h: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        pub fn SetLayeredWindowAttributes(h: isize, key: u32, alpha: u8, flags: u32) -> i32;
        pub fn GetCursorPos(p: *mut POINT) -> i32;
        pub fn ScreenToClient(h: isize, p: *mut POINT) -> i32;
    }
}
