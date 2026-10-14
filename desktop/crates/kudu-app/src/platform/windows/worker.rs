//! A thread with a Win32 message loop and a message-only window, running jobs sent from other
//! threads.
//!
//! Windows ties windows, hooks, clipboard ownership and tray icons to the thread that created
//! them, and that thread must keep pumping messages. Each `Worker` owns one such thread. Jobs
//! are queued on a channel and the thread is woken by posting a message to its window (not to
//! the thread: thread messages are dropped while a modal loop such as the tray menu runs, while
//! window messages are still dispatched).

use std::cell::{Cell, OnceCell};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread::{self, ThreadId};
use std::time::Duration;

use ::windows::core::w;
use ::windows::Win32::Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassExW, TranslateMessage,
    HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSEXW,
};
use anyhow::{anyhow, Context};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

type Job = Box<dyn FnOnce() + Send>;

/// Handles a window message on the worker's message-only window; `None` falls through to
/// `DefWindowProcW`.
pub type Handler = fn(HWND, u32, WPARAM, LPARAM) -> Option<LRESULT>;

const WM_RUN_JOBS: u32 = WM_APP + 1;

thread_local! {
    static JOBS: OnceCell<Receiver<Job>> = const { OnceCell::new() };
    static HANDLER: Cell<Option<Handler>> = const { Cell::new(None) };
}

#[derive(Clone)]
pub struct Worker {
    jobs: Sender<Job>,
    /// The message-only window (an `HWND` is a pointer, so it's kept as an integer to be Send).
    hwnd: isize,
    thread: ThreadId,
}

impl Worker {
    /// Starts the thread, creates its window and runs `init` on it before returning.
    pub fn spawn<F>(name: &str, handler: Option<Handler>, init: F) -> anyhow::Result<Self>
    where
        F: FnOnce(HWND) -> anyhow::Result<()> + Send + 'static,
    {
        let (jobs, receiver) = unbounded::<Job>();
        let (ready_tx, ready_rx) = bounded::<anyhow::Result<isize>>(1);
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                JOBS.with(|cell| {
                    let _ = cell.set(receiver);
                });
                HANDLER.with(|cell| cell.set(handler));
                let hwnd = match create_message_window() {
                    Ok(hwnd) => hwnd,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if let Err(error) = init(hwnd) {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
                let _ = ready_tx.send(Ok(hwnd.0 as isize));
                run_message_loop();
            })
            .context("starting a thread")?;
        let thread = handle.thread().id();
        let hwnd = ready_rx.recv().map_err(|_| anyhow!("the {name} thread stopped while starting"))??;
        Ok(Self { jobs, hwnd, thread })
    }

    pub fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    /// Runs `job` on the worker thread, without waiting.
    pub fn post(&self, job: impl FnOnce() + Send + 'static) {
        if self.jobs.send(Box::new(job)).is_ok() {
            // SAFETY: posting a message to a window handle is safe even if the window is gone.
            if let Err(error) = unsafe { PostMessageW(Some(self.hwnd()), WM_RUN_JOBS, WPARAM(0), LPARAM(0)) } {
                log::warn!("Couldn't wake a worker thread: {error}");
            }
        }
    }

    /// Runs `job` on the worker thread and waits up to `timeout` for its result.
    pub fn call<R: Send + 'static>(&self, timeout: Duration, job: impl FnOnce() -> R + Send + 'static) -> Option<R> {
        if thread::current().id() == self.thread {
            return Some(job());
        }
        let (tx, rx) = bounded(1);
        self.post(move || {
            let _ = tx.send(job());
        });
        match rx.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(_) => {
                log::warn!("A worker thread didn't answer within {timeout:?}");
                None
            }
        }
    }
}

fn create_message_window() -> anyhow::Result<HWND> {
    // SAFETY: plain Win32 calls with valid, 'static class and window names.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(worker_proc),
            hInstance: instance.into(),
            lpszClassName: w!("KuduWorker"),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(anyhow!("RegisterClassExW failed: {:?}", GetLastError()));
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("KuduWorker"),
            w!("Kudu"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )?;
        Ok(hwnd)
    }
}

/// Pumps messages until `WM_QUIT` (or an error).
pub fn run_message_loop() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG to fill; dispatching goes to our own window procedures.
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if result.0 == 0 || result.0 == -1 {
            break;
        }
        // SAFETY: as above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn run_jobs() {
    JOBS.with(|cell| {
        let Some(receiver) = cell.get() else { return };
        while let Ok(job) = receiver.try_recv() {
            if catch_unwind(AssertUnwindSafe(job)).is_err() {
                log::error!("A job on a worker thread panicked");
            }
        }
    });
}

unsafe extern "system" fn worker_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_RUN_JOBS {
        run_jobs();
        return LRESULT(0);
    }
    if let Some(handler) = HANDLER.with(|cell| cell.get()) {
        match catch_unwind(|| handler(hwnd, msg, wparam, lparam)) {
            Ok(Some(result)) => return result,
            Ok(None) => {}
            Err(_) => log::error!("A window message handler panicked"),
        }
    }
    // SAFETY: forwarding the unhandled message unchanged.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
