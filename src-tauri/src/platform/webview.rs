//! WebView2 runtime check (spec 3.1): the fixed-version runtime is NOT bundled, so if the
//! system runtime is missing we explain it in one friendly dialog and offer Microsoft's
//! installer page, instead of failing silently.

/// True if WebView2 is installed; otherwise explains, offers the download page and returns false.
#[cfg(windows)]
pub fn ensure_webview2() -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_YESNO, MessageBoxW, SW_SHOWNORMAL};
    use windows::core::{PCWSTR, PWSTR, w};

    let mut version = PWSTR::null();
    let ok = unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) }.is_ok() && !version.is_null();
    if ok {
        return true;
    }
    let text = w!("Metadata Copier and Editor needs Microsoft Edge WebView2, a free part of Windows that draws the app's window.\n\n\
         It's built into Windows 11 and up-to-date Windows 10, but it seems to be missing on this PC.\n\n\
         Open Microsoft's download page now? Install the \"Evergreen Bootstrapper\", then start the app again.");
    let answer = unsafe { MessageBoxW(None, text, w!("One quick thing before we start"), MB_YESNO | MB_ICONINFORMATION) };
    if answer == IDYES {
        unsafe {
            ShellExecuteW(None, w!("open"), w!("https://developer.microsoft.com/microsoft-edge/webview2/"), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
    }
    false
}

#[cfg(not(windows))]
pub fn ensure_webview2() -> bool {
    true
}
