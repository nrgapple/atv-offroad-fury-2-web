//! Browser text entry for the frontend's Host/Join forms: a real `<input>`
//! laid over each drawn field (menu-screens.js `buildForm`), so phones open
//! their soft keyboard and desktop browsers get native editing, selection
//! and IME. The canvas draws the field box; the input draws the text.
use crate::Message;
use wasm_bindgen::{JsCast, closure::Closure};
use winit::event_loop::EventLoopProxy;

struct Input {
    element: web_sys::HtmlInputElement,
    action: &'static str,
    _listeners: [Closure<dyn FnMut(web_sys::Event)>; 3],
}

#[derive(Default)]
pub struct TextInputs {
    inputs: Vec<Input>,
}

impl TextInputs {
    /// Matches the inputs to this frame's fields and their CSS rects;
    /// no fields removes them.
    pub fn sync(
        &mut self,
        fields: &[atv_ui::front::controller::Field],
        rects: &[atv_ui::Rect],
        proxy: &EventLoopProxy<Message>,
    ) {
        let keep = fields.len().min(rects.len());
        let same = self.inputs.len() == keep
            && self
                .inputs
                .iter()
                .zip(fields)
                .all(|(i, f)| i.action == f.action);
        if !same {
            for input in self.inputs.drain(..) {
                input.element.remove();
            }
            let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            for (index, field) in fields.iter().take(keep).enumerate() {
                let Ok(element) = document.create_element("input") else {
                    continue;
                };
                let Ok(element) = element.dyn_into::<web_sys::HtmlInputElement>() else {
                    continue;
                };
                element.set_type("text");
                element.set_class_name("atv-text-input ms-input");
                let _ = element.set_attribute("autocomplete", "off");
                let _ = element.set_attribute("spellcheck", "false");
                let _ = element.set_attribute("aria-label", field.label);
                element.set_max_length(field.max_length as i32);
                element.set_value(&field.value);
                let style = element.style();
                for (key, value) in [
                    ("position", "fixed"),
                    ("box-sizing", "border-box"),
                    ("border", "0"),
                    ("outline", "none"),
                    ("background", "transparent"),
                    ("color", "#f4f4f4"),
                    ("caret-color", "#e8c832"),
                    ("padding", "4px 20px"),
                    ("letter-spacing", ".06em"),
                    ("z-index", "5"),
                ] {
                    let _ = style.set_property(key, value);
                }
                let _ = style.set_property("font", &format!("700 19px {}", atv_ui::text::DISPLAY));
                let action = field.action;
                let on_input = {
                    let proxy = proxy.clone();
                    let element = element.clone();
                    Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
                        let _ = proxy.send_event(Message::FrontText(action, element.value()));
                    })
                };
                let on_focus = {
                    let proxy = proxy.clone();
                    Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
                        let _ = proxy.send_event(Message::FrontFocus(Some(index)));
                    })
                };
                let on_key = {
                    let proxy = proxy.clone();
                    let element = element.clone();
                    Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
                        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                            return;
                        };
                        // Enter or Escape hands the keyboard back to the menu.
                        if key.key() == "Enter" || key.key() == "Escape" {
                            let _ = element.blur();
                            let _ = proxy.send_event(Message::FrontFocus(None));
                        }
                    })
                };
                let _ = element
                    .add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref());
                let _ = element
                    .add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref());
                let _ = element
                    .add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
                if let Some(body) = document.body() {
                    let _ = body.append_child(&element);
                }
                self.inputs.push(Input {
                    element,
                    action,
                    _listeners: [on_input, on_focus, on_key],
                });
            }
        }
        let active = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element());
        for ((input, field), rect) in self.inputs.iter().zip(fields).zip(rects) {
            let style = input.element.style();
            let _ = style.set_property("left", &format!("{}px", rect.x));
            let _ = style.set_property("top", &format!("{}px", rect.y));
            let _ = style.set_property("width", &format!("{}px", rect.w));
            let _ = style.set_property("height", &format!("{}px", rect.h));
            // The focused input owns its text; others follow the state.
            let focused = active
                .as_ref()
                .is_some_and(|a| a == input.element.unchecked_ref::<web_sys::Element>());
            if !focused && input.element.value() != field.value {
                input.element.set_value(&field.value);
            }
        }
    }
}

impl Drop for TextInputs {
    fn drop(&mut self) {
        for input in self.inputs.drain(..) {
            input.element.remove();
        }
    }
}
