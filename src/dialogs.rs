//! Windows dialog boxes: a message, a Yes/No question and a text input box.

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        pub fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    pub const MB_OK: u32 = 0x0;
    pub const MB_YESNO: u32 = 0x4;
    pub const MB_ICONQUESTION: u32 = 0x20;
    pub const MB_ICONINFORMATION: u32 = 0x40;
    pub const MB_SETFOREGROUND: u32 = 0x1_0000;
    pub const IDYES: i32 = 6;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn message_box(text: &str, kind: u32) -> i32 {
        let (text, caption) = (wide(text), wide("Easier"));
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), kind | MB_SETFOREGROUND) }
    }
}

#[cfg(windows)]
pub fn message(text: &str) -> Result<(), String> {
    win::message_box(text, win::MB_OK | win::MB_ICONINFORMATION);
    Ok(())
}

#[cfg(windows)]
pub fn confirm(question: &str) -> Result<bool, String> {
    Ok(win::message_box(question, win::MB_YESNO | win::MB_ICONQUESTION) == win::IDYES)
}

/// A small native input box: a question, a text field, OK and Cancel.
/// Enter means OK, Escape means Cancel.
#[cfg(windows)]
mod input_box {
    #![allow(clippy::upper_case_acronyms)]
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type HWND = *mut c_void;
    type WPARAM = usize;
    type LPARAM = isize;
    type LRESULT = isize;
    type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

    #[repr(C)]
    struct WNDCLASSW {
        style: u32,
        wnd_proc: WndProc,
        cls_extra: i32,
        wnd_extra: i32,
        instance: *mut c_void,
        icon: *mut c_void,
        cursor: *mut c_void,
        background: *mut c_void,
        menu_name: *const u16,
        class_name: *const u16,
    }

    #[repr(C)]
    struct MSG {
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(class: *const WNDCLASSW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: HWND,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> HWND;
        fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
        fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
        fn IsDialogMessageW(dialog: HWND, msg: *mut MSG) -> i32;
        fn TranslateMessage(msg: *const MSG) -> i32;
        fn DispatchMessageW(msg: *const MSG) -> LRESULT;
        fn DestroyWindow(hwnd: HWND) -> i32;
        fn PostQuitMessage(code: i32);
        fn GetWindowTextW(hwnd: HWND, text: *mut u16, max: i32) -> i32;
        fn GetWindowTextLengthW(hwnd: HWND) -> i32;
        fn SendMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
        fn SetFocus(hwnd: HWND) -> HWND;
        fn SetForegroundWindow(hwnd: HWND) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetStockObject(index: i32) -> *mut c_void;
    }

    const WM_DESTROY: u32 = 0x0002;
    const WM_CLOSE: u32 = 0x0010;
    const WM_SETFONT: u32 = 0x0030;
    const WM_COMMAND: u32 = 0x0111;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_SYSMENU: u32 = 0x0008_0000;
    const WS_BORDER: u32 = 0x0080_0000;
    const WS_TABSTOP: u32 = 0x0001_0000;
    const WS_EX_DLGMODALFRAME: u32 = 0x0001;
    const WS_EX_TOPMOST: u32 = 0x0008;
    const ES_AUTOHSCROLL: u32 = 0x0080;
    const BS_DEFPUSHBUTTON: u32 = 0x0001;
    const IDOK: usize = 1;
    const IDCANCEL: usize = 2;
    const COLOR_BTNFACE: usize = 15;
    const DEFAULT_GUI_FONT: i32 = 17;
    const IDC_ARROW: usize = 32512;

    thread_local! {
        static EDIT: RefCell<HWND> = const { RefCell::new(null_mut()) };
        static ANSWER: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_COMMAND => {
                let id = wparam & 0xFFFF;
                if id == IDOK {
                    let edit = EDIT.with(|e| *e.borrow());
                    let len = GetWindowTextLengthW(edit);
                    let mut buf = vec![0u16; len as usize + 1];
                    let n = GetWindowTextW(edit, buf.as_mut_ptr(), buf.len() as i32);
                    let text = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
                    ANSWER.with(|a| *a.borrow_mut() = Some(text));
                    DestroyWindow(hwnd);
                } else if id == IDCANCEL {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    /// Shows the box and waits; `None` if the user cancelled.
    pub fn ask(question: &str) -> Option<String> {
        let class = wide("EasierInputBox");
        let (title, label) = (wide("Easier"), wide(question));
        let (static_cls, edit_cls, button_cls) = (wide("STATIC"), wide("EDIT"), wide("BUTTON"));
        let (ok, cancel, empty) = (wide("OK"), wide("Cancel"), wide(""));
        ANSWER.with(|a| *a.borrow_mut() = None);
        // SAFETY: plain Win32 calls; every string is a NUL-terminated UTF-16
        // buffer alive for the whole function, and handles come from Win32.
        unsafe {
            let instance = GetModuleHandleW(null_mut());
            let wc = WNDCLASSW {
                style: 0,
                wnd_proc,
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: null_mut(),
                cursor: LoadCursorW(null_mut(), IDC_ARROW as *const u16),
                background: (COLOR_BTNFACE + 1) as *mut c_void,
                menu_name: null_mut(),
                class_name: class.as_ptr(),
            };
            RegisterClassW(&wc); // fails harmlessly if already registered
            let (w, h) = (420, 170);
            let (x, y) = ((GetSystemMetrics(0) - w) / 2, (GetSystemMetrics(1) - h) / 2);
            let dialog = CreateWindowExW(
                WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
                class.as_ptr(),
                title.as_ptr(),
                WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                x,
                y,
                w,
                h,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if dialog.is_null() {
                return None;
            }
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let child = |cls: &[u16], text: &[u16], style: u32, x: i32, y: i32, w: i32, h: i32, id: usize| {
                let c = CreateWindowExW(
                    0,
                    cls.as_ptr(),
                    text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | style,
                    x,
                    y,
                    w,
                    h,
                    dialog,
                    id as *mut c_void,
                    instance,
                    null_mut(),
                );
                SendMessageW(c, WM_SETFONT, font as WPARAM, 1);
                c
            };
            child(&static_cls, &label, 0, 14, 14, 380, 22, 0);
            let edit = child(&edit_cls, &empty, WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL, 14, 42, 380, 24, 3);
            child(&button_cls, &ok, BS_DEFPUSHBUTTON | WS_TABSTOP, 214, 84, 86, 28, IDOK);
            child(&button_cls, &cancel, WS_TABSTOP, 308, 84, 86, 28, IDCANCEL);
            EDIT.with(|e| *e.borrow_mut() = edit);
            SetForegroundWindow(dialog);
            SetFocus(edit);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                if IsDialogMessageW(dialog, &mut msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        ANSWER.with(|a| a.borrow_mut().take())
    }
}

/// Shows an input box; returns the text typed, or "" if cancelled.
#[cfg(windows)]
pub fn ask(question: &str) -> Result<String, String> {
    Ok(input_box::ask(question).unwrap_or_default())
}

#[cfg(not(windows))]
pub fn message(_: &str) -> Result<(), String> {
    Err("dialog boxes are only available on Windows".to_string())
}

#[cfg(not(windows))]
pub fn confirm(_: &str) -> Result<bool, String> {
    Err("dialog boxes are only available on Windows".to_string())
}

#[cfg(not(windows))]
pub fn ask(_: &str) -> Result<String, String> {
    Err("dialog boxes are only available on Windows".to_string())
}
