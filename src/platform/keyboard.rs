//! Preserve keypad operator identity before GPUI translates queued key messages.
use gpui::Keystroke;
use std::cell::Cell;
use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{VK_ADD, VK_SUBTRACT},
        WindowsAndMessaging::{
            CallNextHookEx, HHOOK, MSG, PM_REMOVE, SetWindowsHookExW, UnhookWindowsHookEx,
            WH_GETMESSAGE, WM_KEYDOWN, WM_SYSKEYDOWN,
        },
    },
};

thread_local! {
    static VIRTUAL_KEY: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(crate) struct MessageHook(HHOOK);

impl Drop for MessageHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

pub(crate) fn install() -> anyhow::Result<MessageHook> {
    // GPUI handles keydown in its message loop, before DispatchMessage reaches
    // a window subclass. Observe only this application's UI thread queue.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_GETMESSAGE,
            Some(key_message),
            std::ptr::null_mut(),
            GetCurrentThreadId(),
        )
    };
    anyhow::ensure!(!hook.is_null(), "키보드 입력 구분을 초기화하지 못했습니다.");
    Ok(MessageHook(hook))
}

pub(crate) fn shortcut_keystroke(key: &Keystroke) -> Keystroke {
    let mut key = key.clone();
    let name = match VIRTUAL_KEY.take() {
        Some(vk) if vk == VK_ADD as usize => "numpad_plus",
        Some(vk) if vk == VK_SUBTRACT as usize => "numpad_minus",
        _ => return key,
    };
    key.key = name.into();
    key.key_char = None;
    key
}

unsafe extern "system" fn key_message(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && wparam == PM_REMOVE as usize {
        let message = unsafe { &*(lparam as *const MSG) };
        let key = matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN).then_some(message.wParam);
        VIRTUAL_KEY.set(key);
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{VK_OEM_MINUS, VK_OEM_PLUS},
        WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE, PeekMessageW, PostMessageW, WM_APP,
        },
    };

    #[test]
    fn queued_key_messages_preserve_operator_identity_without_stale_state() {
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            let _hook = install().unwrap();
            for message in [WM_KEYDOWN, WM_SYSKEYDOWN] {
                for (vk, symbol, expected) in [
                    (VK_ADD, "ctrl-+", "numpad_plus"),
                    (VK_OEM_PLUS, "ctrl-+", "+"),
                    (VK_SUBTRACT, "ctrl--", "numpad_minus"),
                    (VK_OEM_MINUS, "ctrl--", "-"),
                ] {
                    assert_ne!(PostMessageW(hwnd, message, vk as usize, 0), 0);
                    let mut event: MSG = std::mem::zeroed();
                    assert_ne!(
                        PeekMessageW(&mut event, hwnd, message, message, PM_REMOVE),
                        0
                    );
                    let key = shortcut_keystroke(&Keystroke::parse(symbol).unwrap());
                    assert_eq!(key.key, expected);
                    assert!(key.modifiers.control);
                    assert_eq!(
                        shortcut_keystroke(&Keystroke::parse("ctrl-+").unwrap()).key,
                        "+"
                    );
                }
            }
            PostMessageW(hwnd, WM_APP, VK_ADD as usize, 0);
            let mut event: MSG = std::mem::zeroed();
            assert_ne!(PeekMessageW(&mut event, hwnd, WM_APP, WM_APP, PM_REMOVE), 0);
            assert_eq!(
                shortcut_keystroke(&Keystroke::parse("ctrl-+").unwrap()).key,
                "+"
            );
            assert_ne!(DestroyWindow(hwnd), 0);
        }
    }
}
