use shoop_app::ApplicationAudioPreview;
use shoop_egui::AppIntent;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{
    AudioBufferSourceNode, AudioContext, AudioContextState, AudioScheduledSourceNode, Event,
};

struct ActivePreview {
    request_id: u64,
    source: AudioBufferSourceNode,
    ended: Closure<dyn FnMut(Event)>,
    done: Rc<Cell<bool>>,
}

#[derive(Default)]
pub struct BrowserPreviewPlayer {
    fallback_context: Option<AudioContext>,
    active: Option<ActivePreview>,
}

impl BrowserPreviewPlayer {
    pub fn play(
        &mut self,
        context: Option<AudioContext>,
        preview: ApplicationAudioPreview,
    ) -> Result<(), String> {
        self.stop_active();
        if preview.sample_rate == 0 || preview.samples.is_empty() {
            return Err("Click preview has no playable audio".to_owned());
        }
        let context = match context {
            Some(context) => context,
            None => {
                if self.fallback_context.is_none() {
                    self.fallback_context =
                        Some(AudioContext::new().map_err(|error| {
                            format!("Could not create preview audio: {error:?}")
                        })?);
                }
                self.fallback_context
                    .as_ref()
                    .expect("fallback context was initialized")
                    .clone()
            }
        };
        if context.state() != AudioContextState::Running {
            let _resume = context
                .resume()
                .map_err(|error| format!("Could not resume preview audio: {error:?}"))?;
        }
        let frame_count = u32::try_from(preview.samples.len())
            .map_err(|_| "Click preview is too large for Web Audio".to_owned())?;
        let buffer = context
            .create_buffer(1, frame_count, preview.sample_rate as f32)
            .map_err(|error| format!("Could not allocate preview buffer: {error:?}"))?;
        buffer
            .copy_to_channel(&preview.samples, 0)
            .map_err(|error| format!("Could not copy preview samples: {error:?}"))?;
        let source = context
            .create_buffer_source()
            .map_err(|error| format!("Could not create preview source: {error:?}"))?;
        source.set_buffer(Some(&buffer));
        source
            .connect_with_audio_node(&context.destination())
            .map_err(|error| format!("Could not connect preview output: {error:?}"))?;
        let done = Rc::new(Cell::new(false));
        let callback_done = Rc::clone(&done);
        let ended = Closure::wrap(Box::new(move |_event: Event| {
            callback_done.set(true);
        }) as Box<dyn FnMut(_)>);
        let scheduled: &AudioScheduledSourceNode = source.unchecked_ref();
        scheduled.set_onended(Some(ended.as_ref().unchecked_ref()));
        source
            .start()
            .map_err(|error| format!("Could not start click preview: {error:?}"))?;
        self.active = Some(ActivePreview {
            request_id: preview.request_id,
            source,
            ended,
            done,
        });
        Ok(())
    }

    pub fn update(&mut self) -> Option<AppIntent> {
        let request_id = self
            .active
            .as_ref()
            .filter(|active| active.done.get())
            .map(|active| active.request_id)?;
        self.stop_active();
        Some(AppIntent::CompleteClickTrackPreview {
            request_id,
            success: true,
            message: "Click preview completed".to_owned(),
        })
    }

    fn stop_active(&mut self) {
        if let Some(active) = self.active.take() {
            let scheduled: &AudioScheduledSourceNode = active.source.unchecked_ref();
            scheduled.set_onended(None);
            let _ = scheduled.stop();
            let _ = active.source.disconnect();
            drop(active.ended);
        }
    }
}

impl Drop for BrowserPreviewPlayer {
    fn drop(&mut self) {
        self.stop_active();
        if let Some(context) = self.fallback_context.take() {
            let _ = context.close();
        }
    }
}
