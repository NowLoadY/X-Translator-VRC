#![cfg(windows)]

use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use std::thread;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::overlay_ipc::{OverlayEvent, OverlayState};

const WM_APP_UPDATE_STATE: u32 = WM_APP + 1;

static STATE: Mutex<Option<OverlayState>> = Mutex::new(None);

fn send_event(event: &OverlayEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("{}", json);
    }
}

pub fn run_native_overlay() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("XRTranslateNativeOverlayClass");

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            lpszClassName: class_name,
            ..Default::default()
        };

        RegisterClassExW(&wnd_class);

        // Screen positioning (Top Right default)
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let window_w = 460;
        let window_h = 360;
        let x = screen_w - window_w - 40;
        let y = 60;

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            w!("XRTranslate Overlay"),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            window_w,
            window_h,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )
        .unwrap();

        // Initialize layered window attributes so Windows displays the alpha layer
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

        // Enable 100% per-pixel DWM transparency
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Spawn Stdin reader thread
        let hwnd_raw = hwnd.0 as usize;
        thread::spawn(move || {
            let hwnd = HWND(hwnd_raw as *mut _);
            let stdin = std::io::stdin();
            let reader = BufReader::new(stdin.lock());
            for line in reader.lines().flatten() {
                if let Ok(new_state) = serde_json::from_str::<OverlayState>(&line) {
                    if let Ok(mut state_guard) = STATE.lock() {
                        *state_guard = Some(new_state);
                    }
                    let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_STATE, WPARAM(0), LPARAM(0));
                }
            }
            // Stdin closed -> exit overlay
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        });

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

struct RenderResources {
    d2d_factory: ID2D1Factory,
    dwrite_factory: IDWriteFactory,
    render_target: Option<ID2D1HwndRenderTarget>,
    brush_header_bg: Option<ID2D1SolidColorBrush>,
    brush_card_bg: Option<ID2D1SolidColorBrush>,
    brush_live_bg: Option<ID2D1SolidColorBrush>,
    brush_text_white: Option<ID2D1SolidColorBrush>,
    brush_text_gray: Option<ID2D1SolidColorBrush>,
    brush_text_sub: Option<ID2D1SolidColorBrush>,
    brush_btn_bg: Option<ID2D1SolidColorBrush>,
    text_format_title: Option<IDWriteTextFormat>,
    text_format_body: Option<IDWriteTextFormat>,
    text_format_sub: Option<IDWriteTextFormat>,
}

impl RenderResources {
    fn new() -> Result<Self> {
        unsafe {
            let d2d_factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            Ok(Self {
                d2d_factory,
                dwrite_factory,
                render_target: None,
                brush_header_bg: None,
                brush_card_bg: None,
                brush_live_bg: None,
                brush_text_white: None,
                brush_text_gray: None,
                brush_text_sub: None,
                brush_btn_bg: None,
                text_format_title: None,
                text_format_body: None,
                text_format_sub: None,
            })
        }
    }

    fn ensure_target(&mut self, hwnd: HWND) -> Result<()> {
        unsafe {
            if self.render_target.is_some() {
                return Ok(());
            }

            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let size = D2D_SIZE_U {
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            };

            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let hwnd_rt_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };

            let target = self
                .d2d_factory
                .CreateHwndRenderTarget(&rt_props, &hwnd_rt_props)?;

            // Brushes
            let brush_header_bg = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.08,
                    g: 0.08,
                    b: 0.10,
                    a: 0.85,
                },
                None,
            )?;
            let brush_card_bg = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.10,
                    g: 0.11,
                    b: 0.14,
                    a: 0.85,
                },
                None,
            )?;
            let brush_live_bg = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.12,
                    g: 0.28,
                    b: 0.55,
                    a: 0.90,
                },
                None,
            )?;
            let brush_text_white = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            )?;
            let brush_text_gray = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.70,
                    g: 0.73,
                    b: 0.78,
                    a: 1.0,
                },
                None,
            )?;
            let brush_text_sub = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.50,
                    g: 0.53,
                    b: 0.58,
                    a: 1.0,
                },
                None,
            )?;
            let brush_btn_bg = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.20,
                    g: 0.22,
                    b: 0.26,
                    a: 0.80,
                },
                None,
            )?;

            // Text Formats
            let font_name = w!("Microsoft YaHei");
            let text_format_title = self.dwrite_factory.CreateTextFormat(
                font_name,
                None,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.0,
                w!("zh-cn"),
            )?;

            let text_format_body = self.dwrite_factory.CreateTextFormat(
                font_name,
                None,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                15.0,
                w!("zh-cn"),
            )?;

            let text_format_sub = self.dwrite_factory.CreateTextFormat(
                font_name,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                11.0,
                w!("zh-cn"),
            )?;

            self.render_target = Some(target);
            self.brush_header_bg = Some(brush_header_bg);
            self.brush_card_bg = Some(brush_card_bg);
            self.brush_live_bg = Some(brush_live_bg);
            self.brush_text_white = Some(brush_text_white);
            self.brush_text_gray = Some(brush_text_gray);
            self.brush_text_sub = Some(brush_text_sub);
            self.brush_btn_bg = Some(brush_btn_bg);
            self.text_format_title = Some(text_format_title);
            self.text_format_body = Some(text_format_body);
            self.text_format_sub = Some(text_format_sub);

            Ok(())
        }
    }
}

static RESOURCES: Mutex<Option<RenderResources>> = Mutex::new(None);

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            if let Ok(res) = RenderResources::new() {
                if let Ok(mut guard) = RESOURCES.lock() {
                    *guard = Some(res);
                }
            }
            LRESULT(0)
        }
        WM_SIZE => {
            if let Ok(mut guard) = RESOURCES.lock() {
                if let Some(res) = guard.as_mut() {
                    if let Some(target) = &res.render_target {
                        let width = (lparam.0 & 0xFFFF) as u32;
                        let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                        let _ = unsafe { target.Resize(&D2D_SIZE_U { width, height }) };
                    }
                }
            }
            LRESULT(0)
        }
        WM_APP_UPDATE_STATE => {
            unsafe {
                adjust_window_height_if_needed(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            draw_overlay(hwnd);
            unsafe {
                let _ = ValidateRect(Some(hwnd), None);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            handle_click(hwnd, x, y);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            unsafe {
                let res = DefWindowProcW(hwnd, msg, wparam, lparam);
                if res == LRESULT(HTCLIENT as isize) {
                    let mut point = POINT {
                        x: (lparam.0 & 0xFFFF) as i16 as i32,
                        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
                    };
                    let _ = ScreenToClient(hwnd, &mut point);
                    // Top bar drag zone (x: 0..280, y: 0..32)
                    if point.y >= 0 && point.y <= 32 && point.x >= 0 && point.x <= 280 {
                        return LRESULT(HTCAPTION as isize);
                    }
                }
                res
            }
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn handle_click(hwnd: HWND, x: f32, y: f32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    let width = (rect.right - rect.left) as f32;

    // Header buttons (Y: 4..28)
    if y >= 4.0 && y <= 28.0 {
        // Close button (Top right: width-28 .. width-8)
        if x >= width - 28.0 && x <= width - 8.0 {
            send_event(&OverlayEvent::CloseRequested);
            unsafe {
                DestroyWindow(hwnd).ok();
            }
            return;
        }

        // Plus button (Top right: width-60 .. width-40)
        if x >= width - 60.0 && x <= width - 40.0 {
            if let Ok(state_guard) = STATE.lock() {
                if let Some(state) = state_guard.as_ref() {
                    if state.max_items < 10 {
                        send_event(&OverlayEvent::MaxCountChanged(state.max_items + 1));
                    }
                }
            }
            return;
        }

        // Minus button (Top right: width-110 .. width-90)
        if x >= width - 110.0 && x <= width - 90.0 {
            if let Ok(state_guard) = STATE.lock() {
                if let Some(state) = state_guard.as_ref() {
                    if state.max_items > 1 {
                        send_event(&OverlayEvent::MaxCountChanged(state.max_items - 1));
                    }
                }
            }
            return;
        }
    }
}

fn measure_text_height(
    factory: &IDWriteFactory,
    text: &str,
    format: &IDWriteTextFormat,
    max_width: f32,
) -> f32 {
    unsafe {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        if utf16.is_empty() {
            return 0.0;
        }
        if let Ok(layout) = factory.CreateTextLayout(&utf16, format, max_width, 2000.0) {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_ok() {
                return metrics.height.max(12.0);
            }
        }
        0.0
    }
}

fn adjust_window_height_if_needed(hwnd: HWND) {
    let required_h = unsafe {
        let guard = match RESOURCES.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let res = match guard.as_ref() {
            Some(r) => r,
            None => return,
        };
        let state_opt = STATE.lock().ok().and_then(|s| s.clone());
        let Some(state) = state_opt else { return };

        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return;
        }
        let width = (rect.right - rect.left) as f32;
        let padding_x = 12.0;
        let max_text_w = (width - padding_x * 2.0).max(50.0);

        let mut curr_y = 36.0;
        if state.visible_entries.is_empty() && state.partial_text.is_none() {
            curr_y += 36.0 + 6.0;
        } else {
            for (src, translated) in &state.visible_entries {
                let h_src = if let Some(format) = &res.text_format_sub {
                    measure_text_height(&res.dwrite_factory, src, format, max_text_w)
                } else {
                    0.0
                };
                let h_trans = if let Some(format) = &res.text_format_body {
                    measure_text_height(&res.dwrite_factory, translated, format, max_text_w)
                } else {
                    0.0
                };
                let top_pad = 8.0;
                let spacing = if h_src > 0.0 && h_trans > 0.0 {
                    4.0
                } else {
                    0.0
                };
                let bot_pad = 8.0;
                let card_h = top_pad + h_src + spacing + h_trans + bot_pad;
                curr_y += card_h + 6.0;
            }

            if let Some(partial) = &state.partial_text {
                let h_partial = if let Some(format) = &res.text_format_body {
                    measure_text_height(&res.dwrite_factory, partial, format, max_text_w)
                } else {
                    20.0
                };
                let card_h = 8.0 + h_partial + 8.0;
                curr_y += card_h + 6.0;
            }
        }

        ((curr_y + 8.0) as i32).max(120).min(900)
    };

    unsafe {
        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_ok() {
            let current_h = window_rect.bottom - window_rect.top;
            if (current_h - required_h).abs() > 4 {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    window_rect.right - window_rect.left,
                    required_h,
                    SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }
}

fn measure_and_create_layout(
    factory: &IDWriteFactory,
    text: &str,
    format: &IDWriteTextFormat,
    max_width: f32,
) -> Option<(IDWriteTextLayout, f32, f32)> {
    unsafe {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        if utf16.is_empty() {
            return None;
        }
        let layout = factory
            .CreateTextLayout(&utf16, format, max_width, 2000.0)
            .ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        if layout.GetMetrics(&mut metrics).is_ok() {
            let width = metrics.width.max(10.0);
            let height = metrics.height.max(12.0);
            Some((layout, width, height))
        } else {
            None
        }
    }
}

fn draw_overlay(hwnd: HWND) {
    unsafe {
        let mut guard = match RESOURCES.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        let res = match guard.as_mut() {
            Some(r) => r,
            None => return,
        };

        if res.ensure_target(hwnd).is_err() {
            return;
        }

        let target = match &res.render_target {
            Some(t) => t,
            None => return,
        };

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let width = (rect.right - rect.left) as f32;

        target.BeginDraw();
        target.Clear(Some(&D2D1_COLOR_F {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }));

        let state_opt = STATE.lock().ok().and_then(|s| s.clone());

        // 1. Slim Header Bar
        let header_rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: width,
                bottom: 30.0,
            },
            radiusX: 6.0,
            radiusY: 6.0,
        };

        if let Some(brush) = &res.brush_header_bg {
            target.FillRoundedRectangle(&header_rect, brush);
        }

        // Header Title
        let max_items = state_opt.as_ref().map(|s| s.max_items).unwrap_or(5);
        let count = state_opt
            .as_ref()
            .map(|s| s.visible_entries.len())
            .unwrap_or(0);
        let title = format!("≡ Subtitles · {count}");
        let title_utf16: Vec<u16> = title.encode_utf16().collect();

        if let (Some(format), Some(brush)) = (&res.text_format_title, &res.brush_text_white) {
            target.DrawText(
                &title_utf16,
                format,
                &D2D_RECT_F {
                    left: 10.0,
                    top: 6.0,
                    right: 200.0,
                    bottom: 26.0,
                },
                brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }

        // Header Controls: [ - ] N [ + ] [ × ]
        // Close button
        if let (Some(brush_btn), Some(brush_txt), Some(format)) = (
            &res.brush_btn_bg,
            &res.brush_text_white,
            &res.text_format_title,
        ) {
            let close_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: width - 28.0,
                    top: 4.0,
                    right: width - 8.0,
                    bottom: 26.0,
                },
                radiusX: 4.0,
                radiusY: 4.0,
            };
            target.FillRoundedRectangle(&close_rect, brush_btn);
            let close_str: Vec<u16> = "×".encode_utf16().collect();
            target.DrawText(
                &close_str,
                format,
                &D2D_RECT_F {
                    left: width - 22.0,
                    top: 5.0,
                    right: width - 8.0,
                    bottom: 26.0,
                },
                brush_txt,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // Plus button
            let plus_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: width - 60.0,
                    top: 4.0,
                    right: width - 40.0,
                    bottom: 26.0,
                },
                radiusX: 4.0,
                radiusY: 4.0,
            };
            target.FillRoundedRectangle(&plus_rect, brush_btn);
            let plus_str: Vec<u16> = "+".encode_utf16().collect();
            target.DrawText(
                &plus_str,
                format,
                &D2D_RECT_F {
                    left: width - 54.0,
                    top: 5.0,
                    right: width - 40.0,
                    bottom: 26.0,
                },
                brush_txt,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // Max count number
            let num_str: Vec<u16> = format!("{}", max_items).encode_utf16().collect();
            target.DrawText(
                &num_str,
                format,
                &D2D_RECT_F {
                    left: width - 82.0,
                    top: 6.0,
                    right: width - 64.0,
                    bottom: 26.0,
                },
                brush_txt,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // Minus button
            let minus_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: width - 110.0,
                    top: 4.0,
                    right: width - 90.0,
                    bottom: 26.0,
                },
                radiusX: 4.0,
                radiusY: 4.0,
            };
            target.FillRoundedRectangle(&minus_rect, brush_btn);
            let minus_str: Vec<u16> = "-".encode_utf16().collect();
            target.DrawText(
                &minus_str,
                format,
                &D2D_RECT_F {
                    left: width - 104.0,
                    top: 5.0,
                    right: width - 90.0,
                    bottom: 26.0,
                },
                brush_txt,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }

        // 2. Render Subtitle Cards with Smart Dynamic Auto-Sizing
        let mut curr_y = 36.0;
        let padding_x = 12.0;
        let max_text_w = (width - padding_x * 2.0).max(50.0);

        if let Some(state) = state_opt {
            if state.visible_entries.is_empty() && state.partial_text.is_none() {
                // Empty placeholder card
                let card_h = 36.0;
                let card_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: curr_y,
                        right: width,
                        bottom: curr_y + card_h,
                    },
                    radiusX: 8.0,
                    radiusY: 8.0,
                };
                if let Some(brush) = &res.brush_card_bg {
                    target.FillRoundedRectangle(&card_rect, brush);
                }
                if let (Some(brush_txt), Some(format)) = (&res.brush_text_sub, &res.text_format_sub)
                {
                    let waiting_str: Vec<u16> = "Listening".encode_utf16().collect();
                    target.DrawText(
                        &waiting_str,
                        format,
                        &D2D_RECT_F {
                            left: padding_x,
                            top: curr_y + 10.0,
                            right: width - padding_x,
                            bottom: curr_y + 30.0,
                        },
                        brush_txt,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
                let _ = card_h;
            } else {
                // Render finished history items with dynamic DirectWrite height calculation
                for (src, translated) in &state.visible_entries {
                    let layout_src = if let Some(format) = &res.text_format_sub {
                        measure_and_create_layout(&res.dwrite_factory, src, format, max_text_w)
                    } else {
                        None
                    };

                    let layout_trans = if let Some(format) = &res.text_format_body {
                        measure_and_create_layout(
                            &res.dwrite_factory,
                            translated,
                            format,
                            max_text_w,
                        )
                    } else {
                        None
                    };

                    let h_src = layout_src.as_ref().map(|(_, _, h)| *h).unwrap_or(0.0);
                    let h_trans = layout_trans.as_ref().map(|(_, _, h)| *h).unwrap_or(0.0);

                    let top_pad = 8.0;
                    let spacing = if h_src > 0.0 && h_trans > 0.0 {
                        4.0
                    } else {
                        0.0
                    };
                    let bot_pad = 8.0;
                    let card_h = top_pad + h_src + spacing + h_trans + bot_pad;

                    let card_rect = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: 0.0,
                            top: curr_y,
                            right: width,
                            bottom: curr_y + card_h,
                        },
                        radiusX: 8.0,
                        radiusY: 8.0,
                    };

                    if let Some(brush) = &res.brush_card_bg {
                        target.FillRoundedRectangle(&card_rect, brush);
                    }

                    // Render source text layout
                    let mut text_y = curr_y + top_pad;
                    if let (Some((layout, _, _)), Some(brush)) = (&layout_src, &res.brush_text_gray)
                    {
                        target.DrawTextLayout(
                            D2D_POINT_2F {
                                x: padding_x,
                                y: text_y,
                            },
                            layout,
                            brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                        text_y += h_src + spacing;
                    }

                    // Render translation text layout
                    if let (Some((layout, _, _)), Some(brush)) =
                        (&layout_trans, &res.brush_text_white)
                    {
                        target.DrawTextLayout(
                            D2D_POINT_2F {
                                x: padding_x,
                                y: text_y,
                            },
                            layout,
                            brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }

                    curr_y += card_h + 6.0;
                }

                // Render live streaming partial text with dynamic height
                if let Some(partial) = &state.partial_text {
                    let layout_partial = if let Some(format) = &res.text_format_body {
                        measure_and_create_layout(&res.dwrite_factory, partial, format, max_text_w)
                    } else {
                        None
                    };

                    let h_partial = layout_partial.as_ref().map(|(_, _, h)| *h).unwrap_or(20.0);
                    let card_h = 8.0 + h_partial + 8.0;

                    let card_rect = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: 0.0,
                            top: curr_y,
                            right: width,
                            bottom: curr_y + card_h,
                        },
                        radiusX: 8.0,
                        radiusY: 8.0,
                    };

                    if let Some(brush) = &res.brush_live_bg {
                        target.FillRoundedRectangle(&card_rect, brush);
                    }

                    if let (Some((layout, _, _)), Some(brush)) =
                        (&layout_partial, &res.brush_text_white)
                    {
                        target.DrawTextLayout(
                            D2D_POINT_2F {
                                x: padding_x,
                                y: curr_y + 8.0,
                            },
                            layout,
                            brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }

                    let _ = card_h;
                }
            }
        }

        let _ = target.EndDraw(None, None);
    }
}
