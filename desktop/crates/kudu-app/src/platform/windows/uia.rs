//! Reading text from other apps through UI Automation.
//!
//! UI Automation calls go into the target app's process and wait for it; a hung app would stall
//! the caller. So all calls run on one worker thread (in the multithreaded COM apartment, which
//! needs no message loop), with UI Automation's own timeouts set, and callers wait for at most
//! a short time.

use std::collections::{HashSet, VecDeque};
use std::thread;
use std::time::{Duration, Instant};

use ::windows::core::{Interface, BSTR};
use ::windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, UIA_ControlTypePropertyId, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId, UIA_NamePropertyId, UIA_TextPatternId,
    UIA_ValuePatternId,
};
use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use crossbeam_channel::{bounded, Sender, TrySendError};

/// Most text taken from the focused field's document.
const FIELD_TEXT_LIMIT: i32 = 20_000;
/// Characters taken before the cursor.
const BEFORE_CURSOR: i32 = 300;
/// Most elements visited for the screen's text.
const SCREEN_ELEMENTS: usize = 400;
/// Most characters kept per element's text.
const ELEMENT_TEXT_LIMIT: usize = 4_000;

type Job = Box<dyn FnOnce(Option<&Context>) + Send>;

struct Context {
    automation: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    cache: IUIAutomationCacheRequest,
}

pub struct Uia {
    jobs: Sender<Job>,
}

impl Uia {
    pub fn spawn() -> anyhow::Result<Self> {
        // A few queued calls at most: if the thread is stuck on a hung app, further calls fail
        // fast instead of piling up.
        let (jobs, receiver) = bounded::<Job>(4);
        thread::Builder::new().name("kudu-automation".into()).spawn(move || {
            let context = match Context::new() {
                Ok(context) => Some(context),
                Err(error) => {
                    log::warn!("UI Automation is unavailable: {error}");
                    None
                }
            };
            for job in receiver {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(context.as_ref()))).is_err() {
                    log::error!("A UI Automation call panicked");
                }
            }
        })?;
        Ok(Self { jobs })
    }

    fn call<R: Send + 'static>(
        &self,
        timeout: Duration,
        f: impl FnOnce(&Context) -> Option<R> + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = bounded(1);
        let job: Job = Box::new(move |context| {
            let _ = tx.send(context.and_then(f));
        });
        match self.jobs.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                log::debug!("UI Automation is busy (a previous call is stuck)");
                return None;
            }
            Err(TrySendError::Disconnected(_)) => return None,
        }
        rx.recv_timeout(timeout).ok().flatten()
    }

    pub fn focused_field_text(&self) -> Option<String> {
        self.call(Duration::from_millis(500), |context| context.focused_field_text())
    }

    pub fn text_before_cursor(&self) -> Option<String> {
        self.call(Duration::from_millis(300), |context| context.text_before_cursor())
    }

    pub fn screen_text(&self, budget: Duration) -> Vec<String> {
        let deadline = Instant::now() + budget;
        self.call(budget + Duration::from_millis(100), move |context| Some(context.screen_text(deadline)))
            .unwrap_or_default()
    }
}

impl Context {
    fn new() -> ::windows::core::Result<Self> {
        // SAFETY: COM initialization for this thread, and plain COM calls on the objects made.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .or_else(|_| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER))?;
            // Bound how long a call into an unresponsive app may take (Windows 8+).
            if let Ok(automation2) = automation.cast::<IUIAutomation2>() {
                let _ = automation2.SetConnectionTimeout(1_000);
                let _ = automation2.SetTransactionTimeout(1_500);
            }
            let walker = automation.ControlViewWalker()?;
            let cache = automation.CreateCacheRequest()?;
            for property in
                [UIA_NamePropertyId, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId]
            {
                cache.AddProperty(property)?;
            }
            Ok(Self { automation, walker, cache })
        }
    }

    /// The focused element, unless it's a password field.
    fn focused(&self) -> Option<IUIAutomationElement> {
        // SAFETY: plain COM calls.
        unsafe {
            let element = self.automation.GetFocusedElement().ok()?;
            if element.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false) {
                return None;
            }
            Some(element)
        }
    }

    fn focused_field_text(&self) -> Option<String> {
        let element = self.focused()?;
        // SAFETY: plain COM calls.
        unsafe {
            if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                if let Ok(value) = pattern.CurrentValue() {
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
            let pattern = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId).ok()?;
            let text = pattern.DocumentRange().ok()?.GetText(FIELD_TEXT_LIMIT).ok()?;
            Some(text.to_string())
        }
    }

    fn text_before_cursor(&self) -> Option<String> {
        let element = self.focused()?;
        // SAFETY: plain COM calls.
        unsafe {
            let pattern = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId).ok()?;
            let selection = pattern.GetSelection().ok()?;
            if selection.Length().ok()? < 1 {
                return None;
            }
            let selected = selection.GetElement(0).ok()?;
            let range = selected.Clone().ok()?;
            // Collapse to the start of the selection (the cursor), then reach back.
            range.MoveEndpointByRange(TextPatternRangeEndpoint_End, &selected, TextPatternRangeEndpoint_Start).ok()?;
            range.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -BEFORE_CURSOR).ok()?;
            let text = range.GetText(BEFORE_CURSOR + 100).ok()?;
            Some(text.to_string())
        }
    }

    /// Names and field contents of the foreground window's visible elements, breadth-first.
    fn screen_text(&self, deadline: Instant) -> Vec<String> {
        let mut texts = Vec::new();
        let mut seen = HashSet::new();
        // SAFETY: plain query and COM calls.
        let root = unsafe {
            let window = GetForegroundWindow();
            if window.is_invalid() {
                return texts;
            }
            match self.automation.ElementFromHandle(window).and_then(|e| e.BuildUpdatedCache(&self.cache)) {
                Ok(root) => root,
                Err(_) => return texts,
            }
        };
        let mut queue = VecDeque::from([root]);
        let mut visited = 0;
        let mut add = |text: String| {
            let text = clip(text.trim());
            if text.chars().count() >= 2 && seen.insert(text.clone()) {
                texts.push(text);
            }
        };
        while let Some(element) = queue.pop_front() {
            if visited >= SCREEN_ELEMENTS || Instant::now() >= deadline {
                break;
            }
            visited += 1;
            // SAFETY: plain COM calls; cached values come from the cache request.
            unsafe {
                if visited > 1 && element.CachedIsOffscreen().map(|b| b.as_bool()).unwrap_or(false) {
                    continue; // Hidden, and so is everything inside it.
                }
                if let Ok(name) = element.CachedName() {
                    add(name.to_string());
                }
                let control = element.CachedControlType().unwrap_or_default();
                let password = element.CachedIsPassword().map(|b| b.as_bool()).unwrap_or(false);
                if (control == UIA_EditControlTypeId || control == UIA_DocumentControlTypeId) && !password {
                    if let Some(text) = element_text(&element) {
                        add(text);
                    }
                }
                let mut child = self.walker.GetFirstChildElementBuildCache(&element, &self.cache);
                while let Ok(current) = child {
                    if queue.len() + visited >= SCREEN_ELEMENTS {
                        break;
                    }
                    child = self.walker.GetNextSiblingElementBuildCache(&current, &self.cache);
                    queue.push_back(current);
                }
            }
        }
        texts
    }
}

/// A field's text, bounded: through its text pattern (only the first part is transferred),
/// else its value.
fn element_text(element: &IUIAutomationElement) -> Option<String> {
    // SAFETY: plain COM calls.
    unsafe {
        if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            if let Ok(text) = pattern.DocumentRange().and_then(|range| range.GetText(ELEMENT_TEXT_LIMIT as i32)) {
                return Some(text.to_string());
            }
        }
        let pattern = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId).ok()?;
        let value: BSTR = pattern.CurrentValue().ok()?;
        Some(value.to_string())
    }
}

fn clip(text: &str) -> String {
    match text.char_indices().nth(ELEMENT_TEXT_LIMIT) {
        Some((end, _)) => text[..end].to_string(),
        None => text.to_string(),
    }
}
