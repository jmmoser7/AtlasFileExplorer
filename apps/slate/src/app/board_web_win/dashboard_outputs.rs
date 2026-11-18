//! Fixed, read-only DOM query. A page publishes data by changing its JSON element.
use super::*;

/// Keep the watch inside the browser. Only changed data crosses into a Slate
/// frame. Output elements can change without any pointer event, and legacy
/// control pages can enqueue native intents in JavaScript without editing DOM.
pub(super) fn watch_value(
    webview: &ICoreWebView2,
    read: &str,
    previous: Option<&Option<String>>,
    watch_dom: bool,
    completed: impl Fn(Result<Option<String>, String>) + 'static,
) -> windows::core::Result<()> {
    let previous = match previous {
        Some(value) => serde_json::to_string(value).expect("optional JSON string"),
        None => "undefined".into(),
    };
    let extra = if watch_dom {
        ""
    } else {
        "window.addEventListener('slate-control-queued',check);
         if(!window.slateControl?.__signalsIntents)timer=setInterval(check,125);"
    };
    let watch_key = if watch_dom { "output" } else { "control" };
    let expression = format!(
        "(async()=>{{const read=()=>({read}), previous={previous};
        const key=Symbol.for('slate.dashboard.watch.{watch_key}');
        if(typeof document[key]==='function')document[key]();
        const first=read(); if(first!==previous)return first;
        return new Promise(resolve=>{{let timer;
          const check=()=>{{const value=read();if(value!==previous)finish(value)}};
          const observer=new MutationObserver(check);
          const finish=value=>{{observer.disconnect();clearInterval(timer);
            window.removeEventListener('slate-control-queued',check);
            delete document[key];resolve(value)}};
          document[key]=()=>finish(previous);
          observer.observe(document,{{subtree:true,childList:true,characterData:true,
             attributes:true,attributeFilter:['type','id','data-slate-ready','data-slate-edit']}});
          {extra}
        }})}})()"
    );
    let parameters = serde_json::json!({
        "expression":expression,"awaitPromise":true,"returnByValue":true,
        "silent":true,"userGesture":false
    });
    let method = wide("Runtime.evaluate");
    let params = wide(&parameters.to_string());
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
        move |status: windows::core::Result<()>, encoded: String| {
            let value = status.map_err(|e| e.to_string()).and_then(|()| {
                let reply: serde_json::Value =
                    serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
                match reply.pointer("/result/value") {
                    Some(serde_json::Value::Null) => Ok(None),
                    Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
                    _ => Err("Dashboard read was interrupted".into()),
                }
            });
            completed(value);
            Ok(())
        },
    ));
    unsafe {
        webview.CallDevToolsProtocolMethod(
            PCWSTR(method.as_ptr()),
            PCWSTR(params.as_ptr()),
            &handler,
        )
    }
}

const READ: &str = r#"(()=>{const e=document.querySelector('script#slate-outputs[type="application/slate-outputs+json"]');if(!e)return null;
if(location.protocol==='file:' && e.hasAttribute('data-slate-controller') && !e.hasAttribute('data-slate-ready'))return null;
const s=e.textContent;if(typeof s!=='string'||s.length>262144)return "";
const p=e.getAttribute('data-slate-edit');return p && p.length<=49664?p+'\u001e'+s:s;})()"#;

type Reply = Result<Option<String>, String>;

#[derive(Default)]
pub(super) struct Read {
    receiver: Option<crossbeam_channel::Receiver<Reply>>,
    previous: Option<Option<String>>,
    retry_at: Option<std::time::Instant>,
}

impl Read {
    pub(super) fn ready(&self) -> bool {
        self.receiver.as_ref().is_some_and(|rx| !rx.is_empty())
    }

    pub(super) fn take(&mut self, wake: &egui::Context) -> Option<Reply> {
        let reply = self.receiver.as_ref()?.try_recv().ok()?;
        self.receiver = None;
        if let Ok(packet) = &reply {
            self.previous = Some(packet.clone());
        } else {
            let delay = std::time::Duration::from_secs(1);
            self.retry_at = Some(std::time::Instant::now() + delay);
            wake.request_repaint_after(delay);
        }
        Some(reply)
    }
}

impl Webview2Host {
    pub(super) fn watch_dashboard(&mut self, id: NodeId, outputs: bool) -> bool {
        let Some(view) = self.views.get(&id) else {
            return false;
        };
        let (webview, previous, sender) = {
            let mut page = view.shared.borrow_mut();
            if page.cancelled || page.navigating || page.url.is_none() {
                return false;
            }
            let Some(webview) = page.webview.clone() else {
                return false;
            };
            let read = if outputs {
                &mut page.output_read
            } else {
                &mut page.control_read
            };
            if read.receiver.is_some() {
                return false;
            }
            if let Some(remaining) = read
                .retry_at
                .and_then(|t| t.checked_duration_since(std::time::Instant::now()))
            {
                // An input frame may arrive before the retry. Preserve the
                // original deadline instead of restarting the delay.
                self.wake.request_repaint_after(remaining);
                return false;
            }
            read.retry_at = None;
            let (sender, receiver) = crossbeam_channel::bounded(1);
            read.receiver = Some(receiver);
            (webview, read.previous.clone(), sender)
        };
        // WebView2 can call completions reentrantly from Close or a navigation.
        // A document owns its receiver. Late completions have no access to the
        // mutable page and disappear automatically when that document is gone.
        let wake = self.wake.clone();
        let on_error = sender.clone();
        let script = if outputs {
            READ
        } else {
            atlas_agent::control::POLL_SCRIPT
        };
        let result = watch_value(&webview, script, previous.as_ref(), outputs, move |value| {
            if sender.try_send(value).is_ok() {
                wake.request_repaint();
            }
        });
        if let Err(error) = result {
            if on_error.try_send(Err(error.to_string())).is_ok() {
                self.wake.request_repaint();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_consumed_on_an_idle_frame_schedules_its_retry() {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |_| {});
        }
        let (sender, receiver) = crossbeam_channel::bounded(1);
        sender.send(Err("temporarily unavailable".into())).unwrap();
        let mut read = Read {
            receiver: Some(receiver),
            ..Default::default()
        };
        let out = ctx.run(Default::default(), |ctx| {
            assert!(read.take(ctx).unwrap().is_err())
        });
        let wait = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert!(wait > std::time::Duration::ZERO && wait <= std::time::Duration::from_secs(1));
        let deadline = read.retry_at.unwrap();
        assert!(read.take(&ctx).is_none());
        assert_eq!(read.retry_at, Some(deadline));
    }
}
