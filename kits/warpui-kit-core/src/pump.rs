//! The net-event pump both store-driven apps run: a background thread sends
//! events over an [`async_channel`], and this loop drains them onto the main
//! thread. It was copy-pasted between the two mains — including the caveat
//! comment below; now there is one copy.
//!
//! Polled with a timer fallback: this gpui rev does not service cross-thread
//! channel wakeups on foreground tasks, so a plain `recv().await` would sleep
//! forever (async_channel recv is cancel-safe, re-creating the future each
//! lap is fine). A wake drains the whole pending burst in one pass — one
//! timer and one frame per burst instead of one per event.

use gpui::AsyncApp;

/// The timer fallback cadence.
const POLL: std::time::Duration = std::time::Duration::from_millis(200);
/// The largest burst handled per wake; a longer queue waits for the next lap.
const BURST: usize = 64;

/// Drain `rx` onto the main thread, handing each event to `on_event`.
/// `on_event` returns false to stop the pump — typically when the entity it
/// feeds is gone or the channel closed. Spawns detached; call once at
/// startup, before the first frame.
pub fn spawn<T: Send + 'static>(
    cx: &mut gpui::App,
    rx: async_channel::Receiver<T>,
    mut on_event: impl FnMut(&mut AsyncApp, T) -> bool + 'static,
) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            let timeout = cx.background_executor().timer(POLL);
            let mut recv = Box::pin(rx.recv());
            match futures_util::future::select(recv.as_mut(), timeout).await {
                futures_util::future::Either::Left((received, _)) => {
                    let Ok(first) = received else { break };
                    if !on_event(cx, first) {
                        break;
                    }
                    let mut batch = 1;
                    while batch < BURST {
                        match rx.try_recv() {
                            Ok(event) => {
                                if !on_event(cx, event) {
                                    return;
                                }
                                batch += 1;
                            }
                            Err(_) => break,
                        }
                    }
                }
                futures_util::future::Either::Right((_, _)) => continue,
            }
        }
    })
    .detach();
}
