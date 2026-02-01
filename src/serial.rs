use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use gloo_timers::future::TimeoutFuture;
use web_sys::Window;
use futures::lock::Mutex;

#[derive(Clone, Default)]
pub struct SerialManager {
    port: std::rc::Rc<std::cell::RefCell<Option<JsValue>>>,
    reader: std::rc::Rc<Mutex<Option<JsValue>>>,
    buffer: std::rc::Rc<Mutex<String>>,
    drain_running: std::rc::Rc<std::cell::Cell<bool>>,
}

impl SerialManager {
    pub fn new() -> Self {
        Self {
            port: std::rc::Rc::new(std::cell::RefCell::new(None)),
            reader: std::rc::Rc::new(Mutex::new(None)),
            buffer: std::rc::Rc::new(Mutex::new(String::new())),
            drain_running: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
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
        // clear any existing reader when connecting
        {
            let mut guard = self.reader.lock().await;
            *guard = None;
        }
        // clear buffer
        {
            let mut b = self.buffer.lock().await;
            b.clear();
        }
        // reading state removed; persistent reader is managed via `reader` field
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

    /// Read a single response frame from the serial port's reader.
    /// Returns decoded UTF-8 string or hex if non-UTF8.
    /// Read a single chunk from a persistent reader (creating it if needed).
    /// This does not release the reader lock; the reader remains owned until `disconnect()`.
    pub async fn read_from_persistent_reader(&self) -> Result<String, JsValue> {
        // Debugging logs to help trace reader lifecycle and incoming data
        web_sys::console::log_1(&JsValue::from_str("serial: read_from_persistent_reader start"));
        let port = self
            .port
            .borrow()
            .as_ref()
            .ok_or_else(|| JsValue::from_str("serial not connected"))?
            .clone();

        let readable = Reflect::get(&port, &JsValue::from_str("readable"))?;
        if readable.is_undefined() || readable.is_null() {
            return Err(JsValue::from_str("port not readable"));
        }

        // Use an async Mutex to serialize reader creation and access.
        let reader = {
            let mut guard = self.reader.lock().await;
            if let Some(r) = guard.as_ref() {
                web_sys::console::log_1(&JsValue::from_str("serial: reusing existing reader"));
                r.clone()
            } else {
                web_sys::console::log_1(&JsValue::from_str("serial: creating reader"));
                let get_reader = Reflect::get(&readable, &JsValue::from_str("getReader"))?
                    .dyn_into::<Function>()?;
                let r = get_reader.call0(&readable)?;
                *guard = Some(r.clone());
                r
            }
        };

        let read_fn = Reflect::get(&reader, &JsValue::from_str("read"))?
            .dyn_into::<Function>()?;
        let read_promise = read_fn.call0(&reader)?;
        let read_res = JsFuture::from(read_promise.dyn_into::<Promise>()?).await?;

        let done = Reflect::get(&read_res, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(false);
        let mut result = String::new();
        if !done {
            let val = Reflect::get(&read_res, &JsValue::from_str("value"))?;
            let uint8 = Uint8Array::new(&val);
            // Log the number of bytes received
            let len = uint8.length();
            web_sys::console::log_1(&JsValue::from_str(&format!("serial: read {} bytes", len)));
            let vec = uint8.to_vec();
            // Try UTF-8, else hex
            match std::str::from_utf8(&vec) {
                Ok(s) => result.push_str(s),
                Err(_) => {
                    let hex = vec.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                    result.push_str(&hex);
                }
            }
            // Log the decoded payload for easier debugging in console
            web_sys::console::log_1(&JsValue::from_str(&format!("serial: payload: {}", result)));
        }

        // Accumulate into buffer and return a complete frame (ending with ';') if available.
        if !result.is_empty() {
            let mut buf = self.buffer.lock().await;
            buf.push_str(&result);
            if let Some(pos) = buf.find(';') {
                // include delimiter
                let frame = buf.drain(..=pos).collect::<String>();
                return Ok(frame);
            }
        }

        Ok(String::new())
    }

    /// Disconnect the serial port and cancel any active reader.
    pub async fn disconnect(&self) -> Result<(), JsValue> {
        // Take and cancel the reader under the mutex so we avoid RefCell panics.
        let reader_opt = {
            let mut guard = self.reader.lock().await;
            guard.take()
        };
        if let Some(reader) = reader_opt {
            let cancel = Reflect::get(&reader, &JsValue::from_str("cancel"))?;
            if !cancel.is_undefined() && !cancel.is_null() {
                let cancel_fn = cancel.dyn_into::<Function>()?;
                let _ = JsFuture::from(cancel_fn.call0(&reader)?.dyn_into::<Promise>()?).await;
            }
            // try releaseLock as well
            let release = Reflect::get(&reader, &JsValue::from_str("releaseLock"))?;
            if !release.is_undefined() && !release.is_null() {
                let release_fn = release.dyn_into::<Function>()?;
                let _ = release_fn.call0(&reader);
            }
        }

        // reader_claim removed; reader state is managed by the async Mutex

        // Close port if present. Clone its JsValue out of the RefCell so we
        // don't hold a borrow across the `await` below.
        let port_opt = { self.port.borrow().as_ref().map(|p| p.clone()) };
        if let Some(port) = port_opt {
            let close = Reflect::get(&port, &JsValue::from_str("close"))?;
            if !close.is_undefined() && !close.is_null() {
                let close_fn = close.dyn_into::<Function>()?;
                let _ = JsFuture::from(close_fn.call0(&port)?.dyn_into::<Promise>()?).await;
            }
            *self.port.borrow_mut() = None;
        }

        Ok(())
    }

    /// Stop and cancel the persistent reader but keep the port open.
    pub async fn stop_reader(&self) -> Result<(), JsValue> {
        // Take and cancel the reader under the mutex so we avoid RefCell panics.
        let reader_opt = {
            let mut guard = self.reader.lock().await;
            guard.take()
        };
        if let Some(reader) = reader_opt {
            let cancel = Reflect::get(&reader, &JsValue::from_str("cancel"))?;
            if !cancel.is_undefined() && !cancel.is_null() {
                let cancel_fn = cancel.dyn_into::<Function>()?;
                let _ = JsFuture::from(cancel_fn.call0(&reader)?.dyn_into::<Promise>()?).await;
            }
            // try releaseLock as well
            let release = Reflect::get(&reader, &JsValue::from_str("releaseLock"))?;
            if !release.is_undefined() && !release.is_null() {
                let release_fn = release.dyn_into::<Function>()?;
                let _ = release_fn.call0(&reader);
            }
        }
        // clear buffer when stopping reader
        {
            let mut b = self.buffer.lock().await;
            b.clear();
        }
        Ok(())
    }

    /// Spawn a background task that periodically reads from the persistent
    /// reader to keep the internal buffer drained. Safe to call multiple
    /// times; only one drain task runs at once.
    pub fn spawn_buffer_drain(&self) {
        if self.drain_running.get() {
            return;
        }
        self.drain_running.set(true);
        let sm = self.clone();
        spawn_local(async move {
            while sm.drain_running.get() && sm.port.borrow().is_some() {
                let _ = sm.read_from_persistent_reader().await;
                TimeoutFuture::new(100).await;
            }
            sm.drain_running.set(false);
        });
    }

    /// Stop the background drain task (if running).
    pub fn stop_buffer_drain(&self) {
        self.drain_running.set(false);
    }

    /// Send raw bytes (alias for write_command)
    pub async fn send_raw(&self, cmd: &str) -> Result<(), JsValue> {
        self.write_command(cmd).await
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

/// Lightweight helper for Kenwood-style commands. Kept separate so we can
/// add other drivers later.
pub struct KenwoodDriver;

impl KenwoodDriver {
    pub async fn tune(serial: &SerialManager, freq_hz: u64, mode: &str) -> Result<(), JsValue> {
        serial.tune_kenwood_ts570(freq_hz, mode).await
    }

    pub async fn test_tune(serial: &SerialManager) -> Result<(), JsValue> {
        // 14.062 MHz = 14_062_000 Hz
        let hz = (14.062_f64 * 1_000_000.0).round() as u64;
        serial.tune_kenwood_ts570(hz, "CW").await
    }

    pub async fn send_raw(serial: &SerialManager, cmd: &str) -> Result<(), JsValue> {
        serial.send_raw(cmd).await
    }

    pub async fn set_vfo_a(serial: &SerialManager) -> Result<(), JsValue> {
        serial.write_command("FR0;").await?;
        serial.write_command("FT0;").await?;
        Ok(())
    }

    pub async fn set_vfo_b(serial: &SerialManager) -> Result<(), JsValue> {
        serial.write_command("FR1;").await?;
        serial.write_command("FT1;").await?;
        Ok(())
    }

    pub async fn set_mode(serial: &SerialManager, mode: &str) -> Result<(), JsValue> {
        let mode_cmd = match mode.to_uppercase().as_str() {
            "LSB" => "MD1;",
            "USB" => "MD2;",
            "CW" => "MD3;",
            "FM" => "MD4;",
            "AM" => "MD5;",
            _ => "MD2;",
        };
        serial.write_command(mode_cmd).await
    }

    pub async fn query_frequency(serial: &SerialManager) -> Result<String, JsValue> {
        // Query current VFO A frequency; response should be read from the
        // persistent streaming reader (if present). We still send the query
        // command here and return the next available chunk from the reader.
        serial.write_command("FA;").await?;
        let resp = serial.read_from_persistent_reader().await?;
        Ok(resp)
    }
}
