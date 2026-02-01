use crate::serial::SerialManager;
use gloo_net::http::Request;
use gloo_timers::callback::Interval;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew::events::InputEvent;
use web_sys::{HtmlInputElement, Storage};

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
    let max_freq = use_state(|| 28.0_f64);

    {
        let min_freq = min_freq.clone();
        let max_freq = max_freq.clone();
        use_effect_with((), move |_| {
            min_freq.set(load_freq(STORAGE_MIN_FREQ, 7.0));
            max_freq.set(load_freq(STORAGE_MAX_FREQ, 28.0));
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
