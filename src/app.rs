use crate::serial::SerialManager;
use gloo_net::http::Request;
use gloo_timers::callback::Interval;
use serde::Deserialize;
use yew::prelude::*;
use yew::events::InputEvent;
use web_sys::{HtmlInputElement, Storage};
use wasm_bindgen_futures::spawn_local;
use crate::serial::KenwoodDriver;
use wasm_bindgen::JsValue;

const SPOTS_URL: &str = "https://api2.sota.org.uk/api/spots/20/%7Bfilter%7D?filter=all";
const REFRESH_MS: u32 = 5 * 60 * 1000;
const STORAGE_MIN_FREQ: &str = "sotachaser.min_freq_mhz";
const STORAGE_MAX_FREQ: &str = "sotachaser.max_freq_mhz";

fn get_storage() -> Option<Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn load_freq(key: &str, default_value: f64) -> f64 {
    if let Some(storage) = get_storage() {
        if let Ok(Some(value)) = storage.get_item(key) {
            if let Ok(parsed) = value.parse::<f64>() {
                return parsed;
            }
        }
    }
    default_value
}

fn save_freq(key: &str, value: f64) {
    if let Some(storage) = get_storage() {
        let _ = storage.set_item(key, &value.to_string());
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SpotRaw {
    #[serde(rename = "timeStamp")]
    timestamp: Option<String>,
    #[serde(rename = "activatorCallsign")]
    callsign: Option<String>,
    #[serde(rename = "summitCode")]
    summit: Option<String>,
    frequency: Option<String>,
    mode: Option<String>,
    comments: Option<String>,
}

#[derive(Debug, Clone)]
struct Spot {
    timestamp: String,
    callsign: String,
    summit: String,
    frequency_mhz: f64,
    mode: String,
    comments: String,
}

impl Spot {
    fn from_raw(raw: SpotRaw) -> Option<Self> {
        let freq = raw.frequency.unwrap_or_default();
        let freq = freq.trim();
        let frequency_mhz = freq.parse::<f64>().ok()?;
        if frequency_mhz <= 0.0 {
            return None;
        }
        Some(Self {
            timestamp: raw.timestamp.unwrap_or_default(),
            callsign: raw.callsign.unwrap_or_default(),
            summit: raw.summit.unwrap_or_default(),
            frequency_mhz,
            mode: raw.mode.unwrap_or_default(),
            comments: raw.comments.unwrap_or_default(),
        })
    }
}

fn format_time(ts: &str) -> String {
    // Expecting ISO-like timestamp. Extract HH:MM:SS(.sss) if present.
    if let Some(t_pos) = ts.find('T') {
        let time_part = &ts[t_pos + 1..];
        let trimmed = time_part
            .trim_end_matches('Z')
            .split(['.', '+', '-'])
            .next()
            .unwrap_or(time_part);
        return trimmed.to_string();
    }
    ts.to_string()
}

#[function_component(App)]
pub fn app() -> Html {
    let spots = use_state(Vec::<Spot>::new);
    let selected_row = use_state(|| None::<usize>);
    let status = use_state(|| "".to_string());
    let connected = use_state(|| false);
    let serial = use_state(SerialManager::new);
    let min_freq = use_state(|| 7.0_f64);
    let max_freq = use_state(|| 29.7_f64);
    let show_settings = use_state(|| false);
    let raw_cmd = use_state(|| "".to_string());
    let response_log = use_state(Vec::<String>::new);
    let last_rx = use_state(|| "".to_string());

    {
        let min_freq = min_freq.clone();
        let max_freq = max_freq.clone();
        use_effect_with((), move |_| {
            min_freq.set(load_freq(STORAGE_MIN_FREQ, 7.0));
            max_freq.set(load_freq(STORAGE_MAX_FREQ, 29.7));
            || ()
        });
    }

    {
        let spots = spots.clone();
        let status = status.clone();
        use_effect_with((), move |_| {
            let fetch = move || {
                let spots = spots.clone();
                let status = status.clone();
                spawn_local(async move {
                    status.set("Refreshing spots...".to_string());
                    let response = Request::get(SPOTS_URL).send().await;
                    match response {
                        Ok(res) => match res.json::<Vec<SpotRaw>>().await {
                            Ok(raw) => {
                                let parsed = raw
                                    .into_iter()
                                    .filter_map(Spot::from_raw)
                                    .collect::<Vec<_>>();
                                spots.set(parsed);
                                status.set("".to_string());
                            }
                            Err(err) => {
                                status.set(format!("Failed to parse spots: {}", err));
                            }
                        },
                        Err(err) => {
                            status.set(format!("Failed to fetch spots: {}", err));
                        }
                    }
                });
            };

            fetch();
            let interval = Interval::new(REFRESH_MS, fetch);
            move || drop(interval)
        });
    }

    let on_connect = {
        let serial = serial.clone();
        let connected = connected.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let serial = serial.clone();
            let connected = connected.clone();
            let status = status.clone();
            spawn_local(async move {
                status.set("Requesting serial port...".to_string());
                match serial.connect(9600).await {
                    Ok(()) => {
                        connected.set(true);
                        status.set("Serial connected".to_string());
                    }
                    Err(err) => {
                        status.set(format!("Serial connect failed: {:?}", err));
                    }
                }
            });
        })
    };

    let on_refresh = {
        let spots = spots.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let spots = spots.clone();
            let status = status.clone();
            spawn_local(async move {
                status.set("Refreshing spots...".to_string());
                let response = Request::get(SPOTS_URL).send().await;
                match response {
                    Ok(res) => match res.json::<Vec<SpotRaw>>().await {
                        Ok(raw) => {
                            let parsed = raw
                                .into_iter()
                                .filter_map(Spot::from_raw)
                                .collect::<Vec<_>>();
                            spots.set(parsed);
                            status.set("".to_string());
                        }
                        Err(err) => {
                            status.set(format!("Failed to parse spots: {}", err));
                        }
                    },
                    Err(err) => {
                        status.set(format!("Failed to fetch spots: {}", err));
                    }
                }
            });
        })
    };

    let on_tune = {
        let serial = serial.clone();
        let selected_row = selected_row.clone();
        let status = status.clone();
        let connected = connected.clone();
        let spots = spots.clone();
        let min_freq = min_freq.clone();
        let max_freq = max_freq.clone();
        Callback::from(move |row: usize| {
            if !*connected {
                status.set("Connect serial first".to_string());
                return;
            }

            let serial = serial.clone();
            let selected_row = selected_row.clone();
            let status = status.clone();
            let spots = spots.clone();
            let min_freq = *min_freq;
            let max_freq = *max_freq;
            spawn_local(async move {
                if let Some(spot) = spots.get(row) {
                    if spot.frequency_mhz < min_freq || spot.frequency_mhz > max_freq {
                        status.set(format!(
                            "Blocked: {:.3} MHz outside {:.3}–{:.3} MHz",
                            spot.frequency_mhz, min_freq, max_freq
                        ));
                        return;
                    }
                    let freq_hz = (spot.frequency_mhz * 1_000_000.0).round() as u64;
                    status.set(format!("Tuning {} MHz {}", spot.frequency_mhz, spot.mode));
                    match serial.tune_kenwood_ts570(freq_hz, &spot.mode).await {
                        Ok(()) => {
                            selected_row.set(Some(row));
                            status.set("Tuned".to_string());
                        }
                        Err(err) => {
                            status.set(format!("Tune failed: {:?}", err));
                        }
                    }
                }
            });
        })
    };

    use gloo_timers::future::TimeoutFuture;

    let on_toggle_settings = {
        let show_settings = show_settings.clone();
        let serial = serial.clone();
        let response_log = response_log.clone();
        let last_rx_handle = last_rx.clone();
        Callback::from(move |_| {
            let currently = *show_settings;
            // open settings
            if !currently {
                show_settings.set(true);
                // start buffer drain while settings are open
                serial.spawn_buffer_drain();

                // start background read loop using the persistent reader
                let show_settings_clone = show_settings.clone();
                let serial_clone = serial.clone();
                let response_log_clone = response_log.clone();
                let last_rx_clone = last_rx_handle.clone();
                spawn_local(async move {
                    while *show_settings_clone {
                        match serial_clone.read_from_persistent_reader().await {
                            Ok(resp) if !resp.is_empty() => {
                                // push to response log and update visible last_rx
                                let mut v = (*response_log_clone).clone();
                                let entry = format!("RX: {}", resp);
                                web_sys::console::log_1(&JsValue::from_str(&format!("app: pushing {}", entry)));
                                v.push(entry.clone());
                                response_log_clone.set(v);
                                last_rx_clone.set(entry.clone());
                                // Log state sizes to help diagnose why DOM isn't updating
                                web_sys::console::log_1(&JsValue::from_str(&format!(
                                    "app: response_log length = {} last_rx = {}",
                                    (*response_log_clone).len(), (*last_rx_clone).clone()
                                )));
                            }
                            _ => {
                                // no data this iteration
                            }
                        }
                        TimeoutFuture::new(200).await;
                    }
                });
            } else {
                // closing settings: stop the background reader and keep port open
                show_settings.set(false);
                // stop drain immediately, then cancel the reader
                serial.stop_buffer_drain();
                let serial = serial.clone();
                spawn_local(async move {
                    let _ = serial.stop_reader().await;
                });
            }
        })
    };

    let on_send_raw = {
        let raw_cmd = raw_cmd.clone();
        let serial = serial.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let cmd = (*raw_cmd).clone();
            let serial = serial.clone();
            let status = status.clone();
            spawn_local(async move {
                if cmd.is_empty() {
                    status.set("Empty raw command".to_string());
                    return;
                }
                match KenwoodDriver::send_raw(&*serial, &cmd).await {
                    Ok(()) => status.set("Raw command sent".to_string()),
                    Err(e) => status.set(format!("Send failed: {:?}", e)),
                }
            });
        })
    };

    let on_raw_input = {
        let raw_cmd = raw_cmd.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            raw_cmd.set(input.value());
        })
    };

    let on_test_14062 = {
        let serial = serial.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let serial = serial.clone();
            let status = status.clone();
            spawn_local(async move {
                match KenwoodDriver::test_tune(&*serial).await {
                    Ok(()) => status.set("14.062 CW test tune sent".to_string()),
                    Err(e) => status.set(format!("Test tune failed: {:?}", e)),
                }
            });
        })
    };

    let on_vfo_a = {
        let serial = serial.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let serial = serial.clone();
            let status = status.clone();
            spawn_local(async move {
                match KenwoodDriver::set_vfo_a(&*serial).await {
                    Ok(()) => status.set("VFO A selected".to_string()),
                    Err(e) => status.set(format!("VFO A failed: {:?}", e)),
                }
                // try read
                // response will be streamed to the log by the background reader
            });
        })
    };

    let on_vfo_b = {
        let serial = serial.clone();
        let status = status.clone();
        Callback::from(move |_| {
            let serial = serial.clone();
            let status = status.clone();
            spawn_local(async move {
                match KenwoodDriver::set_vfo_b(&*serial).await {
                    Ok(()) => status.set("VFO B selected".to_string()),
                    Err(e) => status.set(format!("VFO B failed: {:?}", e)),
                }
                // response will be streamed to the log by the background reader
            });
        })
    };

    let on_set_mode = {
        let serial = serial.clone();
        let status = status.clone();
        Callback::from(move |mode: String| {
            let serial = serial.clone();
            let status = status.clone();
            spawn_local(async move {
                match KenwoodDriver::set_mode(&*serial, &mode).await {
                    Ok(()) => status.set(format!("Mode set: {}", mode)),
                    Err(e) => status.set(format!("Set mode failed: {:?}", e)),
                }
                // response will be streamed to the log by the background reader
            });
        })
    };

    let on_query_freq = {
        let serial = serial.clone();
        let status = status.clone();
        let response_log = response_log.clone();
        let last_rx = last_rx.clone();
        Callback::from(move |_| {
            let serial = serial.clone();
            let status = status.clone();
            let response_log = response_log.clone();
            let last_rx = last_rx.clone();
            spawn_local(async move {
                match KenwoodDriver::query_frequency(&*serial).await {
                    Ok(resp) => {
                        status.set("Queried frequency".to_string());
                        let mut v = (*response_log).clone();
                        let entry = format!("RX: {}", resp);
                        web_sys::console::log_1(&JsValue::from_str(&format!("app: push query resp {}", entry)));
                        v.push(entry.clone());
                        response_log.set(v);
                        last_rx.set(entry);
                    }
                    Err(e) => status.set(format!("Query failed: {:?}", e)),
                }
            });
        })
    };

    // explicit on-demand read removed; background stream supplies responses

    let connect_class = if *connected { "connected" } else { "" };

    let on_min_change = {
        let min_freq = min_freq.clone();
        let status = status.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<f64>() {
                min_freq.set(value);
                save_freq(STORAGE_MIN_FREQ, value);
            } else {
                status.set("Invalid min frequency".to_string());
            }
        })
    };

    let on_max_change = {
        let max_freq = max_freq.clone();
        let status = status.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<f64>() {
                max_freq.set(value);
                save_freq(STORAGE_MAX_FREQ, value);
            } else {
                status.set("Invalid max frequency".to_string());
            }
        })
    };

    html! {
        <div class="app">
            <div class="header">
                <button class="settings" onclick={on_toggle_settings}>{"⚙"}</button>
                <button class={connect_class} onclick={on_connect} disabled={*connected}>{
                    if *connected { "Connected" } else { "Connect Serial" }
                }</button>
                <button onclick={on_refresh}>{"Refresh"}</button>
                <label>
                    {"Allow tuning from"}
                    <input
                        type="number"
                        step="0.001"
                        value={format!("{:.3}", *min_freq)}
                        oninput={on_min_change}
                    />
                </label>
                <label>
                    {" to "}
                    <input
                        type="number"
                        step="0.001"
                        value={format!("{:.3}", *max_freq)}
                        oninput={on_max_change}
                    />
                </label>
                        <div class="status">{(*status).clone()}</div>
            </div>
            { if *show_settings {
                html! {
                    <div class="settings-panel">
                        <h3>{"Settings"}</h3>
                        <label>{"Raw CAT command (text): "}
                            <input type="text" value={(*raw_cmd).clone()} oninput={on_raw_input} />
                        </label>
                        <button onclick={on_send_raw}>{"Send Raw"}</button>
                        <div class="response-log">
                            <h4>{"Response Log"}</h4>
                            <div class="last-rx">{ format!("Last RX: {}", (*last_rx).clone()) }</div>
                            { for (*response_log).iter().map(|line| html!{ <div class="resp">{ line.clone() }</div> }) }
                        </div>
                        <hr/>
                        <div class="std-commands">
                            <h4>{"Standard Commands"}</h4>
                            <button onclick={on_vfo_a}>{"VFO A"}</button>
                            <button onclick={on_vfo_b}>{"VFO B"}</button>
                            <button onclick={on_query_freq}>{"Query Frequency"}</button>
                            <div class="modes">
                                <button onclick={
                                    {
                                        let cb = on_set_mode.clone();
                                        Callback::from(move |_| cb.emit("USB".to_string()))
                                    }
                                }>{"USB"}</button>
                                <button onclick={
                                    {
                                        let cb = on_set_mode.clone();
                                        Callback::from(move |_| cb.emit("LSB".to_string()))
                                    }
                                }>{"LSB"}</button>
                                <button onclick={
                                    {
                                        let cb = on_set_mode.clone();
                                        Callback::from(move |_| cb.emit("CW".to_string()))
                                    }
                                }>{"CW"}</button>
                                <button onclick={
                                    {
                                        let cb = on_set_mode.clone();
                                        Callback::from(move |_| cb.emit("FM".to_string()))
                                    }
                                }>{"FM"}</button>
                                <button onclick={
                                    {
                                        let cb = on_set_mode.clone();
                                        Callback::from(move |_| cb.emit("AM".to_string()))
                                    }
                                }>{"AM"}</button>
                            </div>
                        </div>
                        <hr/>
                        <button onclick={on_test_14062}>{"14.062 CW"}</button>
                    </div>
                }
            } else { html!{} } }
            <table>
                <thead>
                    <tr>
                        <th>{"Time"}</th>
                        <th>{"Callsign"}</th>
                        <th>{"Summit"}</th>
                        <th class="freq">{"Frequency"}</th>
                        <th>{"Mode"}</th>
                        <th>{"Comments"}</th>
                    </tr>
                </thead>
                <tbody>
                    { for spots.iter().enumerate().map(|(idx, spot)| {
                        let row_class = if Some(idx) == *selected_row { "tuned" } else { "" };
                        let on_row_click = {
                            let on_tune = on_tune.clone();
                            Callback::from(move |_| on_tune.emit(idx))
                        };
                        html! {
                            <tr class={row_class} onclick={on_row_click}>
                                <td>{ format_time(&spot.timestamp) }</td>
                                <td>{ spot.callsign.clone() }</td>
                                <td>{ spot.summit.clone() }</td>
                                <td class="freq">{ format!("{:.4}", spot.frequency_mhz) }</td>
                                <td>{ spot.mode.clone() }</td>
                                <td>{ spot.comments.clone() }</td>
                            </tr>
                        }
                    }) }
                </tbody>
            </table>
        </div>
    }
}
