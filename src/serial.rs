use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use gloo_timers::future::TimeoutFuture;
use web_sys::Window;

#[derive(Clone, Default)]
pub struct SerialManager {
    port: std::rc::Rc<std::cell::RefCell<Option<JsValue>>>,
}

impl SerialManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn connect(&self, baud_rate: u32) -> Result<(), JsValue> {
        let window: Window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let navigator = window.navigator();
        let serial = Reflect::get(&navigator, &JsValue::from_str("serial"))?;
        if serial.is_undefined() || serial.is_null() {
            return Err(JsValue::from_str(
                "Web Serial not available. Use Chromium and HTTPS or localhost.",
            ));
        }
        let request_port = Reflect::get(&serial, &JsValue::from_str("requestPort"))?
            .dyn_into::<Function>()?;
        let promise = request_port.call0(&serial)?;
        let port_js = JsFuture::from(promise.dyn_into::<Promise>()?).await?;

        let options = Object::new();
        Reflect::set(&options, &JsValue::from_str("baudRate"), &JsValue::from_f64(baud_rate as f64))?;

        let open_fn = Reflect::get(&port_js, &JsValue::from_str("open"))?
            .dyn_into::<Function>()?;
        let open_promise = open_fn.call1(&port_js, &options)?;
        JsFuture::from(open_promise.dyn_into::<Promise>()?).await?;

        *self.port.borrow_mut() = Some(port_js);
        Ok(())
    }

    pub async fn write_command(&self, command: &str) -> Result<(), JsValue> {
        let port = self
            .port
            .borrow()
            .as_ref()
            .ok_or_else(|| JsValue::from_str("serial not connected"))?
            .clone();

        let writable = Reflect::get(&port, &JsValue::from_str("writable"))?;
        if writable.is_undefined() || writable.is_null() {
            return Err(JsValue::from_str("port not writable"));
        }

        let get_writer = Reflect::get(&writable, &JsValue::from_str("getWriter"))?
            .dyn_into::<Function>()?;
        let writer = get_writer.call0(&writable)?;

        let bytes = command.as_bytes();
        let uint8 = Uint8Array::from(bytes);
        let write_fn = Reflect::get(&writer, &JsValue::from_str("write"))?
            .dyn_into::<Function>()?;
        let write_promise = write_fn.call1(&writer, &uint8)?;
        JsFuture::from(write_promise.dyn_into::<Promise>()?).await?;

        let release = Reflect::get(&writer, &JsValue::from_str("releaseLock"))?
            .dyn_into::<Function>()?;
        release.call0(&writer)?;
        Ok(())
    }

    pub async fn tune_kenwood_ts570(&self, freq_hz: u64, mode: &str) -> Result<(), JsValue> {
        let mode_cmd = match mode.to_uppercase().as_str() {
            "LSB" => "MD1;",
            "USB" => "MD2;",
            "CW" => "MD3;",
            "FM" => "MD4;",
            "AM" => "MD5;",
            "SSB" => "MD2;",
            "FT8" | "FT4" | "PSK31" | "RTTY" => "MD2;",
            _ => "MD2;",
        };

        let freq_cmd = format!("FA{:011};", freq_hz);

        // Ensure VFO A is active for RX/TX
        self.write_command("FR0;").await?;
        self.write_command("FT0;").await?;
        TimeoutFuture::new(80).await;

        // Set frequency first, then mode, with short delays
        self.write_command(&freq_cmd).await?;
        TimeoutFuture::new(80).await;
        self.write_command(mode_cmd).await?;
        Ok(())
    }
}
