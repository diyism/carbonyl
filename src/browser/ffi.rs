use std::ffi::CStr;
use std::io::{stderr, Write};
use std::process::{Command, Stdio};
use std::{env, io};

use libc::{c_char, c_int, c_uchar, c_uint, size_t};

use crate::gfx::{Cast, Color, Point, Rect, Size};
use crate::terminal::output::Renderer;
use crate::terminal::{input, output};

/// This file bridges the C++ code with Rust.
/// "C-unwind" combined with .unwrap() is used to allow catching Rust panics
/// using C++ exception handling.

#[repr(C)]
pub struct CSize {
    width: c_uint,
    height: c_uint,
}
#[repr(C)]
pub struct CPoint {
    x: c_uint,
    y: c_uint,
}
#[repr(C)]
pub struct CRect {
    origin: CPoint,
    size: CSize,
}
#[repr(C)]
pub struct CColor {
    r: u8,
    g: u8,
    b: u8,
}

impl<T: Copy> From<&CPoint> for Point<T>
where
    c_uint: Cast<T>,
{
    fn from(value: &CPoint) -> Self {
        Point::new(value.x, value.y).cast()
    }
}
impl<T: Copy> From<&CSize> for Size<T>
where
    c_uint: Cast<T>,
{
    fn from(value: &CSize) -> Self {
        Size::new(value.width, value.height).cast()
    }
}

#[repr(C)]
pub struct BrowserDelegate {
    shutdown: extern "C" fn(),
    scroll: extern "C" fn(c_int),
    key_press: extern "C" fn(c_char),
    mouse_up: extern "C" fn(c_uint, c_uint),
    mouse_down: extern "C" fn(c_uint, c_uint),
    mouse_move: extern "C" fn(c_uint, c_uint),
}

fn main() -> io::Result<Option<i32>> {
    const CARBONYL_INSIDE_SHELL: &str = "CARBONYL_INSIDE_SHELL";

    if env::vars().find(|(key, value)| key == CARBONYL_INSIDE_SHELL && value == "1") != None {
        return Ok(None);
    }

    let mut terminal = input::Terminal::setup();
    let output = Command::new(env::current_exe()?)
        .args(env::args().skip(1))
        .arg("--disable-threaded-scrolling")
        .arg("--disable-threaded-animation")
        .env(CARBONYL_INSIDE_SHELL, "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .output()?;

    terminal.teardown();
    stderr().write_all(&output.stderr)?;

    let code = output.status.code();

    Ok(if code == None { Some(127) } else { code })
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_shell_main() {
    if let Some(code) = main().unwrap() {
        std::process::exit(code)
    }
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_renderer_create() -> *mut Renderer {
    let mut renderer = Box::new(Renderer::new());
    let src = output::size().unwrap();

    renderer.set_size(Size::new(7, 14), src);

    Box::into_raw(renderer)
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_renderer_clear_text(renderer: *mut Renderer) {
    let renderer = unsafe { &mut *renderer };

    renderer.clear_text()
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_renderer_draw_text(
    renderer: *mut Renderer,
    utf8: *const c_char,
    rect: *const CRect,
    color: *const CColor,
) {
    let (renderer, string, rect, color) =
        unsafe { (&mut *renderer, CStr::from_ptr(utf8), &*rect, &*color) };

    renderer.draw_text(
        string.to_str().unwrap(),
        Point::from(&rect.origin),
        Size::from(&rect.size),
        Color::new(color.r, color.g, color.b),
    )
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_renderer_draw_background(
    renderer: *mut Renderer,
    pixels: *mut c_uchar,
    pixels_size: size_t,
    rect: *const CRect,
) {
    let (renderer, pixels, rect) = unsafe {
        (
            &mut *renderer,
            std::slice::from_raw_parts_mut(pixels, pixels_size),
            &*rect,
        )
    };

    renderer
        .draw_background(
            pixels,
            Rect {
                origin: Point::from(&rect.origin),
                size: Size::from(&rect.size),
            },
        )
        .unwrap()
}

#[no_mangle]
pub extern "C-unwind" fn carbonyl_output_get_size(size: *mut CSize) {
    let dst = unsafe { &mut *size };
    let src = output::size().unwrap().cast::<c_uint>();

    dst.width = src.width * 7;
    dst.height = src.height * 14;
}

/// Function called by the C++ code to listen for input events.
///
/// This will block so the calling code should start and own a dedicated thread.
/// It will panic if there is any error.
#[no_mangle]
pub extern "C-unwind" fn carbonyl_input_listen(
    renderer: *mut Renderer,
    delegate: *mut BrowserDelegate,
) {
    let char_width = 7;
    let char_height = 14;
    let (
        renderer,
        BrowserDelegate {
            shutdown,
            scroll,
            key_press,
            mouse_up,
            mouse_down,
            mouse_move,
        },
    ) = unsafe { (&mut *renderer, &*delegate) };

    use input::*;

    listen(|event| {
        use Event::*;

        match event {
            Exit => return Some(shutdown()),
            KeyPress { key } => key_press(key as c_char),
            Scroll { delta } => scroll(delta as c_int * char_height as c_int),
            MouseUp { col, row } => {
                mouse_up(col as c_uint * char_width, row as c_uint * char_height)
            }
            MouseDown { col, row } => {
                mouse_down(col as c_uint * char_width, row as c_uint * char_height)
            }
            MouseMove { col, row } => {
                mouse_move(col as c_uint * char_width, row as c_uint * char_height)
            }
            Terminal(terminal) => match terminal {
                TerminalEvent::Name(name) => eprintln!("Terminal name: {name}"),
                TerminalEvent::TrueColorSupported => renderer.enable_true_color(),
            },
        }

        None
    })
    .unwrap()
}
